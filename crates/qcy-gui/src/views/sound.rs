//! Equalizer and balance.

use eframe::egui::{self, Align, Layout, Ui};
use qcy_protocol::Command;
use qcy_protocol::eq::{CUSTOM_INDEX, GRAPHIC_FREQUENCIES, GRAPHIC_GAIN_RANGE, ParametricEq};

use crate::app::{Confirmation, ViewCtx};
use crate::material::components::{self as m3, ButtonKind};
use crate::material::{Scheme, icons, text};
use crate::widgets;

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    equalizer(ui, view);
    if view.profile.balance || view.snap.state.balance.is_some() {
        balance(ui, view);
    }
}

fn equalizer(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let profile = view.profile;
    m3::titled_card(ui, icons::ICON_EQUALIZER, "Equalizer", |ui| {
        if profile.eq_presets.is_empty() && !profile.custom_eq {
            m3::supporting(ui, "This model's equalizer is not supported yet.");
            return;
        }
        let state_eq = view.snap.state.parametric_eq.clone();
        let active = view.snap.state.active_eq_preset();
        let editing = view.drafts.custom_eq.is_some();

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for preset in profile.eq_presets {
                let selected = !editing && active == Some(preset.id);
                if m3::chip(ui, None, preset.name, selected).clicked() && !selected {
                    view.drafts.custom_eq = None;
                    let command = Command::SetParametricEq(preset.to_parametric());
                    match preset.warning {
                        Some(w) => view.confirm(Confirmation::new(
                            icons::ICON_SURROUND_SOUND,
                            &format!("Apply {}?", preset.name),
                            w,
                            "Apply",
                            command,
                        )),
                        None => view.send(command),
                    }
                }
            }
            if profile.custom_eq {
                let is_custom = active.is_some_and(|id| id >= 128);
                if m3::chip(ui, Some(icons::ICON_TUNE), "Custom", editing || is_custom).clicked()
                    && !editing
                {
                    view.drafts.custom_eq = Some(current_graphic(state_eq.as_ref()));
                }
            }
        });

        ui.add_space(4.0);
        let preview = match view.drafts.custom_eq {
            Some(gains) => Some(ParametricEq::from_graphic(&gains)),
            None => state_eq,
        };
        if let Some(eq) = &preview {
            widgets::eq_curve(ui, eq);
        }

        if let Some(mut gains) = view.drafts.custom_eq {
            ui.add_space(4.0);
            custom_editor(ui, &mut gains);
            view.drafts.custom_eq = Some(gains);
            if let Some(eq) = &preview {
                if eq.master_gain_db < 0.0 {
                    let s = Scheme::of(ui);
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
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if m3::button(ui, ButtonKind::Filled, Some(icons::ICON_CHECK), "Apply").clicked() {
                    view.send(Command::SetParametricEq(ParametricEq::from_graphic(&gains)));
                    view.drafts.custom_eq = None;
                }
                if m3::button(ui, ButtonKind::Outlined, None, "Flat").clicked() {
                    view.drafts.custom_eq = Some([0; 10]);
                }
                if m3::button(ui, ButtonKind::Text, None, "Cancel").clicked() {
                    view.drafts.custom_eq = None;
                }
            });
        }
    });
}

/// Ten vertical sliders, one per band.
fn custom_editor(ui: &mut Ui, gains: &mut [i8; 10]) {
    let s = Scheme::of(ui);
    let (min, max) = (*GRAPHIC_GAIN_RANGE.start(), *GRAPHIC_GAIN_RANGE.end());
    ui.columns(10, |cols| {
        for ((col, gain), freq) in cols
            .iter_mut()
            .zip(gains.iter_mut())
            .zip(GRAPHIC_FREQUENCIES)
        {
            col.vertical_centered(|ui| {
                ui.spacing_mut().slider_width = 140.0;
                ui.label(text::label_medium(format!("{gain:+}")).color(s.on_surface));
                ui.add(
                    egui::Slider::new(gain, min..=max)
                        .vertical()
                        .show_value(false),
                );
                let label = if freq >= 1000 {
                    format!("{}k", freq / 1000)
                } else {
                    freq.to_string()
                };
                ui.label(text::body_small(label).color(s.on_surface_variant));
            });
        }
    });
}

/// Starts the editor from the device's custom curve when it is one.
fn current_graphic(eq: Option<&ParametricEq>) -> [i8; 10] {
    let mut gains = [0i8; 10];
    if let Some(eq) = eq.filter(|e| e.index == CUSTOM_INDEX) {
        for band in eq.active_bands() {
            if let Some(i) = GRAPHIC_FREQUENCIES.iter().position(|&f| f == band.freq_hz) {
                gains[i] = band.gain_db.round() as i8;
            }
        }
    }
    gains
}

fn balance(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    m3::titled_card(ui, icons::ICON_BALANCE, "Balance", |ui| {
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
