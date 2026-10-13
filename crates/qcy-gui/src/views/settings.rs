//! Features, wear detection, prompts, power, appearance and device management.

use eframe::egui::{self, Align, Layout, Ui};
use qcy_protocol::Command;
use qcy_protocol::command::MAX_NAME_LEN;
use qcy_protocol::types::{Toggle, Version, WearDetection, WearMusicBehavior};

use crate::app::{Action, Confirmation, ViewCtx};
use crate::material::components::{self as m3, ButtonKind};
use crate::material::scheme::{self, ACCENT_PRESETS};
use crate::material::{MaterialIcon, Scheme, icons, text};
use crate::prefs::Appearance;
use crate::views::dropdown;

/// Sleep timer choices in minutes (0 = off).
const SLEEP_TIMER_CHOICES: [u16; 6] = [0, 15, 30, 60, 90, 120];
/// Width reserved for dropdowns at the end of a row.
const TRAILING: f32 = 200.0;

fn toggle_info(t: Toggle) -> (MaterialIcon, &'static str) {
    match t {
        Toggle::GameMode => (
            icons::ICON_SPORTS_ESPORTS,
            "Lower audio latency for games and video",
        ),
        Toggle::SleepMode => (
            icons::ICON_BEDTIME,
            "Disables touch controls until the earbuds go back in the case",
        ),
        Toggle::Ldac => (
            icons::ICON_HIGH_QUALITY,
            "High-resolution audio. Restarts the earbuds and turns off dual device",
        ),
        Toggle::DualDevice => (
            icons::ICON_DEVICES,
            "Stay connected to two devices. Restarts the earbuds and turns off LDAC",
        ),
        Toggle::InEarDetection => (icons::ICON_SENSORS, "Detect when the earbuds are worn"),
    }
}

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let connected = view.connected();
    ui.add_enabled_ui(connected, |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        features(ui, view);
        if view.profile.wear_detection || view.snap.state.wear_detection.is_some() {
            wear_detection(ui, view);
        }
        prompts(ui, view);
        power(ui, view);
    });
    appearance(ui, view);
    ui.add_enabled_ui(connected, |ui| device(ui, view));
}

fn features(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    // Toggles of the profile, plus any the device reports on its own.
    let toggles: Vec<Toggle> = Toggle::ALL
        .into_iter()
        .filter(|t| view.profile.toggles.contains(t) || view.snap.state.toggle(*t).is_some())
        .collect();
    if toggles.is_empty() {
        return;
    }
    m3::titled_card(ui, icons::ICON_TUNE, "Features", |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for t in toggles {
            let (icon, description) = toggle_info(t);
            let on = view.snap.state.toggle(t).unwrap_or(false);
            if let Some(new) = m3::switch_row(ui, icon, t.label(), Some(description), on) {
                let command = Command::SetToggle(t, new);
                match view.profile.toggle_warning(t) {
                    // Settings that reboot the earbuds need consent.
                    Some(w) if new && matches!(t, Toggle::Ldac | Toggle::DualDevice) => {
                        view.confirm(Confirmation::new(
                            icon,
                            &format!("Turn on {}?", t.label()),
                            w,
                            "Turn on",
                            command,
                        ));
                    }
                    _ => view.send(command),
                }
            }
        }
    });
}

fn wear_detection(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    m3::titled_card(ui, icons::ICON_SENSORS, "Wear detection", |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let current = view.snap.state.wear_detection.unwrap_or(WearDetection {
            enabled: false,
            music: WearMusicBehavior::PauseAndResume,
            anc_follows_wear: false,
            tone: None,
        });
        let mut new = current;
        if let Some(on) = m3::switch_row(
            ui,
            icons::ICON_SENSORS,
            "Detect when worn",
            Some("React when the earbuds are put on or taken off"),
            current.enabled,
        ) {
            new.enabled = on;
        }
        ui.add_enabled_ui(current.enabled, |ui| {
            m3::setting_row(
                ui,
                icons::ICON_PLAY_CIRCLE,
                "Playback",
                None,
                TRAILING,
                |ui| {
                    let choices: Vec<_> = WearMusicBehavior::KNOWN
                        .iter()
                        .map(|b| (*b, b.label()))
                        .collect();
                    if let Some(m) = dropdown(
                        ui,
                        "wear-music",
                        Some(current.music),
                        &choices,
                        TRAILING - 10.0,
                    ) {
                        new.music = m;
                    }
                },
            );
            if let Some(on) = m3::switch_row(
                ui,
                icons::ICON_NOISE_AWARE,
                "Noise cancelling follows wear",
                Some("Off when removed, back on when worn"),
                current.anc_follows_wear,
            ) {
                new.anc_follows_wear = on;
            }
        });
        let s = Scheme::of(ui);
        m3::notice(
            ui,
            icons::ICON_INFO,
            "Changes apply after taking the earbuds out and putting them back in.",
            s.on_surface_variant,
        );
        if new != current {
            view.send(Command::SetWearDetection(new));
        }
    });
}

fn prompts(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let p = view.profile;
    if p.prompt_languages.is_empty() && p.prompt_volumes.is_empty() {
        return;
    }
    let state = &view.snap.state;
    m3::titled_card(ui, icons::ICON_RECORD_VOICE_OVER, "Voice prompts", |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        if !p.prompt_languages.is_empty() {
            let mut picked = None;
            m3::setting_row(ui, icons::ICON_LANGUAGE, "Language", None, TRAILING, |ui| {
                let choices: Vec<_> = p
                    .prompt_languages
                    .iter()
                    .map(|c| (c.value, c.label.to_string()))
                    .collect();
                let current = state
                    .prompt_language
                    .as_deref()
                    .and_then(|l| p.prompt_languages.iter().find(|c| c.value == l))
                    .map(|c| c.value);
                picked = dropdown(ui, "prompt-lang", current, &choices, TRAILING - 10.0);
            });
            if let Some(code) = picked {
                view.send(Command::SetPromptLanguage(code.to_string()));
            }
        }
        if !p.prompt_volumes.is_empty() {
            let mut picked = None;
            m3::setting_row(ui, icons::ICON_VOLUME_UP, "Volume", None, TRAILING, |ui| {
                let choices: Vec<_> = p
                    .prompt_volumes
                    .iter()
                    .map(|c| (c.value, c.label.to_string()))
                    .collect();
                let current = state.prompt_volume.map(|v| v.level);
                picked = dropdown(ui, "prompt-vol", current, &choices, TRAILING - 10.0);
            });
            if let Some(level) = picked {
                view.send(Command::SetPromptVolume(level));
            }
        }
    });
}

fn power(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let p = view.profile;
    if p.auto_power_off.is_empty() && !p.sleep_timer {
        return;
    }
    let state = &view.snap.state;
    m3::titled_card(ui, icons::ICON_POWER_SETTINGS_NEW, "Power", |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        if !p.auto_power_off.is_empty() {
            let mut picked = None;
            m3::setting_row(
                ui,
                icons::ICON_BLUETOOTH_DISABLED,
                "Auto power off",
                Some("After being disconnected for"),
                TRAILING,
                |ui| {
                    let choices: Vec<_> =
                        p.auto_power_off.iter().map(|a| (*a, a.label())).collect();
                    picked = dropdown(
                        ui,
                        "auto-off",
                        state.auto_power_off,
                        &choices,
                        TRAILING - 10.0,
                    );
                },
            );
            if let Some(a) = picked {
                view.send(Command::SetAutoPowerOff(a));
            }
        }
        if p.sleep_timer {
            let mut picked = None;
            m3::setting_row(
                ui,
                icons::ICON_TIMER,
                "Sleep timer",
                Some("Power off after a set time, even while playing"),
                TRAILING,
                |ui| {
                    let current = state
                        .power_off_timer
                        .map(|t| if t.minutes == u16::MAX { 0 } else { t.minutes });
                    let mut choices: Vec<_> = SLEEP_TIMER_CHOICES
                        .iter()
                        .map(|&m| {
                            (
                                m,
                                if m == 0 {
                                    "Off".to_string()
                                } else {
                                    format!("{m} min")
                                },
                            )
                        })
                        .collect();
                    if let Some(c) = current.filter(|c| !SLEEP_TIMER_CHOICES.contains(c)) {
                        choices.push((c, format!("{c} min")));
                    }
                    picked = dropdown(ui, "sleep-timer", current, &choices, TRAILING - 10.0);
                },
            );
            if let Some(m) = picked {
                view.send(Command::SetPowerOffTimer(m));
            }
        }
    });
}

/// Purely local, so it stays usable while disconnected.
fn appearance(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    m3::titled_card(ui, icons::ICON_PALETTE, "Appearance", |ui| {
        let s = Scheme::of(ui);
        ui.label(text::title_small("Theme").color(s.on_surface_variant));
        let labels: Vec<&str> = Appearance::ALL.iter().map(|a| a.label()).collect();
        let selected = Appearance::ALL.iter().position(|a| *a == view.appearance);
        if let Some(i) = m3::segmented(ui, &labels, selected) {
            view.act(Action::SetAppearance(Appearance::ALL[i]));
        }

        ui.add_space(8.0);
        ui.label(text::title_small("Accent colour").color(s.on_surface_variant));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            let mut is_preset = false;
            for (name, seed) in ACCENT_PRESETS {
                let selected = view.accent == seed;
                is_preset |= selected;
                if m3::swatch(ui, seed, selected).on_hover_text(name).clicked() && !selected {
                    view.act(Action::SetAccent(seed));
                }
            }
            // Any other colour through the picker.
            let mut custom = view.accent;
            let picker = ui
                .color_edit_button_srgba(&mut custom)
                .on_hover_text("Custom colour");
            if picker.changed() && custom != view.accent {
                view.act(Action::SetAccent(custom.to_opaque()));
            }
            if !is_preset {
                ui.label(text::body_small(scheme::to_hex(view.accent)).color(s.on_surface_variant));
            }
        });
        m3::supporting(
            ui,
            "The whole colour scheme is generated from the accent, for light and dark mode.",
        );
    });
}

fn device(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let snap = view.snap;
    m3::titled_card(ui, icons::ICON_EARBUDS, "Device", |ui| {
        let s = Scheme::of(ui);
        if view.profile.rename {
            let current = snap.state.device_name.clone().unwrap_or_default();
            let mut name = view.drafts.name.clone().unwrap_or_else(|| current.clone());
            ui.label(text::body_small("Bluetooth name").color(s.on_surface_variant));
            ui.horizontal(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .desired_width((ui.available_width() - 130.0).max(120.0))
                        .char_limit(MAX_NAME_LEN)
                        .margin(egui::vec2(12.0, 10.0))
                        .font(crate::material::regular(15.0)),
                );
                if edit.changed() {
                    view.drafts.name = Some(name.clone());
                }
                let trimmed = name.trim();
                let valid =
                    !trimmed.is_empty() && trimmed.len() <= MAX_NAME_LEN && trimmed != current;
                ui.add_enabled_ui(valid, |ui| {
                    if m3::button(ui, ButtonKind::Tonal, Some(icons::ICON_EDIT), "Rename").clicked()
                    {
                        view.drafts.name = None;
                        view.send(Command::SetDeviceName(trimmed.to_string()));
                    }
                });
            });
            m3::notice(
                ui,
                icons::ICON_INFO,
                "Keep both earbuds out of the case. Other devices show the new name after reconnecting.",
                s.on_surface_variant,
            );
            m3::divider(ui);
        }

        let version = |v: Option<Version>| v.map_or("–".into(), |v| v.to_string());
        let mut rows: Vec<(MaterialIcon, &str, String)> = Vec::new();
        if let Some(d) = &snap.device {
            rows.push((
                icons::ICON_EARBUDS,
                "Model",
                format!("{} ({:#06x})", d.profile.name, d.vendor_id),
            ));
            rows.push((
                icons::ICON_BLUETOOTH,
                "Audio address",
                d.classic_address.to_string(),
            ));
            rows.push((
                icons::ICON_SETTINGS_REMOTE,
                "Control address",
                d.ble_address.to_string(),
            ));
        }
        if let Some(fw) = snap.state.firmware {
            rows.push((
                icons::ICON_MEMORY,
                "Firmware",
                format!("L {}  ·  R {}", version(fw.left), version(fw.right)),
            ));
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        for (icon, key, value) in rows {
            m3::setting_row(ui, icon, key, None, TRAILING, |ui| {
                ui.label(text::body_medium(value).color(s.on_surface_variant));
            });
        }
        ui.spacing_mut().item_spacing.y = 8.0;
        m3::notice(
            ui,
            icons::ICON_SHIELD,
            "Firmware updates are intentionally not supported by this app.",
            s.on_surface_variant,
        );
        m3::divider(ui);
        ui.horizontal_wrapped(|ui| {
            if m3::button(ui, ButtonKind::Outlined, Some(icons::ICON_REFRESH), "Reload").clicked() {
                view.act(Action::Refresh);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if m3::button(ui, ButtonKind::Danger, Some(icons::ICON_DEVICE_RESET), "Factory reset").clicked() {
                    view.confirm(
                        Confirmation::new(
                            icons::ICON_DEVICE_RESET,
                            "Factory reset?",
                            "This erases all settings and every pairing. You will need to pair the earbuds again.",
                            "Reset",
                            Command::FactoryReset,
                        )
                        .destructive(),
                    );
                }
                if m3::button(ui, ButtonKind::Outlined, Some(icons::ICON_RESTART_ALT), "Restore defaults").clicked() {
                    view.confirm(Confirmation::new(
                        icons::ICON_RESTART_ALT,
                        "Restore default settings?",
                        "All earbud settings return to their factory values. Pairings are kept.",
                        "Restore",
                        Command::ResetDefaults,
                    ));
                }
            });
        });
    });
}
