//! The model's settings rows (switches, value lists, prompts, power, wear
//! detection, fit test, …), appearance and device management.
//!
//! Every row comes from the model's control panel, so each model shows
//! exactly what it supports, with the panel's own names.

use eframe::egui::{self, Ui};
use qcy_protocol::command::MAX_NAME_LEN;
use qcy_protocol::panel::{
    Action as PanelAction, ActionKind, Choice, Item, ItemKind, Switch, WearDetection as WearSpec,
};
use qcy_protocol::types::{PairedPhone, Version, WearDetection, WearMusicBehavior};
use qcy_protocol::{Characteristic, Command, CommandId, PanelWrite, Write};

use crate::app::{Action, Confirmation, ViewCtx};
use crate::material::components::{self as m3, ButtonKind};
use crate::material::scheme::{self, ACCENT_PRESETS};
use crate::material::{MaterialIcon, Scheme, icons, text};
use crate::prefs::Appearance;
use crate::views::dropdown;

/// Sleep timer choices in minutes (0 = off).
const SLEEP_TIMER_CHOICES: [u16; 5] = [0, 15, 30, 60, 90];
/// Width reserved for dropdowns at the end of a row.
const TRAILING: f32 = 200.0;

/// Which card a row belongs in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Features,
    Wear,
    Prompts,
    Power,
    FitTest,
    /// Rendered elsewhere (device card, sound tab).
    Elsewhere,
}

fn section(item: &Item) -> Section {
    match &item.kind {
        ItemKind::Switch(_)
        | ItemKind::GameConfig { .. }
        | ItemKind::Multipoint
        | ItemKind::AiAnc
        | ItemKind::FocusMode => Section::Features,
        ItemKind::Choice(c) if c.setting == CommandId::AutoPowerOff => Section::Power,
        ItemKind::Choice(_) => Section::Features,
        ItemKind::WearDetection(_) => Section::Wear,
        ItemKind::Language { .. }
        | ItemKind::LegacyLanguage { .. }
        | ItemKind::PromptVolume { .. } => Section::Prompts,
        ItemKind::SleepTimer => Section::Power,
        ItemKind::FitTest => Section::FitTest,
        ItemKind::Rename { .. }
        | ItemKind::Action(_)
        | ItemKind::Volume
        | ItemKind::HeadGestures(_) => Section::Elsewhere,
    }
}

/// An icon for a switch or list, guessed from its setting or name.
fn item_icon(item: &Item) -> MaterialIcon {
    let setting = match &item.kind {
        ItemKind::Switch(s) => s.setting,
        ItemKind::Choice(c) => Some(c.setting),
        _ => None,
    };
    match setting {
        Some(CommandId::GameMode) => return icons::ICON_SPORTS_ESPORTS,
        Some(CommandId::SleepMode) => return icons::ICON_BEDTIME,
        Some(CommandId::Ldac | CommandId::Aptx) => return icons::ICON_HIGH_QUALITY,
        Some(CommandId::DualDevice) => return icons::ICON_DEVICES,
        Some(CommandId::InEar | CommandId::Monitor) => return icons::ICON_SENSORS,
        Some(CommandId::AntiWind) => return icons::ICON_AIR,
        Some(CommandId::SpatialAudio) => return icons::ICON_SURROUND_SOUND,
        Some(CommandId::AdaptiveEq | CommandId::BassBoost) => return icons::ICON_GRAPHIC_EQ,
        Some(CommandId::AdaptiveVolume) => return icons::ICON_VOLUME_UP,
        Some(CommandId::EcoMode) => return icons::ICON_ECO,
        Some(CommandId::LedMode) => return icons::ICON_LIGHTBULB,
        Some(CommandId::AutoPowerOff) => return icons::ICON_BLUETOOTH_DISABLED,
        Some(CommandId::MusicMode) => return icons::ICON_MUSIC_NOTE,
        _ => {}
    }
    match &item.kind {
        ItemKind::GameConfig { .. } => icons::ICON_SPORTS_ESPORTS,
        ItemKind::Multipoint => icons::ICON_DEVICES,
        ItemKind::AiAnc => icons::ICON_AUTO_AWESOME,
        ItemKind::FocusMode => icons::ICON_SD_CARD,
        _ => icons::ICON_TUNE,
    }
}

/// Settings that restart the earbuds; turning them on needs consent.
fn restarts(setting: Option<CommandId>) -> bool {
    matches!(
        setting,
        Some(CommandId::Ldac | CommandId::DualDevice | CommandId::Aptx | CommandId::GameConfig)
    )
}

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let connected = view.connected();
    let panel = view.panel;
    ui.add_enabled_ui(connected, |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        let cards = [
            (Section::Features, icons::ICON_TUNE, "Features"),
            (Section::Wear, icons::ICON_SENSORS, "Wear detection"),
            (
                Section::Prompts,
                icons::ICON_RECORD_VOICE_OVER,
                "Voice prompts",
            ),
            (Section::Power, icons::ICON_POWER_SETTINGS_NEW, "Power"),
            (Section::FitTest, icons::ICON_HEARING, "Ear tip fit"),
        ];
        for (sec, icon, title) in cards {
            let items: Vec<&Item> = panel.items.iter().filter(|i| section(i) == sec).collect();
            if items.is_empty() {
                continue;
            }
            m3::titled_card(ui, icon, title, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for item in items {
                    show_item(ui, view, item);
                }
            });
        }
    });
    appearance(ui, view);
    ui.add_enabled_ui(connected, |ui| device(ui, view));
}

fn show_item(ui: &mut Ui, view: &mut ViewCtx<'_>, item: &Item) {
    let icon = item_icon(item);
    let description = item.description.as_deref();
    match &item.kind {
        ItemKind::Switch(sw) => switch(ui, view, item, sw),
        ItemKind::Choice(c) => choice(ui, view, item, c),
        ItemKind::SleepTimer => sleep_timer(ui, view, item),
        ItemKind::PromptVolume { levels } => {
            let current = view.snap.state.prompt_volume.map(|v| v.level);
            let mut picked = None;
            m3::setting_row(
                ui,
                icons::ICON_VOLUME_UP,
                &item.title,
                None,
                TRAILING,
                |ui| {
                    picked = dropdown(ui, "prompt-vol", current, levels, TRAILING - 10.0);
                },
            );
            if let Some(level) = picked {
                view.send(Command::SetPromptVolume(level));
            }
        }
        ItemKind::Language { languages } => {
            let reported = view.snap.state.prompt_language.clone().unwrap_or_default();
            let current = languages
                .iter()
                .position(|(code, _)| code.eq_ignore_ascii_case(reported.trim()));
            let choices: Vec<(usize, String)> = languages
                .iter()
                .enumerate()
                .map(|(i, (_, l))| (i, l.clone()))
                .collect();
            let mut picked = None;
            m3::setting_row(
                ui,
                icons::ICON_LANGUAGE,
                &item.title,
                None,
                TRAILING,
                |ui| {
                    picked = dropdown(ui, "prompt-lang", current, &choices, TRAILING - 10.0);
                },
            );
            if let Some(i) = picked {
                view.send(Command::SetPromptLanguage(languages[i].0.clone()));
            }
        }
        ItemKind::LegacyLanguage { languages } => {
            let mut picked = None;
            m3::setting_row(
                ui,
                icons::ICON_LANGUAGE,
                &item.title,
                None,
                TRAILING,
                |ui| {
                    picked = dropdown(ui, "legacy-lang", None, languages, TRAILING - 10.0);
                },
            );
            if let Some(code) = picked {
                view.send(Command::Raw(Write {
                    characteristic: Characteristic::LANGUAGE,
                    data: vec![code],
                }));
            }
        }
        ItemKind::WearDetection(spec) => wear_detection(ui, view, spec, description),
        ItemKind::FitTest => fit_test(ui, view),
        ItemKind::GameConfig { sync_label } => game_config(ui, view, item, sync_label.as_deref()),
        ItemKind::Multipoint => multipoint(ui, view, item),
        ItemKind::AiAnc => {
            // `[status, …]`: the app writes 0 to enable and 4 to disable.
            let on = view
                .snap
                .state
                .setting(CommandId::AiAnc)
                .and_then(|p| p.first())
                .map(|&s| s != 4);
            if let Some(new) =
                m3::switch_row(ui, icon, &item.title, description, on.unwrap_or(false))
            {
                view.send(Command::SetValue {
                    id: CommandId::AiAnc,
                    payload: vec![if new { 0 } else { 4 }, 0, 0, 0, 0, 1],
                });
            }
        }
        ItemKind::FocusMode => {
            let focus = view
                .snap
                .state
                .setting_on(CommandId::FocusMode)
                .unwrap_or(false);
            m3::setting_row(ui, icon, &item.title, description, TRAILING, |ui| {
                if let Some(i) =
                    m3::segmented(ui, &["Bluetooth", "Focus"], Some(usize::from(focus)))
                {
                    view.send(Command::SetValue {
                        id: CommandId::FocusMode,
                        payload: vec![i as u8],
                    });
                }
            });
        }
        ItemKind::Rename { .. }
        | ItemKind::Action(_)
        | ItemKind::Volume
        | ItemKind::HeadGestures(_) => {}
    }
}

fn switch(ui: &mut Ui, view: &mut ViewCtx<'_>, item: &Item, sw: &Switch) {
    let on = sw.is_on(&view.snap.state).unwrap_or(false);
    if let Some(new) = m3::switch_row(
        ui,
        item_icon(item),
        &item.title,
        item.description.as_deref(),
        on,
    ) {
        let command = sw.command(new);
        let warning = sw.warning.clone().or_else(|| {
            (new && restarts(sw.setting))
                .then(|| "The earbuds restart to apply this setting.".to_string())
        });
        match warning {
            Some(w) => view.confirm(Confirmation::new(
                item_icon(item),
                &format!(
                    "{} {}?",
                    if new { "Turn on" } else { "Turn off" },
                    item.title
                ),
                &w,
                if new { "Turn on" } else { "Turn off" },
                command,
            )),
            None => view.send(command),
        }
    }
}

fn choice(ui: &mut Ui, view: &mut ViewCtx<'_>, item: &Item, c: &Choice) {
    let current = view
        .snap
        .state
        .setting(c.setting)
        .and_then(|v| c.selected(v));
    let choices: Vec<(usize, String)> = c
        .options
        .iter()
        .enumerate()
        .map(|(i, (_, l))| (i, l.clone()))
        .collect();
    let mut picked = None;
    let id = format!("choice-{}", u8::from(c.setting));
    m3::setting_row(
        ui,
        item_icon(item),
        &item.title,
        item.description.as_deref(),
        TRAILING,
        |ui| {
            picked = dropdown(ui, &id, current, &choices, TRAILING - 10.0);
        },
    );
    if let Some(i) = picked {
        view.send(Command::SetValue {
            id: c.setting,
            payload: c.options[i].0.clone(),
        });
    }
}

fn sleep_timer(ui: &mut Ui, view: &mut ViewCtx<'_>, item: &Item) {
    let current = view
        .snap
        .state
        .power_off_timer
        .map(|t| if t.minutes == u16::MAX { 0 } else { t.minutes });
    let mut choices: Vec<(u16, String)> = SLEEP_TIMER_CHOICES
        .iter()
        .map(|&m| {
            (
                m,
                if m == 0 {
                    "Off".into()
                } else {
                    format!("{m} min")
                },
            )
        })
        .collect();
    if let Some(c) = current.filter(|c| !SLEEP_TIMER_CHOICES.contains(c)) {
        choices.push((c, format!("{c} min")));
    }
    let mut picked = None;
    m3::setting_row(
        ui,
        icons::ICON_TIMER,
        &item.title,
        Some("Power off after a set time, even while playing"),
        TRAILING,
        |ui| picked = dropdown(ui, "sleep-timer", current, &choices, TRAILING - 10.0),
    );
    if let Some(m) = picked {
        view.send(Command::SetPowerOffTimer(m));
    }
}

fn wear_detection(ui: &mut Ui, view: &mut ViewCtx<'_>, spec: &WearSpec, note: Option<&str>) {
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
        let mut choices: Vec<(WearMusicBehavior, String)> = spec
            .music
            .iter()
            .map(|(id, l)| (WearMusicBehavior::from_id(*id), l.clone()))
            .collect();
        if choices.is_empty() {
            choices = WearMusicBehavior::KNOWN
                .iter()
                .map(|b| (*b, b.label()))
                .collect();
        }
        let title = spec.music_title.as_deref().unwrap_or("Playback");
        m3::setting_row(ui, icons::ICON_PLAY_CIRCLE, title, None, TRAILING, |ui| {
            if let Some(m) = dropdown(
                ui,
                "wear-music",
                Some(current.music),
                &choices,
                TRAILING - 10.0,
            ) {
                new.music = m;
            }
        });
        if let Some(on) = m3::switch_row(
            ui,
            icons::ICON_NOISE_AWARE,
            spec.anc_title
                .as_deref()
                .unwrap_or("Noise cancelling follows wear"),
            Some(
                spec.anc_description
                    .as_deref()
                    .unwrap_or("Off when removed, back on when worn"),
            ),
            current.anc_follows_wear,
        ) {
            new.anc_follows_wear = on;
        }
        if !spec.sensitivity.is_empty() {
            let level = view
                .snap
                .state
                .setting(CommandId::InEarSensitivity)
                .and_then(|p| p.first().copied());
            let mut picked = None;
            m3::setting_row(ui, icons::ICON_TUNE, "Sensitivity", None, TRAILING, |ui| {
                picked = dropdown(ui, "wear-sens", level, &spec.sensitivity, TRAILING - 10.0);
            });
            if let Some(v) = picked {
                view.send(Command::SetValue {
                    id: CommandId::InEarSensitivity,
                    payload: vec![v],
                });
            }
        }
    });
    let s = Scheme::of(ui);
    m3::notice(
        ui,
        icons::ICON_INFO,
        note.unwrap_or("Changes apply after taking the earbuds out and putting them back in."),
        s.on_surface_variant,
    );
    if new != current {
        view.send(Command::SetWearDetection(new));
    }
}

/// Fit test: the earbuds measure the seal while sound plays (`[status, L, R]`).
fn fit_test(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let s = Scheme::of(ui);
    let report = view
        .snap
        .state
        .setting(CommandId::FitTest)
        .map(<[u8]>::to_vec);
    m3::supporting(
        ui,
        "Put both earbuds in, start the test and play some music for a few seconds.",
    );
    let label = |r: u8| match r {
        3 => "Good seal",
        1 | 2 => "Poor seal, try another ear tip size",
        _ => "No result",
    };
    if let Some([2, left, right, ..]) = report.as_deref() {
        m3::setting_row(ui, icons::ICON_EARBUDS, "Left", None, TRAILING, |ui| {
            ui.label(text::body_medium(label(*left)).color(s.on_surface_variant));
        });
        m3::setting_row(ui, icons::ICON_EARBUDS, "Right", None, TRAILING, |ui| {
            ui.label(text::body_medium(label(*right)).color(s.on_surface_variant));
        });
    } else if let Some([1, ..]) = report.as_deref() {
        m3::supporting(ui, "Testing…");
    }
    ui.horizontal(|ui| {
        if m3::button(ui, ButtonKind::Tonal, Some(icons::ICON_PLAY_ARROW), "Start").clicked() {
            view.send(Command::SetValue {
                id: CommandId::FitTest,
                payload: vec![1, 1, 1],
            });
        }
        if m3::button(ui, ButtonKind::Text, None, "Stop").clicked() {
            view.send(Command::SetValue {
                id: CommandId::FitTest,
                payload: vec![0, 0, 0],
            });
        }
    });
}

fn game_config(ui: &mut Ui, view: &mut ViewCtx<'_>, item: &Item, sync_label: Option<&str>) {
    let reported = view
        .snap
        .state
        .setting(CommandId::GameConfig)
        .map(<[u8]>::to_vec);
    let enabled = reported.as_ref().and_then(|p| p.first()) == Some(&1);
    let sync = reported.as_ref().and_then(|p| p.get(1)) == Some(&1);
    let send = |view: &mut ViewCtx<'_>, enabled: bool, sync: bool| {
        view.confirm(Confirmation::new(
            icons::ICON_SPORTS_ESPORTS,
            &format!("Change {}?", item.title),
            item.description
                .as_deref()
                .unwrap_or("The earbuds restart to apply this setting."),
            "Apply",
            Command::SetValue {
                id: CommandId::GameConfig,
                payload: vec![u8::from(enabled), u8::from(sync)],
            },
        ));
    };
    if let Some(on) = m3::switch_row(ui, icons::ICON_SPORTS_ESPORTS, &item.title, None, enabled) {
        send(view, on, sync);
    }
    if let Some(label) = sync_label {
        ui.add_enabled_ui(enabled, |ui| {
            if let Some(on) = m3::switch_row(ui, icons::ICON_GRAPHIC_EQ, label, None, sync) {
                send(view, enabled, on);
            }
        });
    }
}

fn multipoint(ui: &mut Ui, view: &mut ViewCtx<'_>, item: &Item) {
    let on = view
        .snap
        .state
        .setting_on(CommandId::DualDevice)
        .unwrap_or(false);
    if let Some(new) = m3::switch_row(
        ui,
        icons::ICON_DEVICES,
        &item.title,
        item.description.as_deref(),
        on,
    ) {
        let command = Command::SetValue {
            id: CommandId::DualDevice,
            payload: vec![u8::from(new)],
        };
        view.confirm(Confirmation::new(
            icons::ICON_DEVICES,
            &format!(
                "{} {}?",
                if new { "Turn on" } else { "Turn off" },
                item.title
            ),
            "The earbuds restart to apply this setting.",
            "Apply",
            command,
        ));
    }
    let phones = view
        .snap
        .state
        .setting(CommandId::PhoneInfo)
        .map(PairedPhone::parse_list)
        .unwrap_or_default();
    let s = Scheme::of(ui);
    for phone in phones {
        let status = if phone.is_connected() {
            "Connected"
        } else {
            "Paired"
        };
        let name = if phone.name.is_empty() {
            phone.address.to_string()
        } else {
            phone.name.clone()
        };
        m3::setting_row(
            ui,
            icons::ICON_SMARTPHONE,
            &name,
            Some(&phone.address.to_string()),
            TRAILING,
            |ui| {
                ui.label(text::body_medium(status).color(s.on_surface_variant));
            },
        );
    }
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

fn rename(ui: &mut Ui, view: &mut ViewCtx<'_>, max_len: usize) {
    let s = Scheme::of(ui);
    let current = view.snap.state.device_name.clone().unwrap_or_default();
    let mut name = view.drafts.name.clone().unwrap_or_else(|| current.clone());
    let max_len = max_len.min(MAX_NAME_LEN);
    ui.label(text::body_small("Bluetooth name").color(s.on_surface_variant));
    ui.horizontal(|ui| {
        let edit = ui.add(
            egui::TextEdit::singleline(&mut name)
                .desired_width((ui.available_width() - 130.0).max(120.0))
                .char_limit(max_len)
                .margin(egui::vec2(12.0, 10.0))
                .font(crate::material::regular(15.0)),
        );
        if edit.changed() {
            view.drafts.name = Some(name.clone());
        }
        let trimmed = name.trim();
        let valid = !trimmed.is_empty() && trimmed.len() <= max_len && trimmed != current;
        ui.add_enabled_ui(valid, |ui| {
            if m3::button(ui, ButtonKind::Tonal, Some(icons::ICON_EDIT), "Rename").clicked() {
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
}

/// The confirmation for a panel action (reset, clear pairing, …).
fn action_confirmation(item: &Item, action: &PanelAction) -> Confirmation {
    let (icon, message, destructive) = match action.kind {
        ActionKind::ResetDefaults => (
            icons::ICON_RESTART_ALT,
            "All earbud settings return to their factory values. Pairings are kept.",
            false,
        ),
        ActionKind::ClearPairing => (
            icons::ICON_LINK_OFF,
            "The earbuds forget every paired device, this computer included. You will need to pair them again.",
            true,
        ),
        ActionKind::FactoryReset => (
            icons::ICON_DEVICE_RESET,
            "This erases all settings and every pairing. You will need to pair the earbuds again.",
            true,
        ),
        ActionKind::Other => (icons::ICON_WARNING, "", false),
    };
    let message = match (message, action.message.as_deref()) {
        ("", Some(m)) => m.to_string(),
        ("", None) => "Are you sure?".to_string(),
        (m, _) => m.to_string(),
    };
    let confirmation = Confirmation::new(
        icon,
        &format!("{}?", item.title),
        &message,
        "Continue",
        Command::Panel(PanelWrite {
            write: action.write.clone(),
            setting: None,
            value: None,
        }),
    );
    if destructive {
        confirmation.destructive()
    } else {
        confirmation
    }
}

fn device(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let snap = view.snap;
    let panel = view.panel;
    m3::titled_card(ui, icons::ICON_EARBUDS, "Device", |ui| {
        let s = Scheme::of(ui);
        if let Some(max_len) = panel.items.iter().find_map(|i| match i.kind {
            ItemKind::Rename { max_len } => Some(max_len),
            _ => None,
        }) {
            rename(ui, view, max_len);
            m3::divider(ui);
        }

        let version = |v: Option<Version>| v.map_or("–".into(), |v| v.to_string());
        let mut rows: Vec<(MaterialIcon, &str, String)> = Vec::new();
        if let Some(d) = &snap.device {
            rows.push((
                icons::ICON_EARBUDS,
                "Model",
                format!("{} ({})", d.model.unwrap_or("Unknown model"), d.vendor_id),
            ));
            let chip = match d.dialect {
                qcy_protocol::dialect::Dialect::Qcy => format!(
                    "{:?}",
                    qcy_protocol::advert::ChipFamily::from_vendor_id(d.vendor_id)
                ),
                qcy_protocol::dialect::Dialect::JieLi { .. } => "JieLi".into(),
                qcy_protocol::dialect::Dialect::Zr => "ZR".into(),
            };
            rows.push((icons::ICON_MEMORY, "Chip", chip));
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
            let value = if fw.left == fw.right {
                version(fw.left)
            } else {
                format!("L {}  ·  R {}", version(fw.left), version(fw.right))
            };
            rows.push((icons::ICON_DEVELOPER_BOARD, "Firmware", value));
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
        let actions: Vec<(&Item, &PanelAction)> = panel
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                ItemKind::Action(a) => Some((i, a)),
                _ => None,
            })
            .collect();
        ui.horizontal_wrapped(|ui| {
            if m3::button(
                ui,
                ButtonKind::Outlined,
                Some(icons::ICON_REFRESH),
                "Reload",
            )
            .clicked()
            {
                view.act(Action::Refresh);
            }
            for (item, action) in actions {
                let kind = match action.kind {
                    ActionKind::FactoryReset | ActionKind::ClearPairing => ButtonKind::Danger,
                    _ => ButtonKind::Outlined,
                };
                let icon = match action.kind {
                    ActionKind::ResetDefaults => icons::ICON_RESTART_ALT,
                    ActionKind::ClearPairing => icons::ICON_LINK_OFF,
                    ActionKind::FactoryReset => icons::ICON_DEVICE_RESET,
                    ActionKind::Other => icons::ICON_TUNE,
                };
                if m3::button(ui, kind, Some(icon), &item.title).clicked() {
                    view.confirm(action_confirmation(item, action));
                }
            }
        });
    });
}
