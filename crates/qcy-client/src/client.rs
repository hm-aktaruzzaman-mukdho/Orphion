//! High level, stateful client for one pair of earbuds.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use qcy_protocol::profile::DeviceProfile;
use qcy_protocol::state::DeviceState;
use qcy_protocol::{Characteristic, Command, Update, update};
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::transport::{Backend, Endpoint, Link, Notification, hex};
use crate::{Error, PlatformLink};

/// Pause between consecutive writes. Writes are unacknowledged, and the
/// official app also sends them strictly one after another.
const WRITE_GAP: Duration = Duration::from_millis(40);
/// Delay before asking the earbuds to confirm a change. Measured on a
/// MeloBuds Pro: an ANC switch is not reported back until ~0.5 s later.
pub const CONFIRM_DELAY: Duration = Duration::from_millis(1000);
/// Band count used by the app when the firmware does not report one.
const DEFAULT_MAX_EQ_BANDS: usize = 20;

/// How to verify that a command took effect.
#[derive(Debug, Clone, Copy)]
enum Confirm {
    Request(qcy_protocol::CommandId),
    Read(Characteristic),
}

/// Link state changes reported by [`Earbuds::watch_connection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEvent {
    Connected,
    Disconnected,
}

/// A connected pair of QCY earbuds. Cheap to clone.
///
/// Generic over the transport; the default is the current platform's.
pub struct Earbuds<L: Link = PlatformLink> {
    inner: Arc<Inner<L>>,
}

impl<L: Link> Clone for Earbuds<L> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct Inner<L: Link> {
    session: L,
    endpoint: Endpoint,
    state: watch::Sender<DeviceState>,
    link: watch::Sender<ConnectionEvent>,
    write_lock: Mutex<()>,
    tasks: Vec<JoinHandle<()>>,
}

impl<L: Link> Drop for Inner<L> {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl<L: Link> Earbuds<L> {
    /// Opens the BLE control link and loads the current settings.
    pub async fn connect<B: Backend<Link = L>>(
        backend: &B,
        endpoint: &Endpoint,
    ) -> Result<Self, Error> {
        info!(name = %endpoint.display_name(), address = %endpoint.ble_address, "connecting");
        let session = backend.open(endpoint).await?;

        let (state, _) = watch::channel(DeviceState::default());
        let (link, _) = watch::channel(ConnectionEvent::Connected);

        let mut notifications = session.notifications().await?;
        let mut disconnected = session.disconnected().await?;

        let notify_task = {
            let state = state.clone();
            tokio::spawn(async move {
                while let Some(n) = notifications.next().await {
                    apply_notification(&state, &n);
                }
            })
        };
        let link_task = {
            let link = link.clone();
            tokio::spawn(async move {
                if disconnected.next().await.is_some() {
                    info!("BLE link lost");
                    link.send_replace(ConnectionEvent::Disconnected);
                }
            })
        };

        let buds = Self {
            inner: Arc::new(Inner {
                session,
                endpoint: endpoint.clone(),
                state,
                link,
                write_lock: Mutex::new(()),
                tasks: vec![notify_task, link_task],
            }),
        };
        buds.refresh().await?;
        Ok(buds)
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.inner.endpoint
    }

    pub fn profile(&self) -> &'static DeviceProfile {
        self.inner.endpoint.profile()
    }

    /// Snapshot of the current state.
    pub fn state(&self) -> DeviceState {
        self.inner.state.borrow().clone()
    }

    /// Receiver that is notified on every state change.
    pub fn watch(&self) -> watch::Receiver<DeviceState> {
        self.inner.state.subscribe()
    }

    /// Receiver that is notified when the BLE link drops.
    pub fn watch_connection(&self) -> watch::Receiver<ConnectionEvent> {
        self.inner.link.subscribe()
    }

    pub fn is_connected(&self) -> bool {
        *self.inner.link.borrow() == ConnectionEvent::Connected
    }

    /// Re-reads every characteristic and requests model specific settings.
    pub async fn refresh(&self) -> Result<(), Error> {
        let s = &self.inner.session;
        for c in [
            Characteristic::Version,
            Characteristic::Battery,
            Characteristic::Settings,
            Characteristic::Keys,
            Characteristic::Eq,
        ] {
            if !s.has(c) {
                continue;
            }
            match s.read(c).await {
                Ok(data) => self.apply_raw(c, &data),
                Err(e) => warn!(?c, "read failed: {e}"),
            }
        }
        for &id in self.profile().requested_settings {
            self.write(&Command::Request(id).encode()?).await?;
        }
        Ok(())
    }

    /// Executes a command.
    ///
    /// The state is updated optimistically right away; [`CONFIRM_DELAY`]
    /// later the earbuds are asked to report the setting back in the
    /// background, which corrects the state if the change did not apply.
    pub async fn send(&self, command: Command) -> Result<(), Error> {
        self.ensure_connected()?;
        let command = self.prepare(command);
        let write = command.encode()?;
        debug!(?command, "sending");
        self.write(&write).await?;

        if let Some(update) = command.implied_update() {
            self.inner.state.send_modify(|s| s.apply(update));
        }

        // Ask the earbuds to report the setting back, so the state reflects
        // what really happened. Mode changes (ANC in particular) take a moment
        // to apply; asking too early returns the previous value.
        let confirm = match write.characteristic {
            // These have no "request" command; read them back instead.
            Characteristic::Keys | Characteristic::Eq => Some(Confirm::Read(write.characteristic)),
            _ => command.confirmation_request().map(Confirm::Request),
        };
        if let Some(confirm) = confirm {
            let this = self.clone();
            tokio::spawn(async move {
                sleep(CONFIRM_DELAY).await;
                if let Err(e) = this.confirm(confirm).await {
                    debug!("confirmation failed: {e}");
                }
            });
        }
        Ok(())
    }

    async fn confirm(&self, confirm: Confirm) -> Result<(), Error> {
        self.ensure_connected()?;
        match confirm {
            Confirm::Read(c) => {
                let data = self.inner.session.read(c).await?;
                self.apply_raw(c, &data);
            }
            Confirm::Request(id) => self.write(&Command::Request(id).encode()?).await?,
        }
        Ok(())
    }

    /// Drops the BLE control link. Audio playback is not affected.
    pub async fn disconnect(&self) -> Result<(), Error> {
        self.inner.session.disconnect().await
    }

    fn ensure_connected(&self) -> Result<(), Error> {
        if self.is_connected() {
            Ok(())
        } else {
            Err(Error::NotConnected)
        }
    }

    /// Adapts a command to the link and firmware (EQ padding, like the app).
    fn prepare(&self, command: Command) -> Command {
        match command {
            Command::SetParametricEq(eq) => {
                let max_len = self.inner.session.max_write_len();
                let firmware_max = self
                    .inner
                    .state
                    .borrow()
                    .max_eq_bands
                    .map_or(DEFAULT_MAX_EQ_BANDS, usize::from);
                // `(mtu - 7) / 6` is the app's formula; additionally make sure
                // the 7-byte bands plus headers really fit into one write.
                let by_mtu = max_len.saturating_sub(7) / 6;
                let fits = max_len.saturating_sub(4 + 3) / 7;
                let total = by_mtu.min(firmware_max).min(fits);
                Command::SetParametricEq(eq.padded(total))
            }
            other => other,
        }
    }

    async fn write(&self, write: &qcy_protocol::Write) -> Result<(), Error> {
        let _guard = self.inner.write_lock.lock().await;
        self.inner.session.write(write).await?;
        sleep(WRITE_GAP).await;
        Ok(())
    }

    fn apply_raw(&self, c: Characteristic, data: &[u8]) {
        apply_notification(
            &self.inner.state,
            &Notification {
                characteristic: c,
                data: data.to_vec(),
            },
        );
    }
}

fn apply_notification(state: &watch::Sender<DeviceState>, n: &Notification) {
    match update::decode(n.characteristic, &n.data) {
        Ok(updates) => {
            for u in &updates {
                if let Update::Unknown { id, payload } = u {
                    debug!(id = %id.name(), payload = %hex(payload), "undecoded setting");
                }
            }
            state.send_modify(|s| updates.into_iter().for_each(|u| s.apply(u)));
        }
        Err(e) => warn!(c = ?n.characteristic, data = %hex(&n.data), "cannot decode: {e}"),
    }
}
