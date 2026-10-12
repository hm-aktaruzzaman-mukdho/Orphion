//! Connection manager for long-running front-ends (GUI, tray).
//!
//! [`Manager`] owns the connection in a background task and exposes it as:
//!
//! * a [`watch`] channel of [`Snapshot`]s (link status + full device state),
//!   cheap to poll from a render loop;
//! * a [`broadcast`] channel of [`Notice`]s (errors / info for toasts);
//! * fire-and-forget methods ([`Manager::send`], [`Manager::connect`], …).
//!
//! While the user wants to be connected it keeps (re)connecting with
//! exponential backoff: earbuds reboot after some setting changes (LDAC,
//! dual device) and disappear while in the case.

use std::time::Duration;

use qcy_protocol::Command;
use qcy_protocol::advert::MacAddress;
use qcy_protocol::profile::DeviceProfile;
use qcy_protocol::state::DeviceState;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::discovery::Selector;
use crate::{Bluez, ConnectionEvent, Earbuds, Endpoint, Error};

const SEARCH_TIMEOUT: Duration = Duration::from_secs(10);
const BACKOFF_START: Duration = Duration::from_secs(3);
const BACKOFF_MAX: Duration = Duration::from_secs(30);
const QUICK_RETRY: Duration = Duration::from_secs(1);

/// Where the connection currently stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LinkStatus {
    /// Not connected and not trying to.
    #[default]
    Idle,
    /// Scanning for the earbuds.
    Searching,
    /// Opening the BLE link.
    Connecting,
    Connected,
    /// Waiting before the next attempt.
    Waiting {
        attempt: u32,
        retry_in: Duration,
    },
}

impl LinkStatus {
    pub fn label(&self) -> String {
        match self {
            LinkStatus::Idle => "Disconnected".into(),
            LinkStatus::Searching => "Searching…".into(),
            LinkStatus::Connecting => "Connecting…".into(),
            LinkStatus::Connected => "Connected".into(),
            LinkStatus::Waiting { retry_in, .. } => {
                format!("Not found · retrying in {}s", retry_in.as_secs())
            }
        }
    }

    /// Whether the manager is actively working towards a connection.
    pub fn is_busy(&self) -> bool {
        !matches!(self, LinkStatus::Idle | LinkStatus::Connected)
    }
}

/// Static facts about the connected earbuds.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub name: String,
    pub ble_address: MacAddress,
    pub classic_address: MacAddress,
    pub vendor_id: u16,
    pub profile: &'static DeviceProfile,
}

impl From<&Endpoint> for DeviceInfo {
    fn from(e: &Endpoint) -> Self {
        Self {
            name: e.display_name(),
            ble_address: e.ble_address,
            classic_address: e.advertisement.classic_address,
            vendor_id: e.advertisement.vendor_id,
            profile: e.profile(),
        }
    }
}

/// Everything a front-end renders.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub link: LinkStatus,
    /// The last connected (or currently connected) earbuds.
    pub device: Option<DeviceInfo>,
    pub state: DeviceState,
}

impl Snapshot {
    pub fn is_connected(&self) -> bool {
        self.link == LinkStatus::Connected
    }
}

/// A message worth showing to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Info(String),
    Error(String),
}

enum Event {
    Connect(Selector),
    Disconnect,
    Refresh,
    Send(Command),
    /// Result of a background connection attempt.
    Attempt {
        generation: u64,
        result: Result<Earbuds, Error>,
    },
    LinkLost {
        generation: u64,
    },
    Retry {
        generation: u64,
    },
}

/// Handle to the background connection manager. Cheap to clone.
#[derive(Clone)]
pub struct Manager {
    events: mpsc::UnboundedSender<Event>,
    snapshot: watch::Receiver<Snapshot>,
    notices: broadcast::Sender<Notice>,
}

impl Manager {
    /// Starts the manager on the current tokio runtime.
    pub fn spawn() -> Self {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let (snapshot_tx, snapshot_rx) = watch::channel(Snapshot::default());
        let (notices, _) = broadcast::channel(32);

        let worker = Worker {
            events_tx: events_tx.clone(),
            snapshot: snapshot_tx,
            notices: notices.clone(),
            bluez: None,
            buds: None,
            selector: Selector::Auto,
            want_connected: false,
            generation: 0,
            attempt: 0,
        };
        tokio::spawn(worker.run(events_rx));

        Self {
            events: events_tx,
            snapshot: snapshot_rx,
            notices,
        }
    }

    /// Starts connecting (and keeps reconnecting until [`Manager::disconnect`]).
    pub fn connect(&self, selector: Selector) {
        self.events.send(Event::Connect(selector)).ok();
    }

    /// Closes the BLE link and stops reconnecting.
    pub fn disconnect(&self) {
        self.events.send(Event::Disconnect).ok();
    }

    /// Disconnects and waits (at most `timeout`) until the link is closed.
    pub async fn shutdown(&self, timeout: Duration) {
        self.disconnect();
        let mut rx = self.snapshot.clone();
        let idle = rx.wait_for(|s| s.link == LinkStatus::Idle);
        tokio::time::timeout(timeout, idle).await.ok();
    }

    /// Re-reads all settings from the earbuds.
    pub fn refresh(&self) {
        self.events.send(Event::Refresh).ok();
    }

    /// Queues a command. Failures are reported as [`Notice::Error`].
    pub fn send(&self, command: Command) {
        self.events.send(Event::Send(command)).ok();
    }

    /// Latest snapshot.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }

    /// Receiver notified on every snapshot change.
    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.snapshot.clone()
    }

    pub fn notices(&self) -> broadcast::Receiver<Notice> {
        self.notices.subscribe()
    }
}

impl Manager {
    /// A manager backed by *simulated* earbuds built from data captured on a
    /// real MeloBuds Pro ([`qcy_protocol::samples`]). No Bluetooth is used;
    /// commands are validated by encoding them and then applied to the state.
    /// Meant for UI development, screenshots and demos.
    pub fn spawn_demo() -> Self {
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let (snapshot, snapshot_rx) = watch::channel(Snapshot::default());
        let (notices, _) = broadcast::channel(32);

        let advert =
            qcy_protocol::advert::Advertisement::parse(&qcy_protocol::samples::ADVERTISEMENT)
                .expect("sample advertisement is valid");
        let device = DeviceInfo {
            name: "QCY MeloBuds Pro (demo)".into(),
            ble_address: "C4:AC:60:89:A4:B1".parse().expect("valid address"),
            classic_address: advert.classic_address,
            vendor_id: advert.vendor_id,
            profile: qcy_protocol::profile::for_vendor(advert.vendor_id),
        };
        let demo_notices = notices.clone();
        tokio::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                match event {
                    Event::Connect(_) => snapshot.send_modify(|s| {
                        s.link = LinkStatus::Connected;
                        s.device = Some(device.clone());
                        if s.state == DeviceState::default() {
                            s.state = qcy_protocol::samples::state();
                        }
                    }),
                    Event::Disconnect => snapshot.send_modify(|s| s.link = LinkStatus::Idle),
                    Event::Send(command) => {
                        if let Err(e) = command.encode() {
                            demo_notices
                                .send(Notice::Error(format!("Command failed: {e}")))
                                .ok();
                            continue;
                        }
                        let update = match &command {
                            Command::SetPromptVolume(level) => {
                                Some(qcy_protocol::Update::PromptVolume(
                                    qcy_protocol::types::PromptVolume {
                                        level: *level,
                                        max: 15,
                                    },
                                ))
                            }
                            Command::SetPowerOffTimer(minutes) => {
                                Some(qcy_protocol::Update::PowerOffTimer(
                                    qcy_protocol::types::PowerOffTimer {
                                        minutes: *minutes,
                                        progress: 0,
                                    },
                                ))
                            }
                            other => other.implied_update(),
                        };
                        if let Some(update) = update {
                            snapshot.send_modify(|s| s.state.apply(update));
                        }
                    }
                    Event::Refresh
                    | Event::Attempt { .. }
                    | Event::LinkLost { .. }
                    | Event::Retry { .. } => {}
                }
            }
        });

        Self {
            events: events_tx,
            snapshot: snapshot_rx,
            notices,
        }
    }
}

struct Worker {
    events_tx: mpsc::UnboundedSender<Event>,
    snapshot: watch::Sender<Snapshot>,
    notices: broadcast::Sender<Notice>,
    bluez: Option<Bluez>,
    buds: Option<Earbuds>,
    selector: Selector,
    want_connected: bool,
    /// Bumped whenever a connection attempt or link becomes obsolete, so
    /// late results of old attempts are ignored.
    generation: u64,
    attempt: u32,
}

impl Worker {
    async fn run(mut self, mut events: mpsc::UnboundedReceiver<Event>) {
        while let Some(event) = events.recv().await {
            match event {
                Event::Connect(selector) => {
                    self.selector = selector;
                    self.want_connected = true;
                    self.attempt = 0;
                    if self.buds.is_none() {
                        self.start_attempt();
                    }
                }
                Event::Disconnect => {
                    self.want_connected = false;
                    self.generation += 1;
                    if let Some(buds) = self.buds.take() {
                        buds.disconnect().await.ok();
                    }
                    self.set_link(LinkStatus::Idle);
                }
                Event::Refresh => {
                    if let Some(buds) = self.buds.clone() {
                        let notices = self.notices.clone();
                        tokio::spawn(async move {
                            if let Err(e) = buds.refresh().await {
                                notices
                                    .send(Notice::Error(format!("Refresh failed: {e}")))
                                    .ok();
                            }
                        });
                    }
                }
                Event::Send(command) => self.send(command),
                Event::Attempt { generation, result } if generation == self.generation => {
                    self.finish_attempt(result);
                }
                Event::LinkLost { generation } if generation == self.generation => {
                    self.buds = None;
                    if self.want_connected {
                        self.notice(Notice::Info("Connection lost, reconnecting…".into()));
                        self.attempt = 0;
                        self.start_attempt();
                    } else {
                        self.set_link(LinkStatus::Idle);
                    }
                }
                Event::Retry { generation }
                    if generation == self.generation
                        && self.want_connected
                        && self.buds.is_none() =>
                {
                    self.start_attempt();
                }
                // Stale results of superseded attempts.
                Event::Attempt { .. } | Event::LinkLost { .. } | Event::Retry { .. } => {}
            }
        }
    }

    fn send(&self, command: Command) {
        let Some(buds) = self.buds.clone() else {
            self.notice(Notice::Error("Not connected".into()));
            return;
        };
        let notices = self.notices.clone();
        tokio::spawn(async move {
            if let Err(e) = buds.send(command).await {
                notices
                    .send(Notice::Error(format!("Command failed: {e}")))
                    .ok();
            }
        });
    }

    fn start_attempt(&mut self) {
        self.generation += 1;
        self.attempt += 1;
        let generation = self.generation;
        let selector = self.selector.clone();
        let bluez = self.bluez.clone();
        let tx = self.events_tx.clone();
        let snapshot = self.snapshot.clone();
        self.set_link(LinkStatus::Searching);

        tokio::spawn(async move {
            let result = async {
                let bluez = match bluez {
                    Some(b) => b,
                    None => Bluez::connect().await?,
                };
                let endpoint = bluez.find(&selector, SEARCH_TIMEOUT).await?;
                snapshot.send_modify(|s| {
                    s.link = LinkStatus::Connecting;
                    s.device = Some(DeviceInfo::from(&endpoint));
                });
                Earbuds::connect(&bluez, &endpoint).await
            }
            .await;
            tx.send(Event::Attempt { generation, result }).ok();
        });
    }

    fn finish_attempt(&mut self, result: Result<Earbuds, Error>) {
        match result {
            Ok(buds) => {
                info!(name = %buds.endpoint().display_name(), "connected");
                self.attempt = 0;
                self.watch_link(&buds);
                self.snapshot.send_modify(|s| {
                    s.link = LinkStatus::Connected;
                    s.device = Some(DeviceInfo::from(buds.endpoint()));
                    s.state = buds.state();
                });
                self.buds = Some(buds);
            }
            Err(e) if self.want_connected => {
                // Keep the Bluez handle unless BlueZ itself is the problem.
                if matches!(e, Error::NoAdapter | Error::AdapterOff | Error::DBus(_)) {
                    self.bluez = None;
                }
                warn!(attempt = self.attempt, "connection attempt failed: {e}");
                // Transient link failures are retried silently; only tell the
                // user once something persistent is going on.
                if (self.attempt == 1 && !e.is_transient()) || self.attempt == 4 {
                    self.notice(Notice::Error(e.to_string()));
                }
                let retry_in = if !matches!(e, Error::NotFound(_)) && self.attempt <= 3 {
                    // The earbuds were found but the link failed (the radio
                    // is often busy with audio): try again almost at once.
                    QUICK_RETRY
                } else {
                    BACKOFF_START
                        .saturating_mul(1 << self.attempt.saturating_sub(1).min(4))
                        .min(BACKOFF_MAX)
                };
                self.set_link(LinkStatus::Waiting {
                    attempt: self.attempt,
                    retry_in,
                });
                let tx = self.events_tx.clone();
                let generation = self.generation;
                tokio::spawn(async move {
                    sleep(retry_in).await;
                    tx.send(Event::Retry { generation }).ok();
                });
            }
            Err(_) => self.set_link(LinkStatus::Idle),
        }
    }

    /// Mirrors the device state into the snapshot and reports link loss.
    fn watch_link(&self, buds: &Earbuds) {
        let generation = self.generation;
        let mut state = buds.watch();
        let mut link = buds.watch_connection();
        let snapshot = self.snapshot.clone();
        let tx = self.events_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    changed = state.changed() => {
                        if changed.is_err() { break; }
                        let new = state.borrow_and_update().clone();
                        snapshot.send_modify(|s| s.state = new);
                    }
                    changed = link.changed() => {
                        if changed.is_err() || *link.borrow() == ConnectionEvent::Disconnected {
                            break;
                        }
                    }
                }
            }
            tx.send(Event::LinkLost { generation }).ok();
        });
    }

    fn set_link(&self, link: LinkStatus) {
        self.snapshot.send_if_modified(|s| {
            let changed = s.link != link;
            s.link = link;
            changed
        });
    }

    fn notice(&self, notice: Notice) {
        self.notices.send(notice).ok();
    }
}
