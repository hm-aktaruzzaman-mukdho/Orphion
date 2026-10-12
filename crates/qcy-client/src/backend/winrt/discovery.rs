//! Finding QCY earbuds with `BluetoothLEAdvertisementWatcher`.
//!
//! The logic mirrors the BlueZ backend: collect QCY advertisements
//! (company id `0x521C`), match their embedded classic address against the
//! paired audio devices, and rank the candidates with [`Endpoint::pick`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qcy_protocol::advert::{Advertisement, MacAddress};
use qcy_protocol::uuids::COMPANY_IDS;
use tokio::time::{Instant, sleep};
use tracing::{debug, info, warn};
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
    BluetoothLEScanningMode,
};
use windows::Devices::Bluetooth::{
    BluetoothAdapter, BluetoothAddressType, BluetoothConnectionStatus, BluetoothDevice,
    BluetoothLEDevice,
};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Devices::Radios::RadioState;
use windows::Foundation::TypedEventHandler;
use windows::core::{HRESULT, Ref};

use super::gatt::WinGattLink;
use super::{buffer_bytes, mac_from_u64};
use crate::Error;
use crate::transport::{Backend, Endpoint, Selector};

const POLL_INTERVAL: Duration = Duration::from_millis(300);
/// `HRESULT_FROM_WIN32(ERROR_DEVICE_NOT_READY)`: what the watcher reports
/// while the radio is off.
const ERROR_DEVICE_NOT_READY: HRESULT = HRESULT(0x8007_10DF_u32 as i32);

/// The last advertisement seen from one BLE address.
#[derive(Debug, Clone)]
struct Seen {
    raw: Vec<u8>,
    advertisement: Advertisement,
    random: bool,
    rssi: i16,
    /// When it was received; `None` for entries loaded from the cache file.
    at: Option<std::time::Instant>,
}

/// A paired classic (audio) device, used to name endpoints.
#[derive(Debug, Clone)]
struct ClassicDevice {
    address: MacAddress,
    name: String,
    connected: bool,
}

/// The WinRT Bluetooth LE stack.
#[derive(Clone, Default)]
pub struct WinRt {
    seen: Arc<Mutex<HashMap<u64, Seen>>>,
}

impl Backend for WinRt {
    type Link = WinGattLink;

    async fn init() -> Result<Self, Error> {
        let adapter = default_adapter().await?;
        if !adapter.IsLowEnergySupported()? {
            return Err(Error::Backend(
                "this Bluetooth adapter does not support Bluetooth Low Energy".into(),
            ));
        }
        ensure_radio_on(&adapter).await?;
        let backend = Self::default();
        backend.load_cache();
        Ok(backend)
    }

    async fn scan(&self, duration: Duration) -> Result<Vec<Endpoint>, Error> {
        ensure_radio_on(&default_adapter().await?).await?;
        let started = std::time::Instant::now();
        let scanner = Scanner::start(Arc::clone(&self.seen))?;
        sleep(duration).await;
        drop(scanner);
        self.save_cache();
        let classic = classic_devices().await;
        Ok(self
            .endpoints(&classic, Some(started), false)
            .await
            .into_iter()
            .filter(|e| e.rssi.is_some() || e.ble_connected)
            .collect())
    }

    async fn find(&self, selector: &Selector, timeout: Duration) -> Result<Endpoint, Error> {
        // The backend outlives radio changes (it is reused across reconnects);
        // without this, a scan with the radio off would fall back to a cached
        // endpoint and fail later with a misleading error.
        ensure_radio_on(&default_adapter().await?).await?;
        let classic = classic_devices().await;

        // Earbuds connected by another program do not advertise; check the
        // ones we know about first.
        let known = self.endpoints(&classic, None, true).await;
        if let Some(e) = Endpoint::pick(known, selector, false).filter(|e| e.ble_connected) {
            info!(address = %e.ble_address, "endpoint already connected");
            return Ok(e);
        }

        let started = std::time::Instant::now();
        let scanner = Scanner::start(Arc::clone(&self.seen))?;
        let deadline = Instant::now() + timeout;
        let mut last_seen = None;
        loop {
            let endpoints = self.endpoints(&classic, Some(started), false).await;
            if let Some(e) = Endpoint::pick(endpoints.clone(), selector, true) {
                // Give a better-ranked candidate (e.g. the one connected for
                // audio) a brief chance to show up before settling.
                if e.audio_connected || Instant::now() + Duration::from_secs(2) > deadline {
                    drop(scanner);
                    self.save_cache();
                    info!(address = %e.ble_address, "found endpoint");
                    return Ok(e);
                }
                last_seen = Some(e);
            }
            if Instant::now() >= deadline {
                drop(scanner);
                self.save_cache();
                // Unlike BlueZ, no fallback to remembered endpoints: Windows
                // can only connect to an unpaired device while it advertises,
                // and connected ones were handled above. A silent one is in
                // its case, out of range or held by the phone app.
                return last_seen.ok_or_else(Error::not_found);
            }
            sleep(POLL_INTERVAL).await;
        }
    }

    async fn open(&self, endpoint: &Endpoint) -> Result<WinGattLink, Error> {
        let (address, random) = parse_id(&endpoint.id)?;
        WinGattLink::open(address, random).await
    }
}

impl WinRt {
    /// Endpoints from the advertisements seen so far. Only those received
    /// after `fresh_since` count as currently advertising (have an RSSI).
    /// With `check_connection`, asks Windows whether each is connected.
    async fn endpoints(
        &self,
        classic: &[ClassicDevice],
        fresh_since: Option<std::time::Instant>,
        check_connection: bool,
    ) -> Vec<Endpoint> {
        // Copy out first: the lock must not be held across `.await`.
        let seen: Vec<(u64, Seen)> = self
            .seen
            .lock()
            .map(|m| m.iter().map(|(a, s)| (*a, s.clone())).collect())
            .unwrap_or_default();

        let mut endpoints = Vec::with_capacity(seen.len());
        for (address, s) in seen {
            let fresh = matches!((s.at, fresh_since), (Some(at), Some(since)) if at >= since);
            let ble_connected = check_connection && is_connected(address, s.random).await;
            let audio = classic
                .iter()
                .find(|c| s.advertisement.matches_classic(c.address));
            endpoints.push(Endpoint {
                id: make_id(address, s.random),
                ble_address: mac_from_u64(address),
                name: audio.map(|c| c.name.clone()),
                audio_connected: audio.is_some_and(|c| c.connected),
                ble_connected,
                rssi: fresh.then_some(s.rssi),
                advertisement: s.advertisement,
            });
        }
        endpoints
    }

    fn cache_path() -> Option<PathBuf> {
        let base = std::env::var_os("LOCALAPPDATA")?;
        Some(
            PathBuf::from(base)
                .join("qcy-earbuds")
                .join("endpoints.txt"),
        )
    }

    /// Loads endpoints remembered by earlier runs (`<address> <random> <adv hex>` lines).
    fn load_cache(&self) {
        let Some(text) = Self::cache_path().and_then(|p| std::fs::read_to_string(p).ok()) else {
            return;
        };
        let Ok(mut seen) = self.seen.lock() else {
            return;
        };
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(address), Some(random), Some(raw)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let (Ok(address), Some(raw)) = (u64::from_str_radix(address, 16), from_hex(raw)) else {
                continue;
            };
            if let Ok(advertisement) = Advertisement::parse(&raw) {
                seen.entry(address).or_insert(Seen {
                    raw,
                    advertisement,
                    random: random == "random",
                    rssi: 0,
                    at: None,
                });
            }
        }
    }

    fn save_cache(&self) {
        let Some(path) = Self::cache_path() else {
            return;
        };
        let text: String = match self.seen.lock() {
            Ok(seen) => seen
                .iter()
                .map(|(address, s)| {
                    let kind = if s.random { "random" } else { "public" };
                    format!("{address:012x} {kind} {}\n", to_hex(&s.raw))
                })
                .collect(),
            Err(_) => return,
        };
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, text));
        if let Err(e) = result {
            debug!("cannot save endpoint cache: {e}");
        }
    }
}

/// A running advertisement watcher; stops when dropped.
struct Scanner {
    watcher: BluetoothLEAdvertisementWatcher,
    token: i64,
}

impl Scanner {
    fn start(seen: Arc<Mutex<HashMap<u64, Seen>>>) -> Result<Self, Error> {
        let watcher = BluetoothLEAdvertisementWatcher::new()?;
        // Active scanning also collects scan responses, where some firmwares
        // put their manufacturer data.
        watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;
        let handler = TypedEventHandler::<
            BluetoothLEAdvertisementWatcher,
            BluetoothLEAdvertisementReceivedEventArgs,
        >::new(
            move |_, args: Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
                if let Ok(args) = args.ok() {
                    if let Some((address, s)) = parse_report(args) {
                        if let Ok(mut seen) = seen.lock() {
                            seen.insert(address, s);
                        }
                    }
                }
                Ok(())
            },
        );
        let token = watcher.Received(&handler)?;
        if let Err(e) = watcher.Start() {
            watcher.RemoveReceived(token).ok();
            // The radio was switched off between the check and the scan.
            return Err(if e.code() == ERROR_DEVICE_NOT_READY {
                Error::AdapterOff
            } else {
                e.into()
            });
        }
        debug!("advertisement watcher started");
        Ok(Self { watcher, token })
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        if let Err(e) = self.watcher.Stop() {
            warn!("failed to stop the advertisement watcher: {e}");
        }
        self.watcher.RemoveReceived(self.token).ok();
        debug!("advertisement watcher stopped");
    }
}

/// Extracts a QCY advertisement from a received report, if it is one.
fn parse_report(args: &BluetoothLEAdvertisementReceivedEventArgs) -> Option<(u64, Seen)> {
    let manufacturer_data = args.Advertisement().ok()?.ManufacturerData().ok()?;
    let (raw, advertisement) = manufacturer_data.into_iter().find_map(|md| {
        if !COMPANY_IDS.contains(&md.CompanyId().ok()?) {
            return None;
        }
        let raw = buffer_bytes(&md.Data().ok()?).ok()?;
        let advertisement = Advertisement::parse(&raw).ok()?;
        Some((raw, advertisement))
    })?;
    let seen = Seen {
        raw,
        advertisement,
        random: args.BluetoothAddressType().ok()? == BluetoothAddressType::Random,
        rssi: args.RawSignalStrengthInDBm().unwrap_or(0),
        at: Some(std::time::Instant::now()),
    };
    Some((args.BluetoothAddress().ok()?, seen))
}

async fn default_adapter() -> Result<BluetoothAdapter, Error> {
    BluetoothAdapter::GetDefaultAsync()?
        .await
        .map_err(|_| Error::NoAdapter)
}

/// Fails with [`Error::AdapterOff`] while the Bluetooth radio is off.
async fn ensure_radio_on(adapter: &BluetoothAdapter) -> Result<(), Error> {
    let radio = adapter.GetRadioAsync()?.await?;
    if radio.State()? == RadioState::On {
        Ok(())
    } else {
        Err(Error::AdapterOff)
    }
}

/// Whether Windows currently has a link to the given BLE address.
async fn is_connected(address: u64, random: bool) -> bool {
    let kind = if random {
        BluetoothAddressType::Random
    } else {
        BluetoothAddressType::Public
    };
    let Ok(op) =
        BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(address, kind)
    else {
        return false;
    };
    match op.await {
        Ok(device) => {
            let connected = device
                .ConnectionStatus()
                .is_ok_and(|s| s == BluetoothConnectionStatus::Connected);
            device.Close().ok();
            connected
        }
        Err(_) => false,
    }
}

/// Paired classic devices; failures are logged and yield an empty list
/// (names are a nicety, not a requirement).
async fn classic_devices() -> Vec<ClassicDevice> {
    match try_classic_devices().await {
        Ok(devices) => devices,
        Err(e) => {
            debug!("cannot list paired devices: {e}");
            Vec::new()
        }
    }
}

async fn try_classic_devices() -> windows::core::Result<Vec<ClassicDevice>> {
    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
    let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.await?;
    let mut devices = Vec::new();
    for i in 0..infos.Size()? {
        let id = infos.GetAt(i)?.Id()?;
        let Ok(device) = BluetoothDevice::FromIdAsync(&id)?.await else {
            continue;
        };
        devices.push(ClassicDevice {
            address: mac_from_u64(device.BluetoothAddress()?),
            name: device.Name()?.to_string(),
            connected: device.ConnectionStatus()? == BluetoothConnectionStatus::Connected,
        });
        device.Close().ok();
    }
    Ok(devices)
}

/// Endpoint ids look like `c4ac6089a4b1-random`.
fn make_id(address: u64, random: bool) -> String {
    let kind = if random { "random" } else { "public" };
    format!("{address:012x}-{kind}")
}

fn parse_id(id: &str) -> Result<(u64, bool), Error> {
    let invalid = || Error::Backend(format!("invalid endpoint id {id:?}"));
    let (address, kind) = id.split_once('-').ok_or_else(invalid)?;
    let address = u64::from_str_radix(address, 16).map_err(|_| invalid())?;
    Ok((address, kind == "random"))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_roundtrip() {
        let id = make_id(0xC4AC_6089_A4B1, true);
        assert_eq!(id, "c4ac6089a4b1-random");
        assert_eq!(parse_id(&id).unwrap(), (0xC4AC_6089_A4B1, true));
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = qcy_protocol::samples::ADVERTISEMENT.to_vec();
        assert_eq!(from_hex(&to_hex(&bytes)).unwrap(), bytes);
    }
}
