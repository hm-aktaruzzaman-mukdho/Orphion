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
    GattCommunicationStatus, GattDeviceService, GattSession, GattValueChangedEventArgs,
    GattWriteOption,
};
use windows::Devices::Bluetooth::{
    BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
};
use windows::Foundation::TypedEventHandler;
use windows::core::{IInspectable, Ref};

use super::{buffer_bytes, guid, to_buffer};
use crate::Error;
use crate::transport::{Link, Notification, hex};

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
    handlers: Mutex<Handlers>,
}

/// Registered event handlers, removed when the link is released.
#[derive(Default)]
struct Handlers {
    value_changed: Vec<(GattCharacteristic, i64)>,
    connection_status: Option<i64>,
    released: bool,
}

impl WinGattLink {
    /// Connects to the device and resolves the control characteristics.
    pub async fn open(address: u64, random: bool) -> Result<Self, Error> {
        let kind = if random {
            BluetoothAddressType::Random
        } else {
            BluetoothAddressType::Public
        };
        let device =
            BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(address, kind)?
                .await
                .map_err(|_| Error::DeviceGone)?;
        let service = connect_service(&device).await?;

        // Keep the link up while this program holds it, even when idle.
        let session = service.Session()?;
        session.SetMaintainConnection(true)?;

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
        debug!(
            mtu = session.MaxPduSize().unwrap_or(DEFAULT_MTU),
            characteristics = chars.len(),
            "GATT session ready"
        );

        Ok(Self {
            inner: Arc::new(Inner {
                device,
                service,
                session,
                chars,
                handlers: Mutex::default(),
            }),
        })
    }

    fn char(&self, c: Characteristic) -> Result<&GattCharacteristic, Error> {
        self.inner
            .chars
            .get(&c)
            .ok_or_else(|| Error::NotFound(format!("characteristic {c:?} not available")))
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

    async fn notifications(&self) -> Result<BoxStream<'static, Notification>, Error> {
        let (tx, rx) = mpsc::unbounded_channel();
        for (&c, ch) in self.inner.chars.iter().filter(|(c, _)| c.notifies()) {
            // Register the handler before enabling notifications so nothing is
            // lost. The delegate is not `Send`, so it lives in its own scope.
            let tx = tx.clone();
            let token = {
                let handler =
                    TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(
                        move |_, args: Ref<GattValueChangedEventArgs>| {
                            if let Ok(args) = args.ok() {
                                match args.CharacteristicValue().and_then(|b| buffer_bytes(&b)) {
                                    Ok(data) => {
                                        trace!(?c, data = %hex(&data), "notification");
                                        tx.send(Notification {
                                            characteristic: c,
                                            data,
                                        })
                                        .ok();
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
            if status != GattCommunicationStatus::Success {
                warn!(?c, ?status, "enabling notifications failed");
            }
        }
        Ok(stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|n| (n, rx)) }).boxed())
    }

    async fn disconnected(&self) -> Result<BoxStream<'static, ()>, Error> {
        let (tx, rx) = mpsc::unbounded_channel();
        let handler = TypedEventHandler::<BluetoothLEDevice, IInspectable>::new(
            move |device: Ref<BluetoothLEDevice>, _| {
                if let Ok(device) = device.ok() {
                    if device.ConnectionStatus()? == BluetoothConnectionStatus::Disconnected {
                        tx.send(()).ok();
                    }
                }
                Ok(())
            },
        );
        let token = self.inner.device.ConnectionStatusChanged(&handler)?;
        if let Ok(mut h) = self.inner.handlers.lock() {
            h.connection_status = Some(token);
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

/// Finds the control service; on Windows this is what opens the link, so
/// slow or failed attempts are retried like a connection.
async fn connect_service(device: &BluetoothLEDevice) -> Result<GattDeviceService, Error> {
    let target = guid(uuids::SERVICE);
    for attempt in 1..=CONNECT_ATTEMPTS {
        debug!(attempt, "discovering services");
        let operation = device
            .GetGattServicesForUuidWithCacheModeAsync(target, BluetoothCacheMode::Uncached)?;
        match timeout(CONNECT_TIMEOUT, operation).await {
            Ok(Ok(result)) => match result.Status()? {
                GattCommunicationStatus::Success => {
                    return result.Services()?.into_iter().next().ok_or_else(|| {
                        Error::NotFound(
                            "the device does not expose the QCY control service (0xA001)".into(),
                        )
                    });
                }
                status => warn!(attempt, ?status, "service discovery failed"),
            },
            Ok(Err(e)) => warn!(attempt, "service discovery failed: {e}"),
            Err(_) => warn!(attempt, "service discovery timed out"),
        }
        sleep(Duration::from_millis(500 * attempt)).await;
    }
    Err(Error::DeviceGone)
}

/// Maps a GATT status to a result.
fn check(status: GattCommunicationStatus, what: &str) -> Result<(), Error> {
    match status {
        GattCommunicationStatus::Success => Ok(()),
        GattCommunicationStatus::Unreachable => Err(Error::NotConnected),
        other => Err(Error::Backend(format!("{what} failed: {other:?}"))),
    }
}
