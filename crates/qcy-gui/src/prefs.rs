//! Tiny persisted UI preferences (`~/.config/qcy-earbuds/gui.conf`).
//!
//! A plain `key=value` file keeps this dependency-free; unknown keys and
//! malformed lines are ignored.

use std::path::PathBuf;

use eframe::egui::ThemePreference;

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

#[derive(Debug, Clone, Copy, Default)]
pub struct Prefs {
    pub appearance: Appearance,
}

impl Prefs {
    fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("qcy-earbuds").join("gui.conf"))
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
        let mut prefs = Self::default();
        let Some(text) = Self::path().and_then(|p| std::fs::read_to_string(p).ok()) else {
            return prefs;
        };
        for line in text.lines() {
            if let Some(("appearance", value)) =
                line.split_once('=').map(|(k, v)| (k.trim(), v.trim()))
            {
                if let Some(a) = Appearance::ALL.into_iter().find(|a| a.key() == value) {
                    prefs.appearance = a;
                }
            }
        }
        prefs
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                std::fs::write(&path, format!("appearance={}\n", self.appearance.key()))
            });
        if let Err(e) = result {
            tracing::warn!("cannot save preferences to {}: {e}", path.display());
        }
    }
}
