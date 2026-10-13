// A GUI program: no console window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

//! `orphion` – control panel and tray icon for QCY earbuds.
//!
//! One program, two modes, picked before any window or GPU state exists:
//!
//! * the control panel window (default);
//! * `--tray`: only the tray icon ([`tray`]).
//!
//! Threading model of the window: egui owns the main thread; a tokio runtime
//! runs in the background and hosts the [`Manager`], which talks to the
//! Bluetooth stack. The UI only reads
//! [`Snapshot`](qcy_client::manager::Snapshot)s and queues commands, so it
//! never blocks on Bluetooth.
//!
//! Options:
//!
//! * `--tray`: run the tray icon instead of the window
//! * `--no-tray`: do not start the tray along with the window (it is started
//!   by default unless turned off under Settings → Appearance)
//! * `--device <MAC>`: use specific earbuds (audio or BLE address)
//! * `--demo [MODEL]`: simulated earbuds, no Bluetooth. `MODEL` is a model id
//!   from the catalogue (`orphionctl models`); the default is the MeloBuds Pro,
//!   simulated from data captured on real earbuds
//! * `--appearance system|light|dark`: override the saved theme for this run
//! * `--screenshots <DIR>`: save every tab as `DIR/<tab>.ppm` once connected, then quit
//!
//! `--demo` and `--screenshots` runs never start a tray and may run next to
//! the normal window; otherwise only one window and one tray run at a time
//! ([`instance`]).

mod app;
mod args;
mod capture;
mod instance;
mod material;
mod prefs;
mod tray;
mod views;
mod widgets;

use anyhow::Result;
use qcy_client::Selector;
use qcy_client::manager::Manager;
use tracing_subscriber::EnvFilter;

use crate::instance::{Instance, Kind};

/// Model simulated by `--demo` without a model id: the MeloBuds Pro.
const DEMO_MODEL: u16 = 19786;
const APP_ID: &str = "io.github.orphion.Orphion";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args::mode(&args) {
        args::Mode::Tray => tray::run(),
        args::Mode::Window => run_window(&args),
    }
}

/// The control panel window.
fn run_window(args: &[String]) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,qcy=info")),
        )
        .init();

    // One window at a time: a second launch shows the first one instead
    // (where the platform allows) and exits.
    let _window = if args::single_window(args) {
        match Instance::acquire(Kind::Window) {
            Some(lock) => Some(lock),
            None => {
                tracing::info!("the control panel is already open");
                instance::raise_window();
                return Ok(());
            }
        }
    } else {
        None
    };

    // Keep the tray running alongside, unless turned off.
    if args::starts_tray(args, prefs::Prefs::load().keep_in_tray)
        && !Instance::is_running(Kind::Tray)
    {
        instance::spawn_tray();
    }

    let selector = arg_value("--device")
        .and_then(|a| a.parse().ok())
        .map_or(Selector::Auto, Selector::Address);
    let demo = std::env::args().any(|a| a == "--demo").then(|| {
        arg_value("--demo")
            .and_then(|m| m.parse::<u16>().ok())
            .unwrap_or(DEMO_MODEL)
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let manager = runtime.block_on(async {
        if let Some(model) = demo {
            Manager::spawn_demo(model)
        } else {
            Manager::spawn()
        }
    });
    manager.connect(selector);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Orphion")
            .with_app_id(APP_ID)
            .with_inner_size([640.0, 860.0])
            .with_min_inner_size([500.0, 560.0])
            .with_icon(eframe::egui::IconData {
                rgba: include_bytes!("../../../assets/icon-64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };

    let handle = runtime.handle().clone();
    let tray_allowed = !args::is_dev_run(args);
    let app_manager = manager.clone();
    let result = eframe::run_native(
        "Orphion",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::App::new(
                cc,
                app_manager,
                handle,
                tray_allowed,
            )))
        }),
    );

    // Release the BLE link so the phone app can connect again. A running
    // tray notices and reconnects on its own.
    runtime.block_on(manager.shutdown(std::time::Duration::from_secs(2)));
    runtime.shutdown_timeout(std::time::Duration::from_millis(500));
    result.map_err(|e| anyhow::anyhow!("{e}"))
}

/// Value following `flag` on the command line, e.g. `--device <MAC>`.
pub fn arg_value(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    args::value(&args, flag)
}
