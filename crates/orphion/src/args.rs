//! Command line: which mode this run is in.
//!
//! Kept free of side effects so the decisions can be unit tested.

/// What this run of the program does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `--tray`: only the tray icon, no window and no GPU stack.
    Tray,
    /// The control panel window.
    Window,
}

/// The mode selected by `args` (without the program name).
pub fn mode(args: &[String]) -> Mode {
    if has(args, "--tray") {
        Mode::Tray
    } else {
        Mode::Window
    }
}

/// Development runs (`--demo`, `--screenshots`): they never start a tray
/// and do not count as the open window.
pub fn is_dev_run(args: &[String]) -> bool {
    has(args, "--demo") || has(args, "--screenshots")
}

/// Whether the window should make sure a tray is running: the user keeps
/// Orphion in the tray, this run did not opt out with `--no-tray`, and it is
/// not a development run.
pub fn starts_tray(args: &[String], keep_in_tray: bool) -> bool {
    keep_in_tray && !has(args, "--no-tray") && !is_dev_run(args)
}

/// Whether this window run takes part in "one window at a time".
pub fn single_window(args: &[String]) -> bool {
    !is_dev_run(args)
}

/// Whether `flag` is on the command line.
pub fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Value following `flag`, e.g. `--device <MAC>`.
pub fn value(args: &[String], flag: &str) -> Option<String> {
    args.iter().skip_while(|a| *a != flag).nth(1).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn modes() {
        assert_eq!(mode(&args("")), Mode::Window);
        assert_eq!(mode(&args("--tray")), Mode::Tray);
        assert_eq!(mode(&args("--device AA:BB:CC:DD:EE:FF --tray")), Mode::Tray);
        assert_eq!(mode(&args("--demo 19786")), Mode::Window);
    }

    #[test]
    fn tray_start() {
        assert!(starts_tray(&args(""), true));
        assert!(!starts_tray(&args(""), false), "setting off");
        assert!(!starts_tray(&args("--no-tray"), true));
        assert!(!starts_tray(&args("--demo"), true));
        assert!(!starts_tray(&args("--demo 23872"), true));
        assert!(!starts_tray(&args("--screenshots shots"), true));
        assert!(starts_tray(&args("--device AA:BB:CC:DD:EE:FF"), true));
    }

    #[test]
    fn one_window() {
        assert!(single_window(&args("")));
        assert!(single_window(&args("--no-tray")));
        assert!(!single_window(&args("--demo --screenshots out")));
    }

    #[test]
    fn values() {
        let a = args("--demo 23872 --appearance dark");
        assert_eq!(value(&a, "--demo").as_deref(), Some("23872"));
        assert_eq!(value(&a, "--appearance").as_deref(), Some("dark"));
        assert_eq!(value(&a, "--device"), None);
        assert!(has(&a, "--demo") && !has(&a, "--tray"));
    }
}
