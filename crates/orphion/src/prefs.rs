//! Tiny persisted UI preferences (`gui.conf` in the platform's config folder).
//!
//! A plain `key=value` file keeps this dependency-free; unknown keys and
//! malformed lines are ignored.

use std::path::PathBuf;

use eframe::egui::{Color32, ThemePreference};

use crate::material::scheme::{self, DEFAULT_SEED};

/// Light / dark mode choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }

    pub fn theme_preference(self) -> ThemePreference {
        match self {
            Appearance::System => ThemePreference::System,
            Appearance::Light => ThemePreference::Light,
            Appearance::Dark => ThemePreference::Dark,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prefs {
    pub appearance: Appearance,
    /// Seed colour of the Material scheme.
    pub accent: Color32,
    /// Start the tray icon together with the window (`tray=true|false`).
    pub keep_in_tray: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            accent: DEFAULT_SEED,
            keep_in_tray: true,
        }
    }
}

/// The platform's per-user config directory: `%APPDATA%` on Windows,
/// `$XDG_CONFIG_HOME` (or `~/.config`) elsewhere.
pub fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
}

impl Prefs {
    /// `%APPDATA%\orphion\gui.conf` on Windows,
    /// `$XDG_CONFIG_HOME/orphion/gui.conf` (or `~/.config/…`) elsewhere.
    fn path() -> Option<PathBuf> {
        Some(config_dir()?.join("orphion").join("gui.conf"))
    }

    /// Where versions before the rename to Orphion kept the file.
    fn legacy_path() -> Option<PathBuf> {
        Some(config_dir()?.join("qcy-earbuds").join("gui.conf"))
    }

    /// Loads the saved preferences; `--appearance` on the command line
    /// overrides the theme for this run without saving it.
    pub fn load() -> Self {
        let mut prefs = Self::load_file();
        if let Some(a) = crate::arg_value("--appearance")
            .and_then(|v| Appearance::ALL.into_iter().find(|a| a.key() == v))
        {
            prefs.appearance = a;
        }
        prefs
    }

    fn load_file() -> Self {
        // Settings saved under the old name are picked up once and then
        // saved under the new one.
        let text = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .or_else(|| Self::legacy_path().and_then(|p| std::fs::read_to_string(p).ok()));
        text.map_or_else(Self::default, |t| Self::parse(&t))
    }

    /// Reads `key=value` lines; anything unknown or malformed is ignored.
    fn parse(text: &str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            match line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
                Some(("appearance", value)) => {
                    if let Some(a) = Appearance::ALL.into_iter().find(|a| a.key() == value) {
                        prefs.appearance = a;
                    }
                }
                Some(("accent", value)) => {
                    if let Some(c) = scheme::from_hex(value) {
                        prefs.accent = c;
                    }
                }
                Some(("tray", "true")) => prefs.keep_in_tray = true,
                Some(("tray", "false")) => prefs.keep_in_tray = false,
                _ => {}
            }
        }
        prefs
    }

    /// The file contents for these preferences.
    fn to_text(self) -> String {
        format!(
            "appearance={}\naccent={}\ntray={}\n",
            self.appearance.key(),
            scheme::to_hex(self.accent),
            self.keep_in_tray
        )
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, self.to_text()));
        if let Err(e) = result {
            tracing::warn!("cannot save preferences to {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_defaults_to_on() {
        assert!(Prefs::default().keep_in_tray);
        assert!(Prefs::parse("appearance=dark\n").keep_in_tray);
        assert!(Prefs::parse("tray=maybe\n").keep_in_tray);
    }

    #[test]
    fn tray_round_trips() {
        let p = Prefs::parse("appearance=dark\naccent=#00897B\ntray=false\n");
        assert!(!p.keep_in_tray);
        assert_eq!(p.appearance, Appearance::Dark);
        assert_eq!(Prefs::parse(&p.to_text()), p);
        assert!(p.to_text().contains("tray=false"));
        let on = Prefs {
            keep_in_tray: true,
            ..p
        };
        assert!(Prefs::parse(&on.to_text()).keep_in_tray);
    }
}
