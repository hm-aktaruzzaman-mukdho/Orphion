//! Locating the BLE control endpoint of a pair of QCY earbuds.
//!
//! QCY earbuds are two Bluetooth devices from BlueZ's point of view:
//!
//! * the **classic** device you pair for audio (e.g. `84:AC:60:89:A4:B0`), and
//! * a **BLE** device on a random address (e.g. `C4:AC:60:89:A4:B1`) that
//!   carries the control service. It is never paired; it only advertises
//!   manufacturer data with company id `0x521C`.
//!
//! The advertisement contains the classic address, which lets us tie the two
//! together and pick "the earbuds you are listening to" automatically.

use std::collections::HashMap;
use std::time::Duration;

use qcy_protocol::advert::{Advertisement, MacAddress};
use qcy_protocol::uuids::COMPANY_IDS;
use tokio::time::{Instant, sleep};
use tracing::{debug, info, warn};
use zbus::Connection;
use zbus::fdo::{ManagedObjects, ObjectManagerProxy};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use super::gatt::GattSession;
use super::proxies::{ADAPTER_IFACE, Adapter1Proxy, DEVICE_IFACE, SERVICE};
use crate::Error;
use crate::transport::{Backend, Endpoint, Selector};

const AUDIO_SINK_UUID: &str = "0000110b-0000-1000-8000-00805f9b34fb";
const POLL_INTERVAL: Duration = Duration::from_millis(300);

/// A paired classic (audio) device, used to name endpoints.
#[derive(Debug, Clone)]
struct ClassicDevice {
    address: MacAddress,
    name: String,
    connected: bool,
}

/// Connection to BlueZ on the system bus, bound to one adapter.
#[derive(Debug, Clone)]
pub struct Bluez {
    conn: Connection,
    adapter: OwnedObjectPath,
}

impl Bluez {
    /// Connects to the system bus and picks the first (preferably powered) adapter.
    pub async fn connect() -> Result<Self, Error> {
        let conn = Connection::system().await?;
        let objects = managed_objects(&conn).await?;
        let mut adapters: Vec<_> = objects
            .iter()
            .filter_map(|(path, ifaces)| {
                let props = ifaces.get(ADAPTER_IFACE)?;
                Some((
                    path.clone(),
                    prop::<bool>(props, "Powered").unwrap_or(false),
                ))
            })
            .collect();
        adapters.sort_by_key(|(path, powered)| (!*powered, path.to_string()));
        let (adapter, powered) = adapters.into_iter().next().ok_or(Error::NoAdapter)?;
        if !powered {
            return Err(Error::AdapterOff);
        }
        debug!(%adapter, "using adapter");
        Ok(Self { conn, adapter })
    }

    /// QCY endpoints BlueZ currently knows about (no scanning).
    pub async fn known_endpoints(&self) -> Result<Vec<Endpoint>, Error> {
        let objects = managed_objects(&self.conn).await?;
        let classic = classic_devices(&objects);
        let prefix = format!("{}/", self.adapter.as_str());

        let mut endpoints: Vec<Endpoint> = objects
            .iter()
            .filter(|(path, _)| path.as_str().starts_with(&prefix))
            .filter_map(|(path, ifaces)| {
                let props = ifaces.get(DEVICE_IFACE)?;
                let advertisement = qcy_advertisement(props)?;
                let ble_address = prop::<String>(props, "Address")?.parse().ok()?;
                let audio = classic
                    .iter()
                    .find(|c| advertisement.matches_classic(c.address));
                Some(Endpoint {
                    id: path.to_string(),
                    ble_address,
                    name: audio.map(|c| c.name.clone()),
                    audio_connected: audio.is_some_and(|c| c.connected),
                    ble_connected: prop::<bool>(props, "Connected").unwrap_or(false),
                    rssi: prop::<i16>(props, "RSSI"),
                    advertisement,
                })
            })
            .collect();
        endpoints.sort_by_key(|e| std::cmp::Reverse(e.rank()));
        Ok(endpoints)
    }

    async fn start_scan(&self) -> Result<ScanGuard, Error> {
        let adapter: Adapter1Proxy<'static> = Adapter1Proxy::builder(&self.conn)
            .path(self.adapter.clone())?
            .build()
            .await?;
        if !adapter.powered().await? {
            return Err(Error::AdapterOff);
        }
        let filter = HashMap::from([
            ("Transport", Value::from("le")),
            ("DuplicateData", Value::from(true)),
        ]);
        adapter.set_discovery_filter(filter).await?;
        match adapter.start_discovery().await {
            Ok(()) => {}
            Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "org.bluez.Error.InProgress" => {}
            Err(e) => return Err(e.into()),
        }
        debug!("discovery started");
        Ok(ScanGuard {
            adapter: Some(adapter),
        })
    }
}

impl Backend for Bluez {
    type Link = GattSession;

    async fn init() -> Result<Self, Error> {
        Self::connect().await
    }

    async fn scan(&self, duration: Duration) -> Result<Vec<Endpoint>, Error> {
        let scan = self.start_scan().await?;
        sleep(duration).await;
        let endpoints = self.known_endpoints().await?;
        scan.stop().await;
        Ok(endpoints
            .into_iter()
            .filter(|e| e.rssi.is_some() || e.ble_connected)
            .collect())
    }

    async fn find(&self, selector: &Selector, timeout: Duration) -> Result<Endpoint, Error> {
        if let Some(e) = Endpoint::pick(self.known_endpoints().await?, selector, true)
            .filter(|e| e.ble_connected)
        {
            info!(address = %e.ble_address, "endpoint already connected");
            return Ok(e);
        }

        let scan = self.start_scan().await?;
        let deadline = Instant::now() + timeout;
        let mut last_seen = None;
        loop {
            let endpoints = self.known_endpoints().await?;
            if let Some(e) = Endpoint::pick(endpoints.clone(), selector, true) {
                // Give a better-ranked candidate (e.g. the one connected for
                // audio) a brief chance to show up before settling.
                if e.audio_connected || Instant::now() + Duration::from_secs(2) > deadline {
                    scan.stop().await;
                    info!(address = %e.ble_address, "found endpoint");
                    return Ok(e);
                }
                last_seen = Some(e);
            }
            if Instant::now() >= deadline {
                scan.stop().await;
                // Fall back to a fresh but lower ranked one, then to BlueZ's cache.
                return last_seen
                    .or_else(|| Endpoint::pick(endpoints, selector, false))
                    .ok_or_else(Error::not_found);
            }
            sleep(POLL_INTERVAL).await;
        }
    }

    async fn open(&self, endpoint: &Endpoint) -> Result<GattSession, Error> {
        let path = ObjectPath::try_from(endpoint.id.as_str())
            .map_err(|e| Error::Backend(format!("invalid device path {:?}: {e}", endpoint.id)))?;
        GattSession::open(&self.conn, &path).await
    }
}

/// An active discovery session. Call [`ScanGuard::stop`] when done; dropping
/// it stops discovery in the background as a fallback (e.g. on errors).
///
/// Stopping *before* connecting matters: BlueZ may stall an LE connection
/// attempt while the same client keeps scanning.
struct ScanGuard {
    adapter: Option<Adapter1Proxy<'static>>,
}

impl ScanGuard {
    async fn stop(mut self) {
        if let Some(adapter) = self.adapter.take() {
            if let Err(e) = adapter.stop_discovery().await {
                warn!("failed to stop discovery: {e}");
            }
            debug!("discovery stopped");
        }
    }
}

impl Drop for ScanGuard {
    fn drop(&mut self) {
        let Some(adapter) = self.adapter.take() else {
            return;
        };
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(e) = adapter.stop_discovery().await {
                    warn!("failed to stop discovery: {e}");
                }
            });
        }
    }
}

pub(crate) async fn managed_objects(conn: &Connection) -> Result<ManagedObjects, Error> {
    let om = ObjectManagerProxy::builder(conn)
        .destination(SERVICE)?
        .path("/")?
        .build()
        .await?;
    Ok(om.get_managed_objects().await?)
}

fn classic_devices(objects: &ManagedObjects) -> Vec<ClassicDevice> {
    objects
        .values()
        .filter_map(|ifaces| {
            let props = ifaces.get(DEVICE_IFACE)?;
            let paired = prop::<bool>(props, "Paired").unwrap_or(false);
            let uuids = prop::<Vec<String>>(props, "UUIDs").unwrap_or_default();
            if !paired
                || !uuids
                    .iter()
                    .any(|u| u.eq_ignore_ascii_case(AUDIO_SINK_UUID))
            {
                return None;
            }
            Some(ClassicDevice {
                address: prop::<String>(props, "Address")?.parse().ok()?,
                name: prop::<String>(props, "Alias").or_else(|| prop(props, "Name"))?,
                connected: prop::<bool>(props, "Connected").unwrap_or(false),
            })
        })
        .collect()
}

fn qcy_advertisement(props: &HashMap<String, OwnedValue>) -> Option<Advertisement> {
    let data = props.get("ManufacturerData")?;
    let map = HashMap::<u16, OwnedValue>::try_from(data.try_clone().ok()?).ok()?;
    COMPANY_IDS.iter().find_map(|id| {
        let bytes = Vec::<u8>::try_from(map.get(id)?.try_clone().ok()?).ok()?;
        Advertisement::parse(&bytes).ok()
    })
}

/// Extracts a typed property from a BlueZ property map.
pub(crate) fn prop<T>(props: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    T::try_from(props.get(key)?.try_clone().ok()?).ok()
}
