// A background program: no console window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

//! `qcy-tray` – tray applet for QCY earbuds.
//!
//! Shows battery levels in the tooltip and offers quick noise control,
//! gaming mode and EQ presets from the menu; clicking the icon opens the full
//! control panel (`qcy-gui`). On Linux, scrolling over the icon also cycles
//! noise control.
//!
//! It keeps reconnecting in the background, so it can be started with the
//! desktop session and simply picks the earbuds up whenever they are in use.
//!
//! * [`menu`]: what the tray shows and does (platform-neutral)
//! * `sni`: Linux renderer (KDE / StatusNotifierItem)
//! * `win`: Windows renderer (notification area)

mod menu;
#[cfg(target_os = "linux")]
mod sni;
#[cfg(windows)]
mod win;
mod window;

use std::time::Duration;

use anyhow::Result;
use qcy_client::Selector;
use qcy_client::manager::Manager;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,qcy_client=info")),
        )
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let manager = runtime.block_on(async { Manager::spawn() });
    manager.connect(Selector::Auto);

    #[cfg(target_os = "linux")]
    let result = runtime.block_on(sni::run(manager.clone()));
    // The Windows tray must live on this (the main) thread.
    #[cfg(windows)]
    let result = win::run(manager.clone());

    // Release the BLE link so the phone app can connect again.
    runtime.block_on(manager.shutdown(Duration::from_secs(2)));
    runtime.shutdown_timeout(Duration::from_millis(500));
    result
}
