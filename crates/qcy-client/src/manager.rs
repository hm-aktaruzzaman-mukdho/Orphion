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

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use qcy_protocol::advert::MacAddress;
use qcy_protocol::dialect::{Codec, Context, Dialect, Op};
use qcy_protocol::panel::{AncControl, ModeKind, Panel};
use qcy_protocol::profile::DeviceProfile;
use qcy_protocol::state::DeviceState;
use qcy_protocol::types::{AncMode, AncSetting, KeyAction, KeyBinding};
use qcy_protocol::{Characteristic, Command, Update, catalog};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::transport::{Backend, Endpoint, Selector};
use crate::{ConnectionEvent, Earbuds, Error, Platform};

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
    /// Marketing name of the model, if it is in the catalogue.
    pub model: Option<&'static str>,
    pub dialect: Dialect,
    /// Everything the model offers.
    pub panel: Arc<Panel>,
    pub profile: &'static DeviceProfile,
    /// Characteristics the device exposes (known once connected).
    pub characteristics: BTreeSet<Characteristic>,
}

/// Characteristics whose presence changes what front-ends offer.
const PROBED: [Characteristic; 15] = [
    Characteristic::COMMAND,
    Characteristic::SETTINGS,
    Characteristic::KEYS,
    Characteristic::EQ,
    Characteristic::FUNCTION,
    Characteristic::LANGUAGE,
    Characteristic::RESET,
    Characteristic::ZR_SETTING,
    Characteristic::RCSP_WRITE,
    Characteristic::legacy_key(1),
    Characteristic::legacy_key(2),
    Characteristic::legacy_key(3),
    Characteristic::legacy_key(4),
    Characteristic::legacy_key(5),
    Characteristic::legacy_key(6),
];

impl DeviceInfo {
    /// Whether the device has a characteristic. Before connecting (and in
    /// demo mode) everything is assumed present.
    pub fn has(&self, c: Characteristic) -> bool {
        self.characteristics.is_empty() || self.characteristics.contains(&c)
    }
}

impl DeviceInfo {
    fn new(name: String, ble: MacAddress, classic: MacAddress, vendor_id: u16) -> Self {
        Self {
            name,
            ble_address: ble,
            classic_address: classic,
            vendor_id,
            model: catalog::name(vendor_id),
            dialect: Dialect::for_vendor(vendor_id),
            panel: Arc::new(catalog::panel(vendor_id)),
            profile: qcy_protocol::profile::for_vendor(vendor_id),
            characteristics: BTreeSet::new(),
        }
    }
}

impl From<&Endpoint> for DeviceInfo {
    fn from(e: &Endpoint) -> Self {
        Self::new(
            e.display_name(),
            e.ble_address,
            e.advertisement.classic_address,
            e.advertisement.vendor_id,
        )
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

/// What front-ends ask for.
enum Request {
    Connect(Selector),
    Disconnect,
    Refresh,
    Send(Command),
}

/// Outcome of one connection attempt. The backend handle travels along so a
/// working Bluetooth stack connection can be reused for the next attempt.
type AttemptResult<B> = Result<(B, Earbuds<<B as Backend>::Link>), (Option<B>, Error)>;

/// What the worker's own background tasks report back.
enum Internal<B: Backend> {
    /// Result of a background connection attempt.
    Attempt {
        generation: u64,
        result: AttemptResult<B>,
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
    requests: mpsc::UnboundedSender<Request>,
    snapshot: watch::Receiver<Snapshot>,
    notices: broadcast::Sender<Notice>,
}

impl Manager {
    /// Starts the manager for the current platform's Bluetooth stack on the
    /// current tokio runtime.
    pub fn spawn() -> Self {
        Self::spawn_with::<Platform>()
    }

    /// Starts the manager with a specific [`Backend`].
    pub fn spawn_with<B: Backend>() -> Self {
        let (requests_tx, requests_rx) = mpsc::unbounded_channel();
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();
        let (snapshot_tx, snapshot_rx) = watch::channel(Snapshot::default());
        let (notices, _) = broadcast::channel(32);

        let worker = Worker::<B> {
            internal_tx,
            snapshot: snapshot_tx,
            notices: notices.clone(),
            backend: None,
            buds: None,
            selector: Selector::Auto,
            want_connected: false,
            generation: 0,
            attempt: 0,
        };
        tokio::spawn(worker.run(requests_rx, internal_rx));

        Self {
            requests: requests_tx,
            snapshot: snapshot_rx,
            notices,
        }
    }

    /// Starts connecting (and keeps reconnecting until [`Manager::disconnect`]).
    pub fn connect(&self, selector: Selector) {
        self.requests.send(Request::Connect(selector)).ok();
    }

    /// Closes the BLE link and stops reconnecting.
    pub fn disconnect(&self) {
        self.requests.send(Request::Disconnect).ok();
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
        self.requests.send(Request::Refresh).ok();
    }

    /// Queues a command. Failures are reported as [`Notice::Error`].
    pub fn send(&self, command: Command) {
        self.requests.send(Request::Send(command)).ok();
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
    /// A manager backed by *simulated* earbuds of model `vendor_id` (any
    /// model of the catalogue). No Bluetooth is used; commands are encoded
    /// by the model's real codec, so invalid ones are reported, and then
    /// applied to the state. The MeloBuds Pro (19786) starts from data
    /// captured on real earbuds ([`qcy_protocol::samples`]), other models
    /// from their panel defaults. Meant for UI development and screenshots.
    pub fn spawn_demo(vendor_id: u16) -> Self {
        let (requests_tx, mut requests_rx) = mpsc::unbounded_channel();
        let (snapshot, snapshot_rx) = watch::channel(Snapshot::default());
        let (notices, _) = broadcast::channel(32);

        let advert =
            qcy_protocol::advert::Advertisement::parse(&qcy_protocol::samples::ADVERTISEMENT)
                .expect("sample advertisement is valid");
        let name = catalog::name(vendor_id).unwrap_or("QCY earbuds");
        let device = DeviceInfo::new(
            format!("{name} (demo)"),
            "C4:AC:60:89:A4:B1".parse().expect("valid address"),
            advert.classic_address,
            vendor_id,
        );
        let mut codec = Codec::new(vendor_id, &device.panel);
        let initial = if vendor_id == advert.vendor_id {
            qcy_protocol::samples::state()
        } else {
            demo_state(&device.panel)
        };
        let demo_notices = notices.clone();
        tokio::spawn(async move {
            while let Some(request) = requests_rx.recv().await {
                match request {
                    Request::Connect(_) => snapshot.send_modify(|s| {
                        s.link = LinkStatus::Connected;
                        s.device = Some(device.clone());
                        if s.state == DeviceState::default() {
                            s.state = initial.clone();
                        }
                    }),
                    Request::Disconnect => snapshot.send_modify(|s| s.link = LinkStatus::Idle),
                    Request::Send(command) => {
                        let state = snapshot.borrow().state.clone();
                        let cx = Context {
                            state: &state,
                            max_write_len: 217,
                            unix_time: 0,
                        };
                        if let Err(e) = codec.encode(&command, &cx).map(|ops: Vec<Op>| ops) {
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
                    Request::Refresh => {}
                }
            }
        });

        Self {
            requests: requests_tx,
            snapshot: snapshot_rx,
            notices,
        }
    }
}

/// A plausible starting state for a simulated model: full batteries, the
/// panel's default noise control, first EQ preset and default key bindings.
fn demo_state(panel: &Panel) -> DeviceState {
    let mut s = DeviceState::default();
    let mut apply = |u| s.apply(u);
    if let Ok(b) = qcy_protocol::types::BatteryStatus::parse(&[85, 80, 0x80 | 60]) {
        apply(Update::Battery(b));
    }
    let v = qcy_protocol::types::Version(1, 0, 0);
    apply(Update::Firmware(qcy_protocol::types::FirmwareVersions {
        left: Some(v),
        right: Some(v),
    }));
    match &panel.anc {
        Some(AncControl::Scenes(a)) => {
            if let Some(m) = a
                .modes
                .iter()
                .find(|m| matches!(m.kind, ModeKind::Scenes(_)))
                .or(a.modes.first())
            {
                apply(Update::Anc(AncMode::from_setting(AncSetting::from_packed(
                    m.default,
                ))));
            }
        }
        Some(AncControl::Legacy(a)) => {
            if let Some((_, m)) = a.modes.first() {
                apply(Update::Setting {
                    id: qcy_protocol::CommandId::NoiseMode,
                    payload: vec![*m],
                });
            }
        }
        Some(AncControl::Percent(a)) => {
            if let Some(m) = a.modes.first() {
                apply(Update::Anc(AncMode::from_setting(AncSetting::from_packed(
                    m.packed(m.max_percent / 2),
                ))));
            }
        }
        None => {}
    }
    if let Some(eq) = &panel.eq
        && let Some(p) = eq.presets.first()
    {
        apply(Update::GraphicEq(qcy_protocol::eq::GraphicEq {
            preset: p.id,
            gains: p.gains.iter().map(|g| *g as i8).collect(),
        }));
    }
    let bindings: Vec<KeyBinding> = [&panel.touch_keys, &panel.legacy_keys, &panel.buttons]
        .into_iter()
        .flatten()
        .flat_map(|k| k.slots())
        .filter_map(|slot| {
            Some(KeyBinding {
                gesture_id: slot.event,
                action: KeyAction::from_id(slot.default.or(slot.choices.first().map(|c| c.0))?),
            })
        })
        .collect();
    apply(Update::KeyBindings(bindings));
    apply(Update::Balance(50));
    s
}

struct Worker<B: Backend> {
    internal_tx: mpsc::UnboundedSender<Internal<B>>,
    snapshot: watch::Sender<Snapshot>,
    notices: broadcast::Sender<Notice>,
    /// The Bluetooth stack handle, kept between attempts once it works.
    backend: Option<B>,
    buds: Option<Earbuds<B::Link>>,
    selector: Selector,
    want_connected: bool,
    /// Bumped whenever a connection attempt or link becomes obsolete, so
    /// late results of old attempts are ignored.
    generation: u64,
    attempt: u32,
}

impl<B: Backend> Worker<B> {
    async fn run(
        mut self,
        mut requests: mpsc::UnboundedReceiver<Request>,
        mut internal: mpsc::UnboundedReceiver<Internal<B>>,
    ) {
        loop {
            tokio::select! {
                request = requests.recv() => match request {
                    Some(r) => self.handle_request(r).await,
                    // Every Manager handle is gone.
                    None => break,
                },
                Some(event) = internal.recv() => self.handle_internal(event),
            }
        }
    }

    async fn handle_request(&mut self, request: Request) {
        match request {
            Request::Connect(selector) => {
                self.selector = selector;
                self.want_connected = true;
                self.attempt = 0;
                if self.buds.is_none() {
                    self.start_attempt();
                }
            }
            Request::Disconnect => {
                self.want_connected = false;
                self.generation += 1;
                if let Some(buds) = self.buds.take() {
                    buds.disconnect().await.ok();
                }
                self.set_link(LinkStatus::Idle);
            }
            Request::Refresh => {
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
            Request::Send(command) => self.send(command),
        }
    }

    fn handle_internal(&mut self, event: Internal<B>) {
        match event {
            Internal::Attempt { generation, result } if generation == self.generation => {
                self.finish_attempt(result);
            }
            Internal::LinkLost { generation } if generation == self.generation => {
                self.buds = None;
                if self.want_connected {
                    self.notice(Notice::Info("Connection lost, reconnecting…".into()));
                    self.attempt = 0;
                    self.start_attempt();
                } else {
                    self.set_link(LinkStatus::Idle);
                }
            }
            Internal::Retry { generation }
                if generation == self.generation && self.want_connected && self.buds.is_none() =>
            {
                self.start_attempt();
            }
            // Stale results of superseded attempts.
            Internal::Attempt { .. } | Internal::LinkLost { .. } | Internal::Retry { .. } => {}
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
        let backend = self.backend.clone();
        let tx = self.internal_tx.clone();
        let snapshot = self.snapshot.clone();
        self.set_link(LinkStatus::Searching);

        tokio::spawn(async move {
            let backend = match backend {
                Some(b) => b,
                None => match B::init().await {
                    Ok(b) => b,
                    Err(e) => {
                        let result = Err((None, e));
                        tx.send(Internal::Attempt { generation, result }).ok();
                        return;
                    }
                },
            };
            let result = async {
                let endpoint = backend.find(&selector, SEARCH_TIMEOUT).await?;
                snapshot.send_modify(|s| {
                    s.link = LinkStatus::Connecting;
                    s.device = Some(DeviceInfo::from(&endpoint));
                });
                Earbuds::connect(&backend, &endpoint).await
            }
            .await;
            let result = match result {
                Ok(buds) => Ok((backend, buds)),
                Err(e) => Err((Some(backend), e)),
            };
            tx.send(Internal::Attempt { generation, result }).ok();
        });
    }

    fn finish_attempt(&mut self, result: AttemptResult<B>) {
        match result {
            Ok((backend, buds)) => {
                info!(name = %buds.endpoint().display_name(), "connected");
                self.backend = Some(backend);
                self.attempt = 0;
                self.watch_link(&buds);
                self.snapshot.send_modify(|s| {
                    s.link = LinkStatus::Connected;
                    let mut device = DeviceInfo::from(buds.endpoint());
                    device.characteristics = PROBED.into_iter().filter(|c| buds.has(*c)).collect();
                    s.device = Some(device);
                    s.state = buds.state();
                });
                self.buds = Some(buds);
            }
            Err((backend, e)) if self.want_connected => {
                // Keep the stack handle unless the stack itself is the problem.
                self.backend = match e {
                    Error::NoAdapter | Error::AdapterOff | Error::Backend(_) => None,
                    _ => backend,
                };
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
                let tx = self.internal_tx.clone();
                let generation = self.generation;
                tokio::spawn(async move {
                    sleep(retry_in).await;
                    tx.send(Internal::Retry { generation }).ok();
                });
            }
            Err(_) => self.set_link(LinkStatus::Idle),
        }
    }

    /// Mirrors the device state into the snapshot and reports link loss.
    fn watch_link(&self, buds: &Earbuds<B::Link>) {
        let generation = self.generation;
        let mut state = buds.watch();
        let mut link = buds.watch_connection();
        let snapshot = self.snapshot.clone();
        let tx = self.internal_tx.clone();
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
            tx.send(Internal::LinkLost { generation }).ok();
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
