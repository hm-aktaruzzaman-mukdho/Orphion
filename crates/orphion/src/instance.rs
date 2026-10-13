//! Single instances: one tray and one window per user session, and starting
//! the tray in the background.
//!
//! An [`Instance`] lock is held for as long as the tray (or the window) runs:
//!
//! * Linux: an exclusive `flock` on `$XDG_RUNTIME_DIR/orphion/<name>.lock`
//!   (the config directory if `XDG_RUNTIME_DIR` is unset). The kernel drops
//!   it when the process ends, however it ends.
//! * Windows: a named mutex `Local\io.github.orphion.<Name>`, which exists
//!   while any process has it open.
//!
//! The window decides whether to start the tray by briefly *probing* the
//! tray lock; the tray then *holds* it. Should a tray that is just starting
//! meet such a probe, it retries for a moment ([`TRAY_START_GRACE`]) before
//! concluding that another tray runs, so concurrent launches always end with
//! exactly one tray.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tracing::{debug, warn};

/// How long a starting tray keeps trying to take the lock.
pub const TRAY_START_GRACE: Duration = Duration::from_millis(500);
const RETRY_EVERY: Duration = Duration::from_millis(50);

/// What a lock guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Tray,
    Window,
}

impl Kind {
    /// Name of the lock file.
    #[cfg(unix)]
    fn name(self) -> &'static str {
        match self {
            Kind::Tray => "tray",
            Kind::Window => "window",
        }
    }
}

/// Proof that this process is the only one of its kind; released on drop.
///
/// When the platform cannot provide a lock (no writable lock directory, say)
/// the program runs as if it were the only instance, with nothing held.
#[derive(Debug)]
pub struct Instance {
    #[cfg(unix)]
    _file: Option<std::fs::File>,
    #[cfg(windows)]
    handle: Option<windows::Win32::Foundation::HANDLE>,
}

impl Instance {
    /// Takes the lock for `kind` if nobody holds it.
    pub fn acquire(kind: Kind) -> Option<Instance> {
        platform::acquire(kind)
    }

    /// Like [`Self::acquire`], but keeps trying for `grace` first.
    pub fn acquire_within(kind: Kind, grace: Duration) -> Option<Instance> {
        retry(grace, || Self::acquire(kind))
    }

    /// Whether another process holds the lock for `kind` right now.
    pub fn is_running(kind: Kind) -> bool {
        Self::acquire(kind).is_none()
    }
}

/// Calls `attempt` until it succeeds or `grace` has passed.
fn retry<T>(grace: Duration, mut attempt: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(value) = attempt() {
            return Some(value);
        }
        if start.elapsed() >= grace {
            return None;
        }
        std::thread::sleep(RETRY_EVERY);
    }
}

#[cfg(windows)]
impl Drop for Instance {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // SAFETY: the handle was returned by CreateMutexW and is closed once.
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(handle);
            }
        }
    }
}

/// Starts `orphion --tray` in the background, detached from this process:
/// it keeps running after the window closes and has no console.
pub fn spawn_tray() {
    let exe = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from(format!("orphion{}", std::env::consts::EXE_SUFFIX)));
    let mut command = Command::new(exe);
    command
        .arg("--tray")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group: Ctrl+C in a terminal that started the
        // window does not reach the tray.
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    match command.spawn() {
        Ok(child) => debug!(pid = child.id(), "tray started"),
        Err(e) => warn!("cannot start the tray: {e}"),
    }
}

/// Brings an already open control panel to the front, where the platform
/// allows it (Windows). Wayland does not let another process do that.
pub fn raise_window() {
    #[cfg(windows)]
    platform::raise_window();
}

#[cfg(unix)]
mod platform {
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::path::{Path, PathBuf};

    use super::{Instance, Kind};

    /// `$XDG_RUNTIME_DIR/orphion`, or the config directory without it.
    fn lock_dir() -> Option<PathBuf> {
        match std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
            Some(dir) => Some(PathBuf::from(dir).join("orphion")),
            None => crate::prefs::config_dir().map(|d| d.join("orphion")),
        }
    }

    pub fn acquire(kind: Kind) -> Option<Instance> {
        match lock_dir() {
            Some(dir) => lock_file(&dir.join(format!("{}.lock", kind.name()))),
            None => Some(Instance { _file: None }),
        }
    }

    /// An exclusive, non-blocking `flock` on `path`. Without a usable lock
    /// file the program runs anyway (as if it were the only instance).
    pub(super) fn lock_file(path: &Path) -> Option<Instance> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let file: File = match OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
        {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(path = %path.display(), "cannot open lock file: {e}");
                return Some(Instance { _file: None });
            }
        };
        // SAFETY: flock on a file descriptor we own.
        let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
        locked.then_some(Instance { _file: Some(file) })
    }
}

#[cfg(windows)]
mod platform {
    use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    use windows::core::{HSTRING, w};

    use super::{Instance, Kind};

    pub fn acquire(kind: Kind) -> Option<Instance> {
        let name = match kind {
            Kind::Tray => "Local\\io.github.orphion.Tray",
            Kind::Window => "Local\\io.github.orphion.Window",
        };
        acquire_named(name)
    }

    pub(super) fn acquire_named(name: &str) -> Option<Instance> {
        // SAFETY: plain Win32 calls; the handle is owned by the Instance.
        unsafe {
            let handle = match CreateMutexW(None, false, &HSTRING::from(name)) {
                Ok(h) => h,
                // Cannot tell: behave as the only instance.
                Err(e) => {
                    tracing::warn!("cannot create mutex {name}: {e}");
                    return Some(Instance { handle: None });
                }
            };
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                None
            } else {
                Some(Instance {
                    handle: Some(handle),
                })
            }
        }
    }

    /// Finds the control panel by winit's window class and its title.
    pub fn raise_window() {
        // SAFETY: plain Win32 calls on a window handle we only look at.
        unsafe {
            if let Ok(hwnd) = FindWindowW(w!("Window Class"), w!("Orphion")) {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_lock_is_held_once() {
        let path = std::env::temp_dir()
            .join(format!("orphion-test-{}", std::process::id()))
            .join("test.lock");
        let first = platform::lock_file(&path).expect("first lock");
        assert!(
            platform::lock_file(&path).is_none(),
            "second lock must fail"
        );
        drop(first);
        assert!(
            platform::lock_file(&path).is_some(),
            "free again after drop"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn a_lock_is_held_once() {
        let name = format!("Local\\io.github.orphion.Test{}", std::process::id());
        let first = platform::acquire_named(&name).expect("first lock");
        assert!(
            platform::acquire_named(&name).is_none(),
            "second lock must fail"
        );
        drop(first);
        assert!(
            platform::acquire_named(&name).is_some(),
            "free again after drop"
        );
    }

    #[test]
    fn retrying_waits_then_gives_up() {
        let mut calls = 0;
        assert_eq!(
            retry(Duration::ZERO, || {
                calls += 1;
                None::<()>
            }),
            None
        );
        assert_eq!(calls, 1);

        let mut calls = 0;
        let got = retry(Duration::from_secs(5), || {
            calls += 1;
            (calls == 3).then_some(calls)
        });
        assert_eq!(got, Some(3));

        let start = Instant::now();
        assert_eq!(retry(Duration::from_millis(120), || None::<()>), None);
        assert!(start.elapsed() >= Duration::from_millis(120));
    }
}
