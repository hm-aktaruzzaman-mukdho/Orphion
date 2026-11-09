//! Windows: a notification-area icon with a context menu, via `tray-icon`.
//!
//! Windows delivers tray and menu events as window messages to the thread
//! that created the icon, so this runs a small message loop on the main
//! thread while the connection manager lives on the tokio runtime. The loop
//! sleeps until a message arrives; device state changes are announced to it
//! with a thread message, so it uses no CPU while nothing happens.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context;
use qcy_client::manager::{Manager, Snapshot};
use tracing::info;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, PostThreadMessageW,
    QS_ALLINPUT, TranslateMessage, WM_APP, WM_QUIT,
};

use super::menu::{self, Action, Entry};
use super::window::GuiWindow;

/// 32×32 RGBA rendering of `assets/orphion.png`.
const ICON_RGBA: &[u8] = include_bytes!("../../../../assets/icon-32.rgba");
/// Longest sleep between looks at the device state. State changes wake the
/// loop at once; this only covers a wake-up lost while a menu is open (modal
/// loops drop thread messages).
const MAX_WAIT: Duration = Duration::from_secs(1);
/// Thread message meaning "the device state changed".
const WM_STATE_CHANGED: u32 = WM_APP + 1;
/// Windows truncates tray tooltips at 127 characters.
const TOOLTIP_MAX: usize = 127;

/// Runs the tray until the user picks "Quit". Must be called on the main
/// thread; `manager` runs on the tokio runtime behind `runtime`.
pub fn run(manager: Manager, runtime: &tokio::runtime::Handle) -> anyhow::Result<()> {
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
    // Wake this thread whenever the device state changes.
    // SAFETY: plain Win32 call.
    let thread = unsafe { GetCurrentThreadId() };
    let mut changes = manager.subscribe();
    let notifier = runtime.spawn(async move {
        while changes.changed().await.is_ok() {
            // SAFETY: posting to a thread that has a message queue (the
            // tray icon's window was created on it); fails harmlessly once
            // the thread is gone.
            unsafe {
                let _ = PostThreadMessageW(thread, WM_STATE_CHANGED, WPARAM(0), LPARAM(0));
            }
        }
    });
    let result = event_loop(&tray, &manager, &mut snapshot, &mut snapshots, &mut actions);
    notifier.abort();
    result
}

fn event_loop(
    tray: &TrayIcon,
    manager: &Manager,
    snapshot: &mut Snapshot,
    snapshots: &mut tokio::sync::watch::Receiver<Snapshot>,
    actions: &mut HashMap<String, Action>,
) -> anyhow::Result<()> {
    let mut window = GuiWindow::default();
    loop {
        // Sleep until a window message (input, tray events, a state change
        // notice) arrives.
        // SAFETY: waits on this thread's message queue only.
        unsafe {
            MsgWaitForMultipleObjects(None, false, MAX_WAIT.as_millis() as u32, QS_ALLINPUT);
        }
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
                Some(action) => menu::perform(action, snapshot, manager),
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
            *snapshot = snapshots.borrow_and_update().clone();
            refresh(tray, snapshot, actions)?;
        }
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
