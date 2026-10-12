//! Noise cancelling / transparency.

use eframe::egui::{self, Ui, vec2};
use qcy_protocol::Command;
use qcy_protocol::types::{AncMode, AncScene};

use crate::app::ViewCtx;
use crate::material::components as m3;
use crate::material::{MaterialIcon, Scheme, icons, text};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Off,
    Anc,
    Transparency,
}

fn family(mode: Option<AncMode>) -> Option<Family> {
    Some(match mode? {
        AncMode::Off => Family::Off,
        AncMode::NoiseCancelling { .. } => Family::Anc,
        AncMode::Transparency { .. } | AncMode::VoiceTransparency => Family::Transparency,
        AncMode::Other(_) => return None,
    })
}

fn scene_info(scene: AncScene) -> (MaterialIcon, &'static str) {
    match scene {
        AncScene::Adaptive => (
            icons::ICON_AUTO_AWESOME,
            "Adjusts to your surroundings automatically",
        ),
        AncScene::Indoor => (icons::ICON_HOME, "Quiet places such as home or the office"),
        AncScene::Commuting => (
            icons::ICON_TRAIN,
            "Very noisy places such as trains and buses",
        ),
        AncScene::Noisy => (icons::ICON_GROUPS, "Busy, noisy environments"),
        AncScene::AntiWind => (icons::ICON_AIR, "Reduces wind noise outdoors"),
    }
}

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let s = Scheme::of(ui);
    let Some(caps) = view.profile.anc else {
        m3::titled_card(ui, icons::ICON_NOISE_AWARE, "Noise control", |ui| {
            m3::supporting(ui, "This model has no adjustable noise control.");
            if let Some(mode) = view.snap.state.anc {
                m3::supporting(ui, &format!("Reported mode: {}", mode.describe()));
            }
        });
        return;
    };
    let current = view.snap.state.anc;
    let current_family = family(current);

    m3::titled_card(ui, icons::ICON_NOISE_AWARE, "Noise control", |ui| {
        let options = [
            (
                Family::Off,
                icons::ICON_NOISE_CONTROL_OFF,
                "Off",
                "No processing",
            ),
            (
                Family::Anc,
                icons::ICON_NOISE_AWARE,
                "Noise cancelling",
                "Block outside sound",
            ),
            (
                Family::Transparency,
                icons::ICON_HEARING,
                "Transparency",
                "Hear surroundings",
            ),
        ];
        let gap = 8.0;
        let width = (ui.available_width() - 2.0 * gap) / 3.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (fam, icon, title, sub) in options {
                let selected = current_family == Some(fam);
                if m3::tile(ui, icon, title, sub, selected, vec2(width, 96.0)).clicked()
                    && !selected
                {
                    let mode = match fam {
                        Family::Off => AncMode::Off,
                        Family::Anc => AncMode::NoiseCancelling {
                            scene: caps.scenes.first().copied().unwrap_or(AncScene::Adaptive),
                            level: 0,
                        },
                        Family::Transparency => AncMode::Transparency {
                            level: caps.default_transparency_level,
                        },
                    };
                    view.send(Command::SetAnc(mode));
                }
            }
        });
        if let Some(AncMode::Other(raw)) = current {
            m3::notice(
                ui,
                icons::ICON_INFO,
                &format!(
                    "The earbuds report an unknown mode ({:#08x}).",
                    raw.packed()
                ),
                s.on_surface_variant,
            );
        }
    });

    match current {
        Some(AncMode::NoiseCancelling { scene, level }) => {
            m3::titled_card(ui, icons::ICON_TUNE, "Environment", |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for &candidate in caps.scenes {
                    let (icon, description) = scene_info(candidate);
                    let selected = candidate == scene;
                    if m3::choice_row(ui, icon, candidate.label(), description, selected).clicked()
                        && !selected
                    {
                        view.send(Command::SetAnc(AncMode::NoiseCancelling {
                            scene: candidate,
                            level: 0,
                        }));
                    }
                }
                if scene.max_level() > 0 {
                    ui.add_space(12.0);
                    ui.label(text::title_small("Strength").color(s.on_surface_variant));
                    let labels: Vec<String> = (0..=scene.max_level())
                        .map(|l| format!("Level {}", l + 1))
                        .collect();
                    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                    if let Some(l) = m3::segmented(ui, &labels, Some(usize::from(level))) {
                        view.send(Command::SetAnc(AncMode::NoiseCancelling {
                            scene,
                            level: l as u8,
                        }));
                    }
                }
            });
        }
        Some(AncMode::Transparency { .. } | AncMode::VoiceTransparency) => {
            m3::titled_card(ui, icons::ICON_HEARING, "Transparency", |ui| {
                let voice = current == Some(AncMode::VoiceTransparency);
                if let Some((min, max)) = caps.transparency_levels {
                    let device_level = match current {
                        Some(AncMode::Transparency { level }) => level,
                        _ => caps.default_transparency_level,
                    };
                    let mut level = view.drafts.transparency.unwrap_or(device_level);
                    ui.add_enabled_ui(!voice, |ui| {
                        ui.label(
                            text::title_small("Ambient sound level").color(s.on_surface_variant),
                        );
                        ui.horizontal(|ui| {
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 60.0).max(120.0);
                            let r =
                                ui.add(egui::Slider::new(&mut level, min..=max).show_value(false));
                            ui.label(text::title_medium(level.to_string()).color(s.on_surface));
                            if r.dragged() {
                                view.drafts.transparency = Some(level);
                            } else if r.drag_stopped() || r.changed() {
                                view.drafts.transparency = None;
                                view.send(Command::SetAnc(AncMode::Transparency { level }));
                            }
                        });
                    });
                }
                if caps.voice_transparency {
                    m3::divider(ui);
                    if let Some(on) = m3::switch_row(
                        ui,
                        icons::ICON_RECORD_VOICE_OVER,
                        "Voice focus",
                        Some("Emphasises voices while letting ambient sound through"),
                        voice,
                    ) {
                        let mode = if on {
                            AncMode::VoiceTransparency
                        } else {
                            AncMode::Transparency {
                                level: caps.default_transparency_level,
                            }
                        };
                        view.send(Command::SetAnc(mode));
                    }
                }
            });
        }
        Some(AncMode::Off) => {
            m3::card(ui, |ui| {
                m3::notice(
                    ui,
                    icons::ICON_INFO,
                    "Noise control is off: the earbuds neither cancel nor pass through outside sound.",
                    s.on_surface_variant,
                );
            });
        }
        _ => {}
    }
}
