//! Noise control, built from the model's panel: its modes, scenes, levels
//! and voice focus, exactly as the panel defines them.

use eframe::egui::{self, Ui, vec2};
use qcy_protocol::panel::{
    AncControl, LegacyAnc, ModeKind, PercentAnc, SceneAnc, SceneMode, anc_command,
};
use qcy_protocol::{Command, CommandId, dialect};

use crate::app::ViewCtx;
use crate::material::components as m3;
use crate::material::{MaterialIcon, Scheme, icons, text};

/// An icon for a mode or scene, guessed from its (English) name.
fn icon_for(name: &str) -> MaterialIcon {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if has(&["transparen", "ambient", "aware", "monitor", "hear"]) {
        icons::ICON_HEARING
    } else if has(&["normal", "off", "close"]) {
        icons::ICON_NOISE_CONTROL_OFF
    } else if has(&["adaptive", "smart", "ai ", "auto"]) {
        icons::ICON_AUTO_AWESOME
    } else if has(&["indoor", "home", "office", "quiet"]) {
        icons::ICON_HOME
    } else if has(&[
        "commut",
        "traffic",
        "train",
        "transport",
        "plane",
        "outdoor",
    ]) {
        icons::ICON_TRAIN
    } else if has(&["noisy", "crowd", "deep", "strong"]) {
        icons::ICON_GROUPS
    } else if has(&["wind"]) {
        icons::ICON_AIR
    } else if has(&["voice", "vocal", "speech"]) {
        icons::ICON_RECORD_VOICE_OVER
    } else {
        icons::ICON_NOISE_AWARE
    }
}

/// A row of selectable tiles, one per mode. Returns the clicked index.
fn mode_tiles(ui: &mut Ui, names: &[&str], selected: Option<usize>) -> Option<usize> {
    let gap = 8.0;
    let per_row = names.len().clamp(1, 3);
    let width = (ui.available_width() - gap * (per_row as f32 - 1.0)) / per_row as f32;
    let mut clicked = None;
    for (row, chunk) in names.chunks(per_row).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (i, name) in chunk.iter().enumerate() {
                let index = row * per_row + i;
                let sel = selected == Some(index);
                if m3::tile(ui, icon_for(name), name, "", sel, vec2(width, 88.0)).clicked() && !sel
                {
                    clicked = Some(index);
                }
            }
        });
    }
    clicked
}

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let panel = view.panel;
    match &panel.anc {
        Some(AncControl::Scenes(anc)) => scenes(ui, view, anc),
        Some(AncControl::Legacy(anc)) => legacy(ui, view, anc),
        Some(AncControl::Percent(anc)) => percent(ui, view, anc),
        None => {
            m3::titled_card(ui, icons::ICON_NOISE_AWARE, "Noise control", |ui| {
                m3::supporting(ui, "This model has no adjustable noise control.");
            });
        }
    }
}

fn scenes(ui: &mut Ui, view: &mut ViewCtx<'_>, anc: &SceneAnc) {
    let s = Scheme::of(ui);
    let current = dialect::current_anc(&view.snap.state);
    let active = current.and_then(|v| anc.modes.iter().position(|m| m.contains(v)));
    if let (Some(i), Some(v)) = (active, current) {
        view.drafts.anc_last.insert(i, v);
    }

    m3::titled_card(ui, icons::ICON_NOISE_AWARE, &anc.title, |ui| {
        let names: Vec<&str> = anc.modes.iter().map(|m| m.name.as_str()).collect();
        if let Some(i) = mode_tiles(ui, &names, active) {
            let mode = &anc.modes[i];
            let value = view
                .drafts
                .anc_last
                .get(&i)
                .copied()
                .unwrap_or(mode.default);
            view.send(anc_command(value));
        }
        if let (None, Some(v)) = (active, current) {
            m3::notice(
                ui,
                icons::ICON_INFO,
                &format!("The earbuds report a mode this panel does not list ({v:#08x})."),
                s.on_surface_variant,
            );
        }
    });

    let (Some(i), Some(v)) = (active, current) else {
        return;
    };
    let mode = &anc.modes[i];
    match &mode.kind {
        ModeKind::Scenes(subs) => {
            m3::titled_card(ui, icons::ICON_TUNE, "Environment", |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for sub in subs {
                    let selected = sub.contains(v);
                    let description = sub.description.as_deref().unwrap_or("");
                    if m3::choice_row(ui, icon_for(&sub.name), &sub.name, description, selected)
                        .clicked()
                        && !selected
                    {
                        view.send(anc_command(sub.default.clamp(sub.start, sub.end)));
                    }
                    if selected && sub.max_level() > 0 {
                        ui.add_space(4.0);
                        ui.label(text::title_small("Strength").color(s.on_surface_variant));
                        let labels: Vec<String> = (0..=sub.max_level())
                            .map(|l| format!("Level {}", l + 1))
                            .collect();
                        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                        let level = (v - sub.start) as usize;
                        if let Some(l) = m3::segmented(ui, &labels, Some(level)) {
                            view.send(anc_command(sub.start + l as u32));
                        }
                        ui.add_space(4.0);
                    }
                }
            });
        }
        ModeKind::Level { voice } => {
            m3::titled_card(ui, icon_for(&mode.name), &mode.name, |ui| {
                let voice_on = *voice == Some(v);
                level_slider(ui, view, mode, v, !voice_on, None);
                if let Some(voice) = voice {
                    m3::divider(ui);
                    if let Some(on) = m3::switch_row(
                        ui,
                        icons::ICON_RECORD_VOICE_OVER,
                        "Voice focus",
                        Some("Emphasises voices while letting ambient sound through"),
                        voice_on,
                    ) {
                        let level = *mode.levels().end().min(&mode.default.max(mode.start + 1));
                        view.send(anc_command(if on { *voice } else { level }));
                    }
                }
            });
        }
        ModeKind::Ruler => {
            m3::titled_card(ui, icon_for(&mode.name), &mode.name, |ui| {
                level_slider(ui, view, mode, v, true, Some(&anc.level_names));
            });
        }
        ModeKind::Button => {}
    }
    if !mode.env_adaptation.is_empty() {
        let on = view
            .snap
            .state
            .setting_on(CommandId::EnvAdaptation)
            .unwrap_or(false);
        m3::card(ui, |ui| {
            if let Some(new) = m3::switch_row(
                ui,
                icons::ICON_AUTO_AWESOME,
                "Environment adaptation",
                Some(&mode.env_adaptation.join(" · ")),
                on,
            ) {
                view.send(Command::SetValue {
                    id: CommandId::EnvAdaptation,
                    payload: vec![u8::from(new)],
                });
            }
        });
    }
}

/// A slider over the packed values of a level or ruler mode.
fn level_slider(
    ui: &mut Ui,
    view: &mut ViewCtx<'_>,
    mode: &SceneMode,
    current: u32,
    enabled: bool,
    labels: Option<&[String]>,
) {
    let s = Scheme::of(ui);
    let range = mode.levels();
    if range.is_empty() {
        return;
    }
    let device = current.clamp(*range.start(), *range.end());
    let mut value = view.drafts.anc_level.unwrap_or(device);
    let label = |v: u32| {
        let n = (v & 0xFF) as usize;
        labels
            .and_then(|l| l.get(n))
            .cloned()
            .unwrap_or_else(|| (v - range.start() + 1).to_string())
    };
    ui.add_enabled_ui(enabled, |ui| {
        ui.label(text::title_small("Level").color(s.on_surface_variant));
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 80.0).max(120.0);
            let r = ui.add(egui::Slider::new(&mut value, range.clone()).show_value(false));
            ui.label(text::title_medium(label(value)).color(s.on_surface));
            if r.dragged() {
                view.drafts.anc_level = Some(value);
            } else if r.drag_stopped() || r.changed() {
                view.drafts.anc_level = None;
                view.send(anc_command(value));
            }
        });
    });
}

fn legacy(ui: &mut Ui, view: &mut ViewCtx<'_>, anc: &LegacyAnc) {
    let current = view
        .snap
        .state
        .setting(CommandId::NoiseMode)
        .and_then(|p| p.first().copied());
    m3::titled_card(ui, icons::ICON_NOISE_AWARE, &anc.title, |ui| {
        let names: Vec<&str> = anc.modes.iter().map(|(n, _)| n.as_str()).collect();
        let active = anc.modes.iter().position(|(_, m)| Some(*m) == current);
        if let Some(i) = mode_tiles(ui, &names, active) {
            view.send(Command::SetNoiseMode(anc.modes[i].1));
        }
    });
}

fn percent(ui: &mut Ui, view: &mut ViewCtx<'_>, anc: &PercentAnc) {
    let s = Scheme::of(ui);
    let current = view.snap.state.anc.map(|m| m.to_setting());
    let active = current.and_then(|c| anc.modes.iter().position(|m| m.mode == c.mode));
    m3::titled_card(ui, icons::ICON_NOISE_AWARE, &anc.title, |ui| {
        let names: Vec<&str> = anc.modes.iter().map(|m| m.name.as_str()).collect();
        if let Some(i) = mode_tiles(ui, &names, active) {
            let m = &anc.modes[i];
            view.send(anc_command(m.packed(m.max_percent)));
        }
        let (Some(i), Some(c)) = (active, current) else {
            return;
        };
        let m = &anc.modes[i];
        if m.max_percent == 0 {
            return;
        }
        ui.add_space(8.0);
        ui.label(text::title_small("Strength").color(s.on_surface_variant));
        let device = u32::from(c.level.min(m.max_percent));
        let mut value = view.drafts.anc_level.unwrap_or(device);
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(120.0);
            let r = ui
                .add(egui::Slider::new(&mut value, 0..=u32::from(m.max_percent)).show_value(false));
            ui.label(text::title_medium(format!("{value} %")).color(s.on_surface));
            if r.dragged() {
                view.drafts.anc_level = Some(value);
            } else if r.drag_stopped() || r.changed() {
                view.drafts.anc_level = None;
                view.send(anc_command(m.packed(value as u8)));
            }
        });
    });
}
