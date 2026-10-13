//! Equalizer (graphic or parametric, from the panel), game EQ, balance and
//! earbud volume.

use eframe::egui::{self, Align, Layout, Ui};
use qcy_protocol::Command;
use qcy_protocol::eq::{CUSTOM_INDEX, EqBand, ParametricEq, auto_master_gain};
use qcy_protocol::panel::{EqControl, EqKind, EqPreset, ItemKind};
use qcy_protocol::state::DeviceState;

use crate::app::{Confirmation, ViewCtx};
use crate::material::components::{self as m3, ButtonKind};
use crate::material::{Scheme, icons, text};
use crate::widgets;

/// Presets whose ids reboot the earbuds when left (`rebootTip`).
const REBOOT_PRESETS: [u8; 2] = [10, 18];

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let panel = view.panel;
    match &panel.eq {
        Some(eq) => equalizer(ui, view, eq),
        None => {
            m3::titled_card(ui, icons::ICON_EQUALIZER, "Equalizer", |ui| {
                m3::supporting(ui, "This model has no adjustable equalizer.");
            });
        }
    }
    if let Some(game) = &panel.game_eq {
        game_eq(ui, view, game);
    }
    if let Some(title) = &panel.balance {
        balance(ui, view, title);
    }
    if panel
        .items
        .iter()
        .any(|i| matches!(i.kind, ItemKind::Volume))
    {
        volume(ui, view);
    }
}

/// The active preset id the device reports.
fn active_preset(state: &DeviceState, eq: &EqControl) -> Option<u8> {
    match eq.kind {
        EqKind::Graphic => state.graphic_eq.as_ref().map(|g| g.preset),
        _ => state.active_eq_preset(),
    }
}

fn is_custom(id: u8) -> bool {
    id & 0x80 != 0 || id == 0
}

/// A curve to draw for a set of gains on the panel's bands.
fn curve(eq: &EqControl, gains: &[f32]) -> ParametricEq {
    let bands: Vec<EqBand> = eq
        .bands
        .iter()
        .zip(gains)
        .filter(|(b, _)| b.freq_hz > 0)
        .map(|(b, g)| EqBand::new(b.freq_hz as u16, *g, b.q, 0))
        .collect();
    let master_gain_db = if eq.kind.is_parametric() {
        auto_master_gain(&bands, eq.volume_level, 0.0)
    } else {
        0.0
    };
    ParametricEq {
        index: CUSTOM_INDEX,
        master_gain_db,
        bands,
    }
}

/// What to draw for a preset.
fn preset_curve(eq: &EqControl, preset: &EqPreset) -> ParametricEq {
    if preset.bands.is_empty() {
        curve(eq, &preset.gains)
    } else {
        preset.clone().to_parametric_eq()
    }
}

trait ToParametric {
    fn to_parametric_eq(self) -> ParametricEq;
}

impl ToParametric for EqPreset {
    fn to_parametric_eq(self) -> ParametricEq {
        ParametricEq {
            index: self.id,
            master_gain_db: self.master_gain_db,
            bands: self.bands,
        }
    }
}

/// Gains for the custom editor: the device's custom curve if it has one,
/// otherwise the panel's default.
fn current_custom(eq: &EqControl, state: &DeviceState) -> Vec<f32> {
    let mut gains = eq.custom_gains.clone();
    match eq.kind {
        EqKind::Graphic => {
            if let Some(g) = state.graphic_eq.as_ref().filter(|g| is_custom(g.preset)) {
                for (out, v) in gains.iter_mut().zip(&g.gains) {
                    *out = f32::from(*v) - f32::from(eq.base_db);
                }
            }
        }
        _ => {
            if let Some(p) = state.parametric_eq.as_ref().filter(|p| is_custom(p.index)) {
                for band in p.active_bands() {
                    if let Some(i) = eq
                        .bands
                        .iter()
                        .position(|b| b.freq_hz == u32::from(band.freq_hz))
                    {
                        gains[i] = band.gain_db;
                    }
                }
            }
        }
    }
    gains
}

fn equalizer(ui: &mut Ui, view: &mut ViewCtx<'_>, eq: &EqControl) {
    let state = &view.snap.state;
    let active = active_preset(state, eq);
    let editing = view.drafts.custom_eq.is_some();
    m3::titled_card(ui, icons::ICON_EQUALIZER, &eq.title, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for preset in &eq.presets {
                let selected = !editing && active == Some(preset.id);
                if m3::chip(ui, None, &preset.name, selected).clicked() && !selected {
                    view.drafts.custom_eq = None;
                    let commands = eq.apply_preset(preset);
                    let leaving_reboot = active.is_some_and(|a| REBOOT_PRESETS.contains(&a));
                    let warning = preset
                        .warning
                        .clone()
                        .or_else(|| eq.reboot_tip.clone().filter(|_| leaving_reboot));
                    match warning {
                        Some(w) => view.confirm(Confirmation::many(
                            icons::ICON_SURROUND_SOUND,
                            &format!("Apply {}?", preset.name),
                            &w,
                            "Apply",
                            commands,
                        )),
                        None => view.send_all(commands),
                    }
                }
            }
            if !eq.bands.is_empty() {
                let custom = active.is_some_and(is_custom);
                if m3::chip(ui, Some(icons::ICON_TUNE), "Custom", editing || custom).clicked()
                    && !editing
                {
                    view.drafts.custom_eq = Some(current_custom(eq, state));
                }
            }
        });

        ui.add_space(4.0);
        let preview = match &view.drafts.custom_eq {
            Some(gains) => Some(curve(eq, gains)),
            None => match active.and_then(|id| eq.preset(id)) {
                Some(p) => Some(preset_curve(eq, p)),
                None => match eq.kind {
                    EqKind::Graphic => state.graphic_eq.as_ref().map(|g| {
                        curve(
                            eq,
                            &g.gains.iter().map(|&v| f32::from(v)).collect::<Vec<_>>(),
                        )
                    }),
                    _ => state.parametric_eq.clone(),
                },
            },
        };
        if let Some(curve) = &preview {
            widgets::eq_curve(ui, curve);
        }

        if let Some(mut gains) = view.drafts.custom_eq.take() {
            ui.add_space(4.0);
            custom_editor(ui, eq, &mut gains);
            let s = Scheme::of(ui);
            if let Some(eq) = &preview
                && eq.master_gain_db < 0.0
            {
                m3::notice(
                    ui,
                    icons::ICON_INFO,
                    &format!(
                        "Overall level lowered by {:.1} dB to avoid distortion.",
                        -eq.master_gain_db
                    ),
                    s.on_surface_variant,
                );
            }
            let mut keep = true;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if m3::button(ui, ButtonKind::Filled, Some(icons::ICON_CHECK), "Apply").clicked() {
                    view.send(eq.apply_custom(&gains));
                    keep = false;
                }
                if m3::button(ui, ButtonKind::Outlined, None, "Flat").clicked() {
                    gains.iter_mut().for_each(|g| *g = 0.0);
                }
                if m3::button(ui, ButtonKind::Text, None, "Cancel").clicked() {
                    keep = false;
                }
            });
            if keep {
                view.drafts.custom_eq = Some(gains);
            }
        }
    });
}

/// One vertical slider per band of the panel.
fn custom_editor(ui: &mut Ui, eq: &EqControl, gains: &mut [f32]) {
    let s = Scheme::of(ui);
    let max = f32::from(eq.max_db);
    let n = gains.len().min(eq.bands.len()).max(1);
    ui.columns(n, |cols| {
        for ((col, gain), band) in cols.iter_mut().zip(gains.iter_mut()).zip(&eq.bands) {
            col.vertical_centered(|ui| {
                ui.spacing_mut().slider_width = 140.0;
                ui.label(text::label_medium(format!("{:+}", gain.round())).color(s.on_surface));
                ui.add(
                    egui::Slider::new(gain, -max..=max)
                        .step_by(1.0)
                        .vertical()
                        .show_value(false),
                );
                ui.label(text::body_small(&band.label).color(s.on_surface_variant));
            });
        }
    });
}

fn game_eq(ui: &mut Ui, view: &mut ViewCtx<'_>, eq: &EqControl) {
    let active = view.snap.state.graphic_eq.as_ref().map(|g| g.preset);
    m3::titled_card(ui, icons::ICON_SPORTS_ESPORTS, &eq.title, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for preset in &eq.presets {
                let selected = active == Some(preset.id);
                if m3::chip(ui, None, &preset.name, selected).clicked() && !selected {
                    view.send_all(eq.apply_preset(preset));
                }
            }
        });
    });
}

fn balance(ui: &mut Ui, view: &mut ViewCtx<'_>, title: &str) {
    m3::titled_card(ui, icons::ICON_BALANCE, title, |ui| {
        let s = Scheme::of(ui);
        let device = view.snap.state.balance.unwrap_or(50);
        let mut value = view.drafts.balance.unwrap_or(device);
        ui.horizontal(|ui| {
            ui.label(text::label_large("L").color(s.on_surface_variant));
            ui.spacing_mut().slider_width = (ui.available_width() - 130.0).max(120.0);
            let r = ui.add(egui::Slider::new(&mut value, 0..=100).show_value(false));
            ui.label(text::label_large("R").color(s.on_surface_variant));
            if r.dragged() {
                view.drafts.balance = Some(value);
            } else if r.drag_stopped() || r.changed() {
                view.drafts.balance = None;
                view.send(Command::SetBalance(value));
            }
            ui.add_enabled_ui(value != 50, |ui| {
                if m3::button(ui, ButtonKind::Text, None, "Centre").clicked() {
                    view.send(Command::SetBalance(50));
                }
            });
        });
        let offset = i16::from(value) - 50;
        let description = match offset {
            0 => "Centred".to_string(),
            o if o < 0 => format!("{} % towards the left", -o * 2),
            o => format!("{} % towards the right", o * 2),
        };
        m3::supporting(ui, &description);
    });
}

fn volume(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    m3::titled_card(ui, icons::ICON_VOLUME_UP, "Earbud volume", |ui| {
        let s = Scheme::of(ui);
        let Some(v) = view.snap.state.volume else {
            m3::supporting(ui, "Waiting for the earbuds to report their volume…");
            return;
        };
        let max = v.max.max(1);
        let mut value = view.drafts.volume.unwrap_or(v.left.max(v.right).min(max));
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 60.0).max(120.0);
            let r = ui.add(egui::Slider::new(&mut value, 0..=max).show_value(false));
            ui.label(text::title_medium(value.to_string()).color(s.on_surface));
            if r.dragged() {
                view.drafts.volume = Some(value);
            } else if r.drag_stopped() || r.changed() {
                view.drafts.volume = None;
                view.send(Command::SetVolume {
                    left: value,
                    right: value,
                });
            }
        });
    });
}
