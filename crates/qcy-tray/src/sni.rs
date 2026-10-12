//! Linux: a StatusNotifierItem (tray icon + menu) for KDE Plasma and other
//! desktops implementing the freedesktop/KDE tray protocol, via `ksni`.

use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Category, Status, ToolTip, TrayMethods};
use qcy_client::manager::{Manager, Snapshot};
use tokio::sync::mpsc;
use tracing::info;

use crate::menu::{self, Action, Entry};
use crate::window::GuiWindow;

struct QcyTray {
    manager: Manager,
    snapshot: Snapshot,
    /// Actions the menu callbacks cannot handle themselves.
    local: mpsc::UnboundedSender<Action>,
}

impl QcyTray {
    fn trigger(&mut self, action: &Action) {
        match action {
            Action::OpenWindow | Action::Quit => {
                self.local.send(action.clone()).ok();
            }
            other => menu::perform(other, &self.snapshot, &self.manager),
        }
    }
}

impl ksni::Tray for QcyTray {
    fn id(&self) -> String {
        "qcy-tray".into()
    }

    fn title(&self) -> String {
        "QCY Earbuds".into()
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    /// Installed into the icon theme by `packaging/install.sh`; hosts fall
    /// back to [`Self::icon_pixmap`] when it is missing.
    fn icon_name(&self) -> String {
        "qcy-earbuds".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![app_icon()]
    }

    fn status(&self) -> Status {
        if self.snapshot.is_connected() {
            Status::Active
        } else {
            Status::Passive
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let content = menu::content(&self.snapshot);
        ToolTip {
            icon_name: "qcy-earbuds".into(),
            title: content.title,
            description: content.tooltip.join("\n"),
            ..Default::default()
        }
    }

    /// Left click opens the control panel.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.trigger(&Action::OpenWindow);
    }

    /// Mouse wheel over the icon cycles noise control.
    fn scroll(&mut self, delta: i32, _orientation: ksni::Orientation) {
        if let Some(action) = menu::cycle_noise(&self.snapshot, delta) {
            self.trigger(&action);
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        menu::content(&self.snapshot)
            .entries
            .into_iter()
            .map(convert)
            .collect()
    }
}

/// The application icon as StatusNotifierItem pixmap (ARGB32, network byte
/// order), converted from `assets/icon-64.rgba`.
fn app_icon() -> ksni::Icon {
    const RGBA: &[u8] = include_bytes!("../../../assets/icon-64.rgba");
    let data = RGBA
        .chunks_exact(4)
        .flat_map(|px| [px[3], px[0], px[1], px[2]])
        .collect();
    ksni::Icon {
        width: 64,
        height: 64,
        data,
    }
}

/// Maps a platform-neutral entry onto `ksni`'s menu types.
fn convert(entry: Entry) -> MenuItem<QcyTray> {
    match entry {
        Entry::Label(label) => StandardItem {
            label,
            enabled: false,
            ..Default::default()
        }
        .into(),
        Entry::Separator => MenuItem::Separator,
        Entry::Radio { options, selected } => {
            let actions: Vec<Action> = options.iter().map(|(_, a)| a.clone()).collect();
            RadioGroup {
                selected: selected.unwrap_or(usize::MAX),
                select: Box::new(move |tray: &mut QcyTray, index| {
                    if let Some(action) = actions.get(index) {
                        tray.trigger(action);
                    }
                }),
                options: options
                    .into_iter()
                    .map(|(label, _)| RadioItem {
                        label,
                        ..Default::default()
                    })
                    .collect(),
            }
            .into()
        }
        Entry::Check {
            label,
            checked,
            action,
        } => CheckmarkItem {
            label,
            checked,
            activate: Box::new(move |tray: &mut QcyTray| tray.trigger(&action)),
            ..Default::default()
        }
        .into(),
        Entry::Submenu { label, entries } => SubMenu {
            label,
            submenu: entries.into_iter().map(convert).collect(),
            ..Default::default()
        }
        .into(),
        Entry::Button {
            label,
            icon,
            action,
        } => StandardItem {
            label,
            icon_name: icon.into(),
            activate: Box::new(move |tray: &mut QcyTray| tray.trigger(&action)),
            ..Default::default()
        }
        .into(),
    }
}

/// Runs the tray until the user picks "Quit" or presses Ctrl+C.
pub async fn run(manager: Manager) -> anyhow::Result<()> {
    let (local, mut local_rx) = mpsc::unbounded_channel();
    let handle = QcyTray {
        manager: manager.clone(),
        snapshot: manager.snapshot(),
        local,
    }
    .spawn()
    .await
    .map_err(|e| {
        anyhow::anyhow!(
            "cannot register the tray icon (is a StatusNotifier host such as Plasma running?): {e}"
        )
    })?;

    let mut snapshots = manager.subscribe();
    let mut window = GuiWindow::default();
    loop {
        tokio::select! {
            changed = snapshots.changed() => {
                if changed.is_err() {
                    break;
                }
                let snap = snapshots.borrow_and_update().clone();
                handle.update(|t| t.snapshot = snap).await;
            }
            action = local_rx.recv() => match action {
                Some(Action::OpenWindow) => window.open(),
                Some(Action::Quit) | None => break,
                Some(_) => {}
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    info!("quitting");
    handle.shutdown().await;
    Ok(())
}
