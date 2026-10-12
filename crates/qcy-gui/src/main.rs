//! `qcy-gui` – desktop control panel for QCY earbuds.
//!
//! Threading model: egui owns the main thread; a tokio runtime runs in the
//! background and hosts the [`Manager`], which talks to BlueZ. The UI only
//! reads [`Snapshot`](qcy_client::manager::Snapshot)s and queues commands, so
//! it never blocks on Bluetooth.
//!
//! Options:
//!
//! * `--device <MAC>`: use specific earbuds (audio or BLE address)
//! * `--demo`: simulated earbuds (captured MeloBuds Pro data), no Bluetooth
//! * `--appearance system|light|dark`: override the saved theme for this run
//! * `--screenshots <DIR>`: save every tab as `DIR/<tab>.ppm` once connected, then quit

mod app;
mod capture;
mod material;
mod prefs;
mod views;
mod widgets;

use anyhow::Result;
use qcy_client::Selector;
use qcy_client::manager::Manager;
use tracing_subscriber::EnvFilter;

const APP_ID: &str = "io.github.qcy_linux.Control";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,qcy=info")),
        )
        .init();

    let selector = arg_value("--device")
        .and_then(|a| a.parse().ok())
        .map_or(Selector::Auto, Selector::Address);
    let demo = std::env::args().any(|a| a == "--demo");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let manager = runtime.block_on(async {
        if demo {
            Manager::spawn_demo()
        } else {
            Manager::spawn()
        }
    });
    manager.connect(selector);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("QCY Earbuds")
            .with_app_id(APP_ID)
            .with_inner_size([640.0, 860.0])
            .with_min_inner_size([500.0, 560.0]),
        ..Default::default()
    };

    let handle = runtime.handle().clone();
    let app_manager = manager.clone();
    let result = eframe::run_native(
        "QCY Earbuds",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, app_manager, handle)))),
    );

    // Release the BLE link so the phone app can connect again. A running
    // `qcy-tray` notices and reconnects on its own.
    runtime.block_on(manager.shutdown(std::time::Duration::from_secs(2)));
    runtime.shutdown_timeout(std::time::Duration::from_millis(500));
    result.map_err(|e| anyhow::anyhow!("{e}"))
}

/// Value following `flag` on the command line, e.g. `--device <MAC>`.
pub fn arg_value(flag: &str) -> Option<String> {
    std::env::args().skip_while(|a| a != flag).nth(1)
}
