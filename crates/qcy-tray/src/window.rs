//! Launching the control panel (`qcy-gui`) from the tray.

use std::path::PathBuf;
use std::process::{Child, Command};

use tracing::warn;

/// The control panel process started by the tray, if any.
#[derive(Default)]
pub struct GuiWindow {
    child: Option<Child>,
}

impl GuiWindow {
    /// Starts the control panel unless it is already running.
    pub fn open(&mut self) {
        if let Some(child) = &mut self.child {
            if matches!(child.try_wait(), Ok(None)) {
                // Still open. Bringing it to the front is not possible
                // from another process on Wayland, so leave it be.
                return;
            }
        }
        match Command::new(gui_binary()).spawn() {
            Ok(child) => self.child = Some(child),
            Err(e) => warn!("cannot start qcy-gui: {e}"),
        }
    }
}

/// `qcy-gui` (`qcy-gui.exe` on Windows) next to this executable, otherwise
/// from `PATH`.
fn gui_binary() -> PathBuf {
    let file = format!("qcy-gui{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&file)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(file))
}
