//! A GATT session with the QCY vendor services, on top of BlueZ.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use qcy_protocol::uuids::{self, Service};
use qcy_protocol::{Characteristic, Write};
use tokio::time::{sleep, timeout};
use tracing::{debug, trace, warn};
use zbus::Connection;
use zbus::fdo::PropertiesProxy;
use zbus::names::InterfaceName;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};

use super::discovery::{managed_objects, prop};
use super::proxies::{
    Device1Proxy, GATT_CHAR_IFACE, GATT_SERVICE_IFACE, GattCharacteristic1Proxy, SERVICE,
};
use crate::Error;
use crate::transport::{Link, Notification, hex};

const CONNECT_ATTEMPTS: u32 = 2;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(45);
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(20);
/// Default ATT MTU if BlueZ does not report one.
const DEFAULT_MTU: u16 = 23;

/// One resolved characteristic.
#[derive(Debug, Clone)]
struct Char {
    proxy: GattCharacteristic1Proxy<'static>,
    /// Supports write without response (otherwise writes are requests).
    write_command: bool,
    notifies: bool,
}

/// An open GATT connection to the earbuds' vendor services.
#[derive(Debug, Clone)]
pub struct GattSession {
    conn: Connection,
    device: Device1Proxy<'static>,
    chars: HashMap<Characteristic, Char>,
    mtu: u16,
}

impl GattSession {
    /// Connects (if needed) and resolves the vendor characteristics.
    pub async fn open(conn: &Connection, device_path: &ObjectPath<'_>) -> Result<Self, Error> {
        let device = Device1Proxy::builder(conn)
            .path(device_path.to_owned())?
            .build()
            .await?;

        connect_with_retry(&device).await?;
        wait_services_resolved(&device).await?;

        let chars = resolve_characteristics(conn, device_path).await?;
        let mut mtu = DEFAULT_MTU;
        if let Some(c) = chars.values().next() {
            mtu = c.proxy.mtu().await.unwrap_or(DEFAULT_MTU);
        }
        debug!(mtu, characteristics = chars.len(), "GATT session ready");

        Ok(Self {
            conn: conn.clone(),
            device,
            chars,
            mtu,
        })
    }

    fn char(&self, c: Characteristic) -> Result<&Char, Error> {
        self.chars
            .get(&c)
            .ok_or_else(|| Error::NotFound(format!("characteristic {c:?} not available")))
    }

    pub async fn is_connected(&self) -> bool {
        self.device.connected().await.unwrap_or(false)
    }
}

impl Link for GattSession {
    /// Largest value that fits into a single ATT write.
    fn max_write_len(&self) -> usize {
        usize::from(self.mtu.saturating_sub(3)).max(20)
    }

    fn has(&self, c: Characteristic) -> bool {
        self.chars.contains_key(&c)
    }

    async fn read(&self, c: Characteristic) -> Result<Vec<u8>, Error> {
        let value = self.char(c)?.proxy.read_value(HashMap::new()).await?;
        trace!(?c, data = %hex(&value), "read");
        Ok(value)
    }

    /// Writes like Android's default write type: without response if the
    /// characteristic supports it, otherwise as a write request (which BlueZ
    /// turns into a long write when the value exceeds the MTU).
    async fn write(&self, w: &Write) -> Result<(), Error> {
        let ch = self.char(w.characteristic)?;
        if ch.write_command && w.data.len() > self.max_write_len() {
            return Err(Error::TooLarge {
                len: w.data.len(),
                max: self.max_write_len(),
            });
        }
        trace!(c = ?w.characteristic, data = %hex(&w.data), "write");
        let kind = if ch.write_command {
            "command"
        } else {
            "request"
        };
        let options = HashMap::from([("type", Value::from(kind))]);
        ch.proxy.write_value(&w.data, options).await?;
        Ok(())
    }

    /// Enables notifications and returns a stream of everything received.
    async fn notifications(&self) -> Result<BoxStream<'static, Notification>, Error> {
        let mut streams = Vec::new();
        for (&c, ch) in self.chars.iter().filter(|(_, ch)| ch.notifies) {
            let proxy = &ch.proxy;
            let props = PropertiesProxy::builder(&self.conn)
                .destination(SERVICE)?
                .path(proxy.inner().path().to_owned())?
                .build()
                .await?;
            // Subscribe before enabling notifications so nothing is lost.
            let signals = props.receive_properties_changed().await?;
            if let Err(e) = proxy.start_notify().await {
                warn!(?c, "StartNotify failed: {e}");
                continue;
            }
            streams.push(
                signals
                    .filter_map(move |signal| async move {
                        let args = signal.args().ok()?;
                        if args.interface_name()
                            != &InterfaceName::from_static_str_unchecked(GATT_CHAR_IFACE)
                        {
                            return None;
                        }
                        let value = args.changed_properties().get("Value")?;
                        let data = Vec::<u8>::try_from(value.try_clone().ok()?).ok()?;
                        trace!(?c, data = %hex(&data), "notification");
                        Some(Notification {
                            characteristic: c,
                            data,
                        })
                    })
                    .boxed(),
            );
        }
        Ok(stream::select_all(streams).boxed())
    }

    /// Stream that yields once the BLE link goes down.
    async fn disconnected(&self) -> Result<BoxStream<'static, ()>, Error> {
        let stream = self.device.receive_connected_changed().await;
        Ok(stream
            .filter_map(|change| async move {
                match change.get().await {
                    Ok(false) => Some(()),
                    _ => None,
                }
            })
            .boxed())
    }

    /// Drops the BLE control link (audio over classic Bluetooth is unaffected).
    async fn disconnect(&self) -> Result<(), Error> {
        self.device.disconnect().await?;
        Ok(())
    }
}

/// Connects the BLE link.
///
/// LE connections to the buds can take well over 20 s while they are busy
/// streaming audio, and a D-Bus `Connect` call may time out while BlueZ keeps
/// trying in the background. So we never cancel a pending attempt: we issue
/// `Connect` and in parallel watch the `Connected` property, whichever
/// finishes first wins.
async fn connect_with_retry(device: &Device1Proxy<'_>) -> Result<(), Error> {
    for attempt in 1..=CONNECT_ATTEMPTS {
        if device.connected().await.unwrap_or(false) {
            return Ok(());
        }
        debug!(attempt, "connecting");
        let call = async {
            match device.connect().await {
                Ok(()) => Ok(()),
                // Already being connected (by an earlier call or another
                // client): let the property watcher report the outcome.
                Err(zbus::Error::MethodError(name, _, _))
                    if name.as_str() == "org.bluez.Error.InProgress" =>
                {
                    std::future::pending().await
                }
                Err(e) => Err(Error::from(e)),
            }
        };
        let watch = async {
            while !device.connected().await.unwrap_or(false) {
                sleep(Duration::from_millis(250)).await;
            }
            Ok(())
        };
        let outcome = timeout(CONNECT_TIMEOUT, async {
            tokio::select! {
                r = call => r,
                r = watch => r,
            }
        })
        .await
        .unwrap_or(Err(Error::Timeout("connecting to the earbuds")));

        match outcome {
            Ok(()) => return Ok(()),
            // BlueZ gave up on the device and removed its (temporary) object:
            // retrying this path is pointless, the caller has to rescan.
            Err(e) if is_gone(&e) => return Err(Error::DeviceGone),
            Err(e) if attempt < CONNECT_ATTEMPTS => {
                warn!(attempt, "connect failed: {e}; retrying");
                sleep(Duration::from_millis(500 * u64::from(attempt))).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("the loop returns on the last attempt")
}

/// Whether BlueZ aborted the connection or forgot the device object.
fn is_gone(e: &Error) -> bool {
    let text = e.to_string();
    text.contains("UnknownObject") || text.contains("le-connection-abort-by-local")
}

async fn wait_services_resolved(device: &Device1Proxy<'_>) -> Result<(), Error> {
    let poll = async {
        loop {
            if device.services_resolved().await.unwrap_or(false) {
                return;
            }
            sleep(Duration::from_millis(200)).await;
        }
    };
    timeout(RESOLVE_TIMEOUT, poll)
        .await
        .map_err(|_| Error::Timeout("discovering GATT services"))
}

/// Finds the characteristics of the vendor services (`0xA001`, and `0xA002`
/// on JieLi models) below `device_path`.
///
/// The earbuds may also expose a firmware-update service (WuQi `0x7033`)
/// whose characteristics re-use the UUIDs `0x1001`/`0x1002`, so matching on
/// the parent service is essential: anything outside [`Service`] is ignored.
async fn resolve_characteristics(
    conn: &Connection,
    device_path: &ObjectPath<'_>,
) -> Result<HashMap<Characteristic, Char>, Error> {
    let objects = managed_objects(conn).await?;
    let prefix = format!("{}/", device_path.as_str());

    let services: HashMap<OwnedObjectPath, Service> = objects
        .iter()
        .filter(|(path, _)| path.as_str().starts_with(&prefix))
        .filter_map(|(path, ifaces)| {
            let uuid: String = prop(ifaces.get(GATT_SERVICE_IFACE)?, "UUID")?;
            Some((path.clone(), Service::from_uuid(&uuid)?))
        })
        .collect();
    if !services.values().any(|s| *s == Service::Control) {
        return Err(Error::NotFound(
            "the device does not expose the QCY control service (0xA001)".into(),
        ));
    }

    let mut chars = HashMap::new();
    for (path, ifaces) in &objects {
        let Some(props) = ifaces.get(GATT_CHAR_IFACE) else {
            continue;
        };
        let Some(&service) = prop::<OwnedObjectPath>(props, "Service")
            .as_ref()
            .and_then(|s| services.get(s))
        else {
            continue;
        };
        let Some(id) = prop::<String>(props, "UUID").and_then(|u| uuids::shorten(&u)) else {
            continue;
        };
        let flags: Vec<String> = prop(props, "Flags").unwrap_or_default();
        let has = |f: &str| flags.iter().any(|x| x == f);
        let proxy = GattCharacteristic1Proxy::builder(conn)
            .path(path.clone())?
            .build()
            .await?;
        chars.insert(
            Characteristic::new(service, id),
            Char {
                proxy,
                write_command: has("write-without-response"),
                notifies: has("notify") || has("indicate"),
            },
        );
    }
    debug!(chars = ?chars.keys().collect::<Vec<_>>(), "resolved characteristics");
    Ok(chars)
}
