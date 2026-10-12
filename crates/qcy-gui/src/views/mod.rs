//! The individual screens. Each `show` function renders from the snapshot in
//! [`ViewCtx`] and records user intent as actions; none of them perform I/O.

pub mod controls;
pub mod header;
pub mod noise;
pub mod searching;
pub mod settings;
pub mod sound;

use eframe::egui::{self, Ui};

use crate::app::ViewCtx;
use crate::material::{MaterialIcon, icons};

/// Navigation destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Noise,
    Sound,
    Controls,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Noise, Tab::Sound, Tab::Controls, Tab::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Noise => "Noise",
            Tab::Sound => "Sound",
            Tab::Controls => "Controls",
            Tab::Settings => "Settings",
        }
    }

    pub fn icon(self) -> MaterialIcon {
        match self {
            Tab::Noise => icons::ICON_NOISE_AWARE,
            Tab::Sound => icons::ICON_GRAPHIC_EQ,
            Tab::Controls => icons::ICON_TOUCH_APP,
            Tab::Settings => icons::ICON_SETTINGS,
        }
    }
}

pub fn show_tab(ui: &mut Ui, tab: Tab, view: &mut ViewCtx<'_>) {
    // Controls keep showing the last known values but are inert while
    // disconnected. Settings decides per section (appearance always works).
    let enabled = view.connected();
    match tab {
        Tab::Noise => ui.add_enabled_ui(enabled, |ui| noise::show(ui, view)).inner,
        Tab::Sound => ui.add_enabled_ui(enabled, |ui| sound::show(ui, view)).inner,
        Tab::Controls => {
            ui.add_enabled_ui(enabled, |ui| controls::show(ui, view))
                .inner
        }
        Tab::Settings => settings::show(ui, view),
    }
}

/// A dropdown over a list of choices; returns a newly picked value.
pub fn dropdown<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    current: Option<T>,
    choices: &[(T, String)],
    width: f32,
) -> Option<T> {
    let selected_text = current
        .and_then(|c| choices.iter().find(|(v, _)| *v == c))
        .map_or("–".to_string(), |(_, l)| l.clone());
    let mut picked = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected_text)
        .width(width)
        .height(320.0)
        .show_ui(ui, |ui| {
            for (value, label) in choices {
                if ui
                    .selectable_label(current == Some(*value), label.as_str())
                    .clicked()
                {
                    picked = Some(*value);
                }
            }
        });
    picked.filter(|p| current != Some(*p))
}
