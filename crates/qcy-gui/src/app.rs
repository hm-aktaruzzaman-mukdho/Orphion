//! Top-level application: layout, navigation, snackbars and confirmations.
//!
//! Views never talk to the [`Manager`] directly. They render from a
//! [`Snapshot`] and push [`Action`]s, which [`App`] executes after the frame.
//! This keeps views free of side effects and easy to rearrange.

use std::time::{Duration, Instant};

use eframe::egui::{self, Id};
use qcy_bluez::discovery::Selector;
use qcy_bluez::manager::{Manager, Notice, Snapshot};
use qcy_protocol::Command;
use qcy_protocol::profile::{self, DeviceProfile};
use tokio::sync::broadcast;

use crate::capture::{Capture, Shot};
use crate::material::components::{self as m3, DialogResult};
use crate::material::{self, MaterialIcon, Scheme, icons};
use crate::prefs::{Appearance, Prefs};
use crate::views::{self, Tab};

const SNACKBAR_TTL: Duration = Duration::from_secs(5);
/// Content column width; wider windows get margins on both sides.
const CONTENT_MAX_WIDTH: f32 = 680.0;

/// Something a view wants to happen.
pub enum Action {
    Send(Command),
    /// Ask the user first, then send.
    Confirm(Confirmation),
    Connect,
    Disconnect,
    Refresh,
    SetAppearance(Appearance),
}

/// A pending "are you sure?" dialog.
pub struct Confirmation {
    pub icon: MaterialIcon,
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub destructive: bool,
    pub command: Command,
}

impl Confirmation {
    pub fn new(
        icon: MaterialIcon,
        title: &str,
        message: &str,
        confirm: &str,
        command: Command,
    ) -> Self {
        Self {
            icon,
            title: title.into(),
            message: message.into(),
            confirm_label: confirm.into(),
            destructive: false,
            command,
        }
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

/// UI-only values that are being edited and not yet sent.
#[derive(Default)]
pub struct Drafts {
    /// 10-band custom EQ being edited, `None` when not editing.
    pub custom_eq: Option<[i8; 10]>,
    /// Slider positions while dragging.
    pub balance: Option<u8>,
    pub transparency: Option<u8>,
    /// Bluetooth name text field.
    pub name: Option<String>,
}

/// Everything a view needs for one frame.
pub struct ViewCtx<'a> {
    pub snap: &'a Snapshot,
    pub profile: &'static DeviceProfile,
    pub drafts: &'a mut Drafts,
    pub appearance: Appearance,
    actions: &'a mut Vec<Action>,
}

impl ViewCtx<'_> {
    pub fn send(&mut self, command: Command) {
        self.actions.push(Action::Send(command));
    }

    pub fn confirm(&mut self, confirmation: Confirmation) {
        self.actions.push(Action::Confirm(confirmation));
    }

    pub fn act(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn connected(&self) -> bool {
        self.snap.is_connected()
    }
}

pub struct App {
    manager: Manager,
    notices: broadcast::Receiver<Notice>,
    snackbars: Vec<(String, bool, Instant)>,
    tab: Tab,
    drafts: Drafts,
    prefs: Prefs,
    pending: Option<Confirmation>,
    capture: Option<Capture>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        manager: Manager,
        rt: tokio::runtime::Handle,
    ) -> Self {
        material::install(&cc.egui_ctx);
        let prefs = Prefs::load();
        cc.egui_ctx.set_theme(prefs.appearance.theme_preference());

        // Repaint whenever the device state changes; egui is otherwise idle.
        let ctx = cc.egui_ctx.clone();
        let mut snapshots = manager.subscribe();
        let mut notices = manager.notices();
        rt.spawn(async move {
            loop {
                tokio::select! {
                    r = snapshots.changed() => if r.is_err() { break },
                    r = notices.recv() => if matches!(r, Err(broadcast::error::RecvError::Closed)) { break },
                }
                ctx.request_repaint();
            }
        });

        Self {
            notices: manager.notices(),
            manager,
            snackbars: Vec::new(),
            tab: Tab::Noise,
            drafts: Drafts::default(),
            prefs,
            pending: None,
            capture: Capture::from_args(),
        }
    }

    fn run(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Send(command) => self.manager.send(command),
            Action::Confirm(c) => self.pending = Some(c),
            Action::Connect => self.manager.connect(Selector::Auto),
            Action::Disconnect => self.manager.disconnect(),
            Action::Refresh => self.manager.refresh(),
            Action::SetAppearance(a) => {
                self.prefs.appearance = a;
                self.prefs.save();
                ctx.set_theme(a.theme_preference());
            }
        }
    }

    fn collect_notices(&mut self) {
        loop {
            match self.notices.try_recv() {
                Ok(Notice::Info(t)) => self.snackbars.push((t, false, Instant::now())),
                Ok(Notice::Error(t)) => self.snackbars.push((t, true, Instant::now())),
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
        self.snackbars
            .retain(|(_, _, t)| t.elapsed() < SNACKBAR_TTL);
    }

    fn show_snackbars(&self, ctx: &egui::Context) {
        let messages: Vec<(String, bool)> = self
            .snackbars
            .iter()
            .rev()
            .take(3)
            .map(|(t, e, _)| (t.clone(), *e))
            .collect();
        if !messages.is_empty() {
            m3::snackbars(ctx, &messages);
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn show_confirmation(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.pending else { return };
        match m3::dialog(
            ctx,
            c.icon,
            &c.title,
            &c.message,
            &c.confirm_label,
            c.destructive,
        ) {
            DialogResult::Open => {}
            DialogResult::Dismissed => self.pending = None,
            DialogResult::Confirmed => {
                if let Some(c) = self.pending.take() {
                    self.manager.send(c.command);
                }
            }
        }
    }

    /// Puts the UI into the state a screenshot needs (`--screenshots`).
    fn stage(&mut self, shot: Shot) {
        match shot {
            Shot::Tab(tab) => self.tab = tab,
            Shot::CustomEq => {
                self.tab = Tab::Sound;
                self.drafts
                    .custom_eq
                    .get_or_insert([4, 3, 1, 0, -1, 0, 1, 2, 3, 2]);
            }
            Shot::Dialog => {
                self.tab = Tab::Settings;
                self.drafts.custom_eq = None;
                self.pending.get_or_insert_with(|| {
                    Confirmation::new(
                        icons::ICON_DEVICE_RESET,
                        "Factory reset?",
                        "This erases all settings and every pairing. You will need to pair the earbuds again.",
                        "Reset",
                        Command::FactoryReset,
                    )
                    .destructive()
                });
            }
        }
    }

    /// Left navigation rail: app mark + one destination per tab.
    fn rail(&mut self, ui: &mut egui::Ui, show_tabs: bool) {
        let s = Scheme::of(ui);
        ui.vertical_centered(|ui| {
            ui.add_space(16.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 16, s.primary_container);
            m3::paint_icon(
                ui,
                icons::ICON_EARBUDS,
                rect.center(),
                28.0,
                s.on_primary_container,
            );
            ui.add_space(24.0);
            if show_tabs {
                for tab in Tab::ALL {
                    if m3::nav_item(ui, tab.icon(), tab.label(), self.tab == tab).clicked() {
                        self.tab = tab;
                    }
                    ui.add_space(4.0);
                }
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.collect_notices();
        let snap = self.manager.snapshot();
        let profile = snap
            .device
            .as_ref()
            .map_or(&profile::GENERIC, |d| d.profile);
        if let Some(shot) = self
            .capture
            .as_mut()
            .and_then(|c| c.step(&ctx, snap.is_connected()))
        {
            self.stage(shot);
        }
        let s = Scheme::of(ui);
        let mut actions = Vec::new();

        egui::Panel::left(Id::new("rail"))
            .exact_size(88.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(s.surface))
            .show(ui, |ui| self.rail(ui, snap.device.is_some()));

        let mut view = ViewCtx {
            snap: &snap,
            profile,
            drafts: &mut self.drafts,
            appearance: self.prefs.appearance,
            actions: &mut actions,
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(s.surface))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // A centred column, so wide windows do not stretch the cards.
                        let width = ui.available_width();
                        let column = (width - 24.0).min(CONTENT_MAX_WIDTH);
                        let side = ((width - column) / 2.0).max(8.0);
                        ui.add_space(16.0);
                        ui.horizontal_top(|ui| {
                            ui.add_space(side - 8.0);
                            ui.vertical(|ui| {
                                ui.set_width(column);
                                ui.spacing_mut().item_spacing.y = 12.0;
                                if snap.device.is_none() {
                                    views::searching::show(ui, &mut view);
                                } else {
                                    views::header::show(ui, &mut view);
                                    views::show_tab(ui, self.tab, &mut view);
                                }
                            });
                        });
                        ui.add_space(32.0);
                    });
            });

        for action in actions {
            self.run(&ctx, action);
        }
        self.show_snackbars(&ctx);
        self.show_confirmation(&ctx);
        if snap.link.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
