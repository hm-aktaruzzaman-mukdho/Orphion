//! Windows: a notification-area icon with a context menu, via `tray-icon`.
//!
//! Windows delivers tray and menu events as window messages to the thread
//! that created the icon, so this runs a small message loop on the main
//! thread while the connection manager lives on the tokio runtime.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context;
use qcy_client::manager::{Manager, Snapshot};
use tracing::info;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT,
};

use crate::menu::{self, Action, Entry};
use crate::window::GuiWindow;

/// 32×32 RGBA rendering of `assets/qcy-earbuds.png`.
const ICON_RGBA: &[u8] = include_bytes!("../../../assets/icon-32.rgba");
/// How often to look for state changes between window messages.
const TICK: Duration = Duration::from_millis(40);
/// Windows truncates tray tooltips at 127 characters.
const TOOLTIP_MAX: usize = 127;

/// Runs the tray until the user picks "Quit". Must be called on the main
/// thread; `manager` runs on a tokio runtime owned by the caller.
pub fn run(manager: Manager) -> anyhow::Result<()> {
    let icon = Icon::from_rgba(ICON_RGBA.to_vec(), 32, 32).context("invalid tray icon")?;
    let mut snapshot = manager.snapshot();
    let mut actions = HashMap::new();
    let content = menu::content(&snapshot);

    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip(tooltip(&content))
        .with_menu(Box::new(build_menu(&content.entries, &mut actions)?))
        .with_menu_on_left_click(false)
        .build()
        .context("cannot create the tray icon")?;

    let mut snapshots = manager.subscribe();
    let mut window = GuiWindow::default();
    loop {
        if !pump_messages() {
            break;
        }

        // Menu clicks.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match actions.get(&event.id.0) {
                Some(Action::Quit) => {
                    info!("quitting");
                    return Ok(());
                }
                Some(Action::OpenWindow) => window.open(),
                Some(action) => menu::perform(action, &snapshot, &manager),
                None => {}
            }
        }

        // Left click on the icon opens the control panel.
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window.open();
            }
        }

        // Device state changed: rebuild tooltip and menu.
        if snapshots.has_changed().unwrap_or(false) {
            snapshot = snapshots.borrow_and_update().clone();
            refresh(&tray, &snapshot, &mut actions)?;
        }

        std::thread::sleep(TICK);
    }
    Ok(())
}

fn refresh(
    tray: &TrayIcon,
    snapshot: &Snapshot,
    actions: &mut HashMap<String, Action>,
) -> anyhow::Result<()> {
    let content = menu::content(snapshot);
    actions.clear();
    tray.set_menu(Some(Box::new(build_menu(&content.entries, actions)?)));
    tray.set_tooltip(Some(tooltip(&content)))?;
    Ok(())
}

fn tooltip(content: &menu::Content) -> String {
    let mut text = std::iter::once(content.title.clone())
        .chain(content.tooltip.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n");
    if text.chars().count() > TOOLTIP_MAX {
        text = text.chars().take(TOOLTIP_MAX - 1).collect::<String>() + "…";
    }
    text
}

/// Builds the native menu; clickable items get ids that map to `actions`.
fn build_menu(entries: &[Entry], actions: &mut HashMap<String, Action>) -> anyhow::Result<Menu> {
    let menu = Menu::new();
    append_entries(&menu, entries, actions)?;
    Ok(menu)
}

/// Something items can be appended to (the root menu or a submenu).
trait Container {
    fn add(&self, item: &dyn tray_icon::menu::IsMenuItem) -> tray_icon::menu::Result<()>;
}

impl Container for Menu {
    fn add(&self, item: &dyn tray_icon::menu::IsMenuItem) -> tray_icon::menu::Result<()> {
        self.append(item)
    }
}

impl Container for Submenu {
    fn add(&self, item: &dyn tray_icon::menu::IsMenuItem) -> tray_icon::menu::Result<()> {
        self.append(item)
    }
}

fn append_entries(
    target: &dyn Container,
    entries: &[Entry],
    actions: &mut HashMap<String, Action>,
) -> anyhow::Result<()> {
    for entry in entries {
        match entry {
            Entry::Label(text) => target.add(&MenuItem::new(text, false, None))?,
            Entry::Separator => target.add(&PredefinedMenuItem::separator())?,
            Entry::Radio { options, selected } => {
                for (i, (label, action)) in options.iter().enumerate() {
                    let id = register(actions, action);
                    target.add(&CheckMenuItem::with_id(
                        id,
                        label,
                        true,
                        *selected == Some(i),
                        None,
                    ))?;
                }
            }
            Entry::Check {
                label,
                checked,
                action,
            } => {
                let id = register(actions, action);
                target.add(&CheckMenuItem::with_id(id, label, true, *checked, None))?;
            }
            Entry::Button { label, action, .. } => {
                let id = register(actions, action);
                target.add(&MenuItem::with_id(id, label, true, None))?;
            }
            Entry::Submenu { label, entries } => {
                let sub = Submenu::new(label, true);
                append_entries(&sub, entries, actions)?;
                target.add(&sub)?;
            }
        }
    }
    Ok(())
}

/// Remembers `action` under a fresh menu item id.
fn register(actions: &mut HashMap<String, Action>, action: &Action) -> String {
    let id = format!("action-{}", actions.len());
    actions.insert(id.clone(), action.clone());
    id
}

/// Dispatches pending window messages; returns `false` once the thread was
/// asked to quit.
fn pump_messages() -> bool {
    let mut msg = MSG::default();
    // SAFETY: plain Win32 message loop on the thread that owns the tray icon.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}
