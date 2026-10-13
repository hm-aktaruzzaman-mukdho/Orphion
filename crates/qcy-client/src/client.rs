//! High level, stateful client for one pair of earbuds.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use qcy_protocol::dialect::{Codec, Context, Dialect, Op};
use qcy_protocol::panel::Panel;
use qcy_protocol::profile::DeviceProfile;
use qcy_protocol::state::DeviceState;
use qcy_protocol::{Characteristic, Command, Update, catalog};
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::transport::{Backend, Endpoint, Link, hex};
use crate::{Error, PlatformLink};

/// Pause between consecutive writes. Writes are unacknowledged, and the
/// official app also sends them strictly one after another.
const WRITE_GAP: Duration = Duration::from_millis(40);
/// Delay before asking the earbuds to confirm a change. Measured on a
/// MeloBuds Pro: an ANC switch is not reported back until ~0.5 s later.
pub const CONFIRM_DELAY: Duration = Duration::from_millis(1000);
/// Resets and pairing changes make the earbuds busy for a moment; the app
/// waits this long before reading everything again.
const RESET_SETTLE: Duration = Duration::from_millis(2500);

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
    panel: Arc<Panel>,
    /// Translates commands and data for this model; shared with the
    /// notification task (never held across an `.await`).
    codec: Arc<StdMutex<Codec>>,
    state: watch::Sender<DeviceState>,
    link: watch::Sender<ConnectionEvent>,
    write_lock: Mutex<()>,
    tasks: StdMutex<Vec<JoinHandle<()>>>,
}

impl<L: Link> Drop for Inner<L> {
    fn drop(&mut self) {
        if let Ok(tasks) = self.tasks.lock() {
            for t in tasks.iter() {
                t.abort();
            }
        }
    }
}

fn unix_time() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as u32)
}

impl<L: Link> Earbuds<L> {
    /// Opens the BLE control link and loads the current settings.
    pub async fn connect<B: Backend<Link = L>>(
        backend: &B,
        endpoint: &Endpoint,
    ) -> Result<Self, Error> {
        let vendor_id = endpoint.advertisement.vendor_id;
        let dialect = Dialect::for_vendor(vendor_id);
        info!(
            name = %endpoint.display_name(),
            address = %endpoint.ble_address,
            model = vendor_id,
            ?dialect,
            "connecting"
        );
        let session = backend.open(endpoint).await?;
        if matches!(dialect, Dialect::JieLi { .. }) && !session.has(Characteristic::RCSP_WRITE) {
            return Err(Error::NotFound(
                "the JieLi control service (0xA002) is missing".into(),
            ));
        }

        let panel = Arc::new(catalog::panel(vendor_id));
        let codec = Arc::new(StdMutex::new(Codec::new(vendor_id, &panel)));
        let (state, _) = watch::channel(DeviceState::default());
        let (link, _) = watch::channel(ConnectionEvent::Connected);

        let mut notifications = session.notifications().await?;
        let mut disconnected = session.disconnected().await?;

        let notify_task = {
            let state = state.clone();
            let codec = Arc::clone(&codec);
            tokio::spawn(async move {
                while let Some(n) = notifications.next().await {
                    apply(&state, &codec, n.characteristic, &n.data);
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
                panel,
                codec,
                state,
                link,
                write_lock: Mutex::new(()),
                tasks: StdMutex::new(vec![notify_task, link_task]),
            }),
        };
        let hello = buds.with_codec(|c| c.connect(unix_time()))?;
        buds.run(hello).await?;
        buds.refresh().await?;
        buds.start_polling();
        Ok(buds)
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.inner.endpoint
    }

    /// Legacy hard-coded capabilities (superseded by [`Self::panel`]).
    pub fn profile(&self) -> &'static DeviceProfile {
        self.inner.endpoint.profile()
    }

    /// The model's control panel: everything it offers.
    pub fn panel(&self) -> Arc<Panel> {
        Arc::clone(&self.inner.panel)
    }

    /// How this model is driven.
    pub fn dialect(&self) -> Dialect {
        self.with_codec(|c| c.dialect())
    }

    /// Whether the device exposes a characteristic.
    pub fn has(&self, c: Characteristic) -> bool {
        self.inner.session.has(c)
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

    /// Reads every setting the model's panel shows.
    pub async fn refresh(&self) -> Result<(), Error> {
        let session = &self.inner.session;
        let ops = self.with_codec(|c| c.refresh(&self.inner.panel, |ch| session.has(ch)))?;
        self.run(ops).await
    }

    /// Executes a command.
    ///
    /// The state is updated optimistically right away; [`CONFIRM_DELAY`]
    /// later the setting is read back in the background, which corrects the
    /// state if the change did not apply.
    pub async fn send(&self, command: Command) -> Result<(), Error> {
        self.ensure_connected()?;
        let ops = {
            let state = self.inner.state.borrow().clone();
            let cx = Context {
                state: &state,
                max_write_len: self.inner.session.max_write_len(),
                unix_time: unix_time(),
            };
            self.with_codec(|c| c.encode(&command, &cx))?
        };
        debug!(?command, "sending");
        self.run(ops).await?;

        if let Some(update) = command.implied_update() {
            self.inner.state.send_modify(|s| s.apply(update));
        }

        let this = self.clone();
        let resets = matches!(
            command,
            Command::ResetDefaults | Command::ClearPairing | Command::FactoryReset
        ) || matches!(
            &command,
            Command::Panel(p) if p.setting.is_none() && p.value.is_none()
        );
        tokio::spawn(async move {
            if resets {
                // Like the app: give the earbuds time, then re-read it all.
                sleep(RESET_SETTLE).await;
                if let Err(e) = this.refresh().await {
                    debug!("re-reading after reset failed: {e}");
                }
                return;
            }
            sleep(CONFIRM_DELAY).await;
            let confirm = this.with_codec(|c| c.confirm(&command));
            let result = match confirm {
                Ok(ops) => this.run(ops).await,
                Err(e) => Err(e.into()),
            };
            if let Err(e) = result {
                debug!("confirmation failed: {e}");
            }
        });
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

    fn with_codec<T>(&self, f: impl FnOnce(&mut Codec) -> T) -> T {
        let mut codec = self.inner.codec.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut codec)
    }

    /// Polls status the model does not notify (JieLi battery, some ANC).
    fn start_polling(&self) {
        let Some(interval) = self.with_codec(|c| c.poll_interval()) else {
            return;
        };
        // A weak handle, so the task does not keep the link alive.
        let weak = Arc::downgrade(&self.inner);
        let task = tokio::spawn(async move {
            loop {
                sleep(interval).await;
                let Some(inner) = weak.upgrade() else { break };
                let buds = Earbuds { inner };
                if !buds.is_connected() {
                    break;
                }
                let ops = buds.with_codec(|c| c.poll());
                match ops {
                    Ok(ops) => {
                        if let Err(e) = buds.run(ops).await {
                            debug!("poll failed: {e}");
                        }
                    }
                    Err(e) => debug!("poll failed: {e}"),
                }
            }
        });
        if let Ok(mut tasks) = self.inner.tasks.lock() {
            tasks.push(task);
        }
    }

    /// Runs GATT operations in order. Operations on characteristics the
    /// device does not have are skipped, like the app silently does.
    async fn run(&self, ops: Vec<Op>) -> Result<(), Error> {
        let session = &self.inner.session;
        for op in ops {
            match op {
                Op::Read(c) => {
                    if !session.has(c) {
                        continue;
                    }
                    match session.read(c).await {
                        Ok(data) => apply(&self.inner.state, &self.inner.codec, c, &data),
                        Err(e) => warn!(?c, "read failed: {e}"),
                    }
                }
                Op::Write(w) => {
                    if !session.has(w.characteristic) {
                        debug!(c = ?w.characteristic, "not on this device; write skipped");
                        continue;
                    }
                    let _guard = self.inner.write_lock.lock().await;
                    session.write(&w).await?;
                    sleep(WRITE_GAP).await;
                }
            }
        }
        Ok(())
    }
}

fn apply(
    state: &watch::Sender<DeviceState>,
    codec: &StdMutex<Codec>,
    c: Characteristic,
    data: &[u8],
) {
    let updates = codec
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .decode(c, data);
    if updates.is_empty() {
        debug!(?c, data = %hex(data), "nothing decoded");
        return;
    }
    for u in &updates {
        if let Update::Unknown { id, payload } = u {
            debug!(id = %id.name(), payload = %hex(payload), "undecoded setting");
        }
    }
    state.send_modify(|s| updates.into_iter().for_each(|u| s.apply(u)));
}
