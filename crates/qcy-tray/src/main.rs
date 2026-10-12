//! `qcy-tray` – tray applet for QCY earbuds.
//!
//! Shows battery levels in the tooltip and offers quick noise control,
//! gaming mode and EQ presets from the menu. Scrolling over the icon cycles
//! noise control; clicking it opens the full control panel (`qcy-gui`).
//!
//! It keeps reconnecting in the background, so it can be autostarted with the
//! desktop session and simply picks the earbuds up whenever they are in use.

mod tray;

use std::path::PathBuf;
use std::process::{Child, Command as Process};

use anyhow::{Context, Result};
use ksni::TrayMethods;
use qcy_client::Selector;
use qcy_client::manager::Manager;
use tokio::sync::mpsc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::tray::{QcyTray, TrayRequest};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,qcy_client=info")),
        )
        .init();

    let manager = Manager::spawn();
    manager.connect(Selector::Auto);

    let (requests, mut request_rx) = mpsc::unbounded_channel();
    let handle = QcyTray {
        manager: manager.clone(),
        snapshot: manager.snapshot(),
        requests,
    }
    .spawn()
    .await
    .context("cannot register the tray icon (is a StatusNotifier host such as Plasma running?)")?;

    let mut snapshots = manager.subscribe();
    let mut window: Option<Child> = None;
    loop {
        tokio::select! {
            changed = snapshots.changed() => {
                if changed.is_err() {
                    break;
                }
                let snap = snapshots.borrow_and_update().clone();
                handle.update(|t| t.snapshot = snap).await;
            }
            request = request_rx.recv() => match request {
                Some(TrayRequest::OpenWindow) => open_window(&mut window),
                Some(TrayRequest::Quit) | None => break,
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }

    info!("quitting");
    handle.shutdown().await;
    manager.shutdown(std::time::Duration::from_secs(2)).await;
    Ok(())
}

/// Starts the control panel unless it is already open.
fn open_window(window: &mut Option<Child>) {
    if let Some(child) = window {
        if matches!(child.try_wait(), Ok(None)) {
            // Still running; Wayland does not let us raise it.
            return;
        }
    }
    match Process::new(gui_binary()).spawn() {
        Ok(child) => *window = Some(child),
        Err(e) => warn!("cannot start qcy-gui: {e}"),
    }
}

/// `qcy-gui` next to this executable, otherwise from `PATH`.
fn gui_binary() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("qcy-gui")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("qcy-gui"))
}
