//! Opening the control panel from the tray.

use std::path::PathBuf;
use std::process::{Child, Command};

use tracing::warn;

/// The control panel process started by the tray, if any.
#[derive(Default)]
pub struct GuiWindow {
    child: Option<Child>,
}

impl GuiWindow {
    /// Starts the control panel unless it is already running. The window is
    /// told not to start a tray, since this one is running; if a window was
    /// opened another way, the new process notices and exits (bringing that
    /// window to the front on Windows).
    pub fn open(&mut self) {
        if let Some(child) = &mut self.child {
            if matches!(child.try_wait(), Ok(None)) {
                // Still open. Bringing it to the front is not possible
                // from another process on Wayland, so leave it be.
                return;
            }
        }
        match Command::new(this_program()).arg("--no-tray").spawn() {
            Ok(child) => self.child = Some(child),
            Err(e) => warn!("cannot start the control panel: {e}"),
        }
    }
}

/// This executable, which shows the window when started without `--tray`;
/// `orphion` from `PATH` if its location is unknown.
fn this_program() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from(format!("orphion{}", std::env::consts::EXE_SUFFIX)))
}
