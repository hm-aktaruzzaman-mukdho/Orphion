//! A GATT connection to the QCY control service through WinRT.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use qcy_protocol::{Characteristic, Write, uuids};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tracing::{debug, trace, warn};
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
    GattCommunicationStatus, GattDeviceService, GattOpenStatus, GattSession, GattSharingMode,
    GattValueChangedEventArgs, GattWriteOption,
};
use windows::Devices::Bluetooth::{
    BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
};
use windows::Foundation::TypedEventHandler;
use windows::core::{IInspectable, Ref};

use super::{buffer_bytes, guid, to_buffer};
use crate::Error;
use crate::transport::{Link, Notification, hex};

/// Complete setups (service discovery up to enabled notifications) tried
/// before giving up.
const CONNECT_ATTEMPTS: u64 = 3;
/// Service discovery doubles as the connection attempt on Windows.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default ATT MTU if Windows does not report one.
const DEFAULT_MTU: u16 = 23;

/// An open connection to the earbuds' control service. Cheap to clone.
#[derive(Clone)]
pub struct WinGattLink {
    inner: Arc<Inner>,
}

struct Inner {
    device: BluetoothLEDevice,
    service: GattDeviceService,
    session: GattSession,
    /// Characteristics of service `0xA001` only (never the OTA service).
    chars: HashMap<Characteristic, GattCharacteristic>,
    subscribers: Arc<Subscribers>,
    handlers: Mutex<Handlers>,
}

/// Receivers of the events forwarded by the WinRT handlers.
#[derive(Default)]
struct Subscribers {
    notifications: Mutex<Vec<mpsc::UnboundedSender<Notification>>>,
    disconnected: Mutex<Vec<mpsc::UnboundedSender<()>>>,
}

impl Subscribers {
    fn notify(&self, n: &Notification) {
        if let Ok(mut subs) = self.notifications.lock() {
            subs.retain(|tx| tx.send(n.clone()).is_ok());
        }
    }

    fn link_lost(&self) {
        if let Ok(mut subs) = self.disconnected.lock() {
            subs.retain(|tx| tx.send(()).is_ok());
        }
    }
}

/// Registered event handlers, removed when the link is released.
#[derive(Default)]
struct Handlers {
    value_changed: Vec<(GattCharacteristic, i64)>,
    connection_status: Option<i64>,
    released: bool,
}

impl WinGattLink {
    /// Connects to the device, resolves the control characteristics and
    /// enables their notifications.
    ///
    /// The whole setup is retried with a fresh device object: Windows keeps
    /// an idle link up for a few seconds after the last program released it,
    /// and a program attaching just then sees the link vanish mid-setup
    /// (`ERROR_CANCELLED`, `Unreachable`).
    pub async fn open(address: u64, random: bool) -> Result<Self, Error> {
        let kind = if random {
            BluetoothAddressType::Random
        } else {
            BluetoothAddressType::Public
        };
        for attempt in 1..=CONNECT_ATTEMPTS {
            debug!(attempt, "opening GATT link");
            match Self::try_open(address, kind).await {
                Ok(link) => return Ok(link),
                Err(e) if attempt < CONNECT_ATTEMPTS && e.is_transient() => {
                    warn!(attempt, "GATT setup failed: {e}; retrying");
                    sleep(Duration::from_millis(500 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!("the loop returns on the last attempt")
    }

    async fn try_open(address: u64, kind: BluetoothAddressType) -> Result<Self, Error> {
        let device =
            BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(address, kind)?
                .await
                .map_err(|_| Error::DeviceGone)?;
        let service = match discover_service(&device).await {
            Ok(service) => service,
            Err(e) => {
                device.Close().ok();
                return Err(e);
            }
        };
        let setup = async {
            // An implicitly opened service is exclusive to this process; open
            // it shared so the tray and the window can use it side by side.
            open_shared(&service).await?;
            // Keep the link up while this program holds it, even when idle.
            let session = service.Session()?;
            session.SetMaintainConnection(true)?;
            let chars = control_characteristics(&service).await?;
            Ok::<_, Error>((session, chars))
        };
        let (session, chars) = match setup.await {
            Ok(parts) => parts,
            Err(e) => {
                service.Close().ok();
                device.Close().ok();
                return Err(e);
            }
        };
        debug!(
            mtu = session.MaxPduSize().unwrap_or(DEFAULT_MTU),
            characteristics = chars.len(),
            "GATT session ready"
        );

        // From here on dropping `link` releases everything.
        let link = Self {
            inner: Arc::new(Inner {
                device,
                service,
                session,
                chars,
                subscribers: Arc::default(),
                handlers: Mutex::default(),
            }),
        };
        link.watch_connection()?;
        link.enable_notifications().await?;
        Ok(link)
    }

    fn char(&self, c: Characteristic) -> Result<&GattCharacteristic, Error> {
        self.inner
            .chars
            .get(&c)
            .ok_or_else(|| Error::NotFound(format!("characteristic {c:?} not available")))
    }

    /// Forwards link loss to the subscribers of [`Link::disconnected`].
    fn watch_connection(&self) -> Result<(), Error> {
        let subscribers = Arc::clone(&self.inner.subscribers);
        // The delegate is not `Send`; it must be gone before any `.await`.
        let token = {
            let handler = TypedEventHandler::<BluetoothLEDevice, IInspectable>::new(
                move |device: Ref<BluetoothLEDevice>, _| {
                    if let Ok(device) = device.ok() {
                        if device.ConnectionStatus()? == BluetoothConnectionStatus::Disconnected {
                            debug!("Windows reports the link as disconnected");
                            subscribers.link_lost();
                        }
                    }
                    Ok(())
                },
            );
            self.inner.device.ConnectionStatusChanged(&handler)?
        };
        if let Ok(mut h) = self.inner.handlers.lock() {
            h.connection_status = Some(token);
        }
        Ok(())
    }

    /// Registers value handlers and writes the CCCDs of every notifying
    /// characteristic. Fails if the link is unusable, so `open` can retry.
    async fn enable_notifications(&self) -> Result<(), Error> {
        for (&c, ch) in self.inner.chars.iter().filter(|(c, _)| c.notifies()) {
            // Register the handler before enabling notifications so nothing is
            // lost. The delegate is not `Send`, so it lives in its own scope.
            let subscribers = Arc::clone(&self.inner.subscribers);
            let token = {
                let handler =
                    TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(
                        move |_, args: Ref<GattValueChangedEventArgs>| {
                            if let Ok(args) = args.ok() {
                                match args.CharacteristicValue().and_then(|b| buffer_bytes(&b)) {
                                    Ok(data) => {
                                        trace!(?c, data = %hex(&data), "notification");
                                        subscribers.notify(&Notification {
                                            characteristic: c,
                                            data,
                                        });
                                    }
                                    Err(e) => warn!(?c, "unreadable notification: {e}"),
                                }
                            }
                            Ok(())
                        },
                    );
                ch.ValueChanged(&handler)?
            };
            if let Ok(mut h) = self.inner.handlers.lock() {
                h.value_changed.push((ch.clone(), token));
            }
            let status = ch
                .WriteClientCharacteristicConfigurationDescriptorAsync(
                    GattClientCharacteristicConfigurationDescriptorValue::Notify,
                )?
                .await?;
            match status {
                GattCommunicationStatus::Success => {}
                GattCommunicationStatus::Unreachable => return Err(Error::NotConnected),
                status => warn!(?c, ?status, "enabling notifications failed"),
            }
        }
        Ok(())
    }
}

impl Link for WinGattLink {
    fn max_write_len(&self) -> usize {
        let mtu = self.inner.session.MaxPduSize().unwrap_or(DEFAULT_MTU);
        usize::from(mtu.saturating_sub(3)).max(20)
    }

    fn has(&self, c: Characteristic) -> bool {
        self.inner.chars.contains_key(&c)
    }

    async fn read(&self, c: Characteristic) -> Result<Vec<u8>, Error> {
        let result = self
            .char(c)?
            .ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        check(result.Status()?, "reading")?;
        let data = buffer_bytes(&result.Value()?)?;
        trace!(?c, data = %hex(&data), "read");
        Ok(data)
    }

    async fn write(&self, w: &Write) -> Result<(), Error> {
        if w.data.len() > self.max_write_len() {
            return Err(Error::TooLarge {
                len: w.data.len(),
                max: self.max_write_len(),
            });
        }
        trace!(c = ?w.characteristic, data = %hex(&w.data), "write");
        // The buffer is not `Send`; it must be gone before awaiting.
        let operation = {
            let buffer = to_buffer(&w.data)?;
            self.char(w.characteristic)?
                .WriteValueWithResultAndOptionAsync(
                    &buffer,
                    GattWriteOption::WriteWithoutResponse,
                )?
        };
        let result = operation.await?;
        check(result.Status()?, "writing")
    }

    /// Notifications were enabled by `open`; this only subscribes.
    async fn notifications(&self) -> Result<BoxStream<'static, Notification>, Error> {
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut subs) = self.inner.subscribers.notifications.lock() {
            subs.push(tx);
        }
        Ok(stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|n| (n, rx)) }).boxed())
    }

    async fn disconnected(&self) -> Result<BoxStream<'static, ()>, Error> {
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut subs) = self.inner.subscribers.disconnected.lock() {
            subs.push(tx);
        }
        Ok(stream::unfold(
            rx,
            |mut rx| async move { rx.recv().await.map(|()| ((), rx)) },
        )
        .boxed())
    }

    async fn disconnect(&self) -> Result<(), Error> {
        self.inner.release();
        Ok(())
    }
}

impl Inner {
    /// Unregisters handlers and closes every object; Windows then drops the
    /// link once no other program uses it. Idempotent.
    fn release(&self) {
        let Ok(mut h) = self.handlers.lock() else {
            return;
        };
        if h.released {
            return;
        }
        h.released = true;
        for (ch, token) in h.value_changed.drain(..) {
            ch.RemoveValueChanged(token).ok();
        }
        if let Some(token) = h.connection_status.take() {
            self.device.RemoveConnectionStatusChanged(token).ok();
        }
        self.session.SetMaintainConnection(false).ok();
        self.session.Close().ok();
        self.service.Close().ok();
        self.device.Close().ok();
        debug!("GATT link released");
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.release();
    }
}

/// Finds the control service; on Windows this is what opens the link.
async fn discover_service(device: &BluetoothLEDevice) -> Result<GattDeviceService, Error> {
    let operation = device.GetGattServicesForUuidWithCacheModeAsync(
        guid(uuids::SERVICE),
        BluetoothCacheMode::Uncached,
    )?;
    let result = timeout(CONNECT_TIMEOUT, operation)
        .await
        .map_err(|_| Error::Timeout("discovering GATT services"))??;
    match result.Status()? {
        GattCommunicationStatus::Success => {
            result.Services()?.into_iter().next().ok_or_else(|| {
                Error::NotFound(
                    "the device does not expose the QCY control service (0xA001)".into(),
                )
            })
        }
        // Not advertising (in the case, connected elsewhere) or out of range.
        GattCommunicationStatus::Unreachable => Err(Error::DeviceGone),
        status => Err(Error::Backend(format!(
            "service discovery failed: {}",
            status_name(status)
        ))),
    }
}

/// Opens `service` for shared reading and writing.
async fn open_shared(service: &GattDeviceService) -> Result<(), Error> {
    let status = service
        .OpenAsync(GattSharingMode::SharedReadAndWrite)?
        .await?;
    match status {
        GattOpenStatus::Success | GattOpenStatus::AlreadyOpened => Ok(()),
        GattOpenStatus::SharingViolation => Err(Error::Backend(
            "another program holds the earbuds' control service exclusively".into(),
        )),
        other => Err(Error::Backend(format!(
            "opening the control service failed: {}",
            open_status_name(other)
        ))),
    }
}

/// The characteristics of the control service, by role. Only `service`
/// (`0xA001`) is searched, so the OTA service's identically named
/// characteristics can never be reached.
async fn control_characteristics(
    service: &GattDeviceService,
) -> Result<HashMap<Characteristic, GattCharacteristic>, Error> {
    let result = service
        .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?
        .await?;
    check(result.Status()?, "listing characteristics")?;
    let mut chars = HashMap::new();
    for ch in result.Characteristics()? {
        let uuid = ch.Uuid()?.to_u128();
        if let Some(c) = Characteristic::ALL
            .into_iter()
            .find(|c| guid(c.uuid()).to_u128() == uuid)
        {
            chars.insert(c, ch);
        }
    }
    for required in [Characteristic::Command, Characteristic::Settings] {
        if !chars.contains_key(&required) {
            return Err(Error::NotFound(format!(
                "the device does not expose the QCY control characteristic {required:?}"
            )));
        }
    }
    Ok(chars)
}

/// Maps a GATT status to a result.
fn check(status: GattCommunicationStatus, what: &str) -> Result<(), Error> {
    match status {
        GattCommunicationStatus::Success => Ok(()),
        GattCommunicationStatus::Unreachable => Err(Error::NotConnected),
        other => Err(Error::Backend(format!(
            "{what} failed: {}",
            status_name(other)
        ))),
    }
}

/// Readable names; the WinRT enums only debug-print their number.
fn status_name(status: GattCommunicationStatus) -> String {
    match status {
        GattCommunicationStatus::Success => "success".into(),
        GattCommunicationStatus::Unreachable => "unreachable".into(),
        GattCommunicationStatus::ProtocolError => "protocol error".into(),
        GattCommunicationStatus::AccessDenied => "access denied".into(),
        other => format!("status {}", other.0),
    }
}

fn open_status_name(status: GattOpenStatus) -> String {
    match status {
        GattOpenStatus::Success => "success".into(),
        GattOpenStatus::AlreadyOpened => "already opened".into(),
        GattOpenStatus::NotFound => "not found".into(),
        GattOpenStatus::SharingViolation => "sharing violation".into(),
        GattOpenStatus::AccessDenied => "access denied".into(),
        other => format!("status {}", other.0),
    }
}
