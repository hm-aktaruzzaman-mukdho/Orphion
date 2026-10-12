//! What the tray shows and does, independent of how a platform draws it.
//!
//! [`content`] turns a [`Snapshot`] into a tooltip and a list of [`Entry`]s;
//! each platform module renders them with its own toolkit and hands clicked
//! [`Action`]s to [`perform`].

use qcy_client::Selector;
use qcy_client::manager::{LinkStatus, Manager, Snapshot};
use qcy_protocol::Command;
use qcy_protocol::types::{AncMode, AncScene, BatteryLevel, Toggle};

/// Something the user can trigger from the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// 0 = off, 1 = noise cancelling, 2 = transparency.
    SetNoise(usize),
    SetGameMode(bool),
    EqPreset(u8),
    OpenWindow,
    Connect,
    Disconnect,
    Quit,
}

/// One menu entry.
#[derive(Debug, Clone)]
pub enum Entry {
    /// Informational, not clickable.
    Label(String),
    Separator,
    /// Mutually exclusive options.
    Radio {
        options: Vec<(String, Action)>,
        selected: Option<usize>,
    },
    Check {
        label: String,
        checked: bool,
        action: Action,
    },
    Submenu {
        label: String,
        entries: Vec<Entry>,
    },
    /// A command; `icon` is a freedesktop icon name (ignored where unsupported).
    Button {
        label: String,
        #[cfg_attr(windows, allow(dead_code))]
        icon: &'static str,
        action: Action,
    },
}

/// Everything a tray renders.
#[derive(Debug, Clone)]
pub struct Content {
    pub title: String,
    pub tooltip: Vec<String>,
    pub entries: Vec<Entry>,
}

const NOISE_MODES: [&str; 3] = ["Off", "Noise cancelling", "Transparency"];

pub fn content(snap: &Snapshot) -> Content {
    let name = device_name(snap);
    let battery = battery_line(snap);

    let mut tooltip = vec![snap.link.label()];
    tooltip.extend(battery.clone());
    tooltip.extend(snap.state.anc.map(AncMode::describe));

    let mut entries = vec![Entry::Label(format!("{name} — {}", snap.link.label()))];
    if let Some(b) = battery {
        entries.push(Entry::Label(format!("Battery: {b}")));
    }
    entries.push(Entry::Separator);

    let profile = snap.device.as_ref().map(|d| d.profile);
    if snap.is_connected()
        && let Some(profile) = profile
    {
        if profile.anc.is_some() {
            entries.push(Entry::Radio {
                options: NOISE_MODES
                    .iter()
                    .enumerate()
                    .map(|(i, label)| (label.to_string(), Action::SetNoise(i)))
                    .collect(),
                selected: noise_index(snap),
            });
            entries.push(Entry::Separator);
        }
        if profile.toggles.contains(&Toggle::GameMode) {
            let on = snap.state.toggle(Toggle::GameMode).unwrap_or(false);
            entries.push(Entry::Check {
                label: "Gaming mode".into(),
                checked: on,
                action: Action::SetGameMode(!on),
            });
        }
        // Presets that need a confirmation (e.g. ones that reboot the
        // earbuds) are left to the control panel.
        let presets: Vec<_> = profile
            .eq_presets
            .iter()
            .filter(|p| p.warning.is_none())
            .collect();
        if !presets.is_empty() {
            let active = snap.state.active_eq_preset();
            entries.push(Entry::Submenu {
                label: "Equalizer".into(),
                entries: vec![Entry::Radio {
                    selected: presets.iter().position(|p| Some(p.id) == active),
                    options: presets
                        .iter()
                        .map(|p| (p.name.to_string(), Action::EqPreset(p.id)))
                        .collect(),
                }],
            });
        }
        entries.push(Entry::Separator);
    }

    entries.push(Entry::Button {
        label: "Open control panel…".into(),
        icon: "configure",
        action: Action::OpenWindow,
    });
    entries.push(match snap.link {
        LinkStatus::Idle => Entry::Button {
            label: "Connect".into(),
            icon: "network-connect",
            action: Action::Connect,
        },
        _ => Entry::Button {
            label: "Disconnect".into(),
            icon: "network-disconnect",
            action: Action::Disconnect,
        },
    });
    entries.push(Entry::Button {
        label: "Quit".into(),
        icon: "application-exit",
        action: Action::Quit,
    });

    Content {
        title: name,
        tooltip,
        entries,
    }
}

/// Executes the device-related actions. `OpenWindow` and `Quit` concern the
/// tray process itself and are handled by the caller.
pub fn perform(action: &Action, snap: &Snapshot, manager: &Manager) {
    match action {
        Action::SetNoise(index) => {
            if let Some(mode) = noise_mode(snap, *index) {
                manager.send(Command::SetAnc(mode));
            }
        }
        Action::SetGameMode(on) => manager.send(Command::SetToggle(Toggle::GameMode, *on)),
        Action::EqPreset(id) => {
            let preset = snap.device.as_ref().and_then(|d| d.profile.eq_preset(*id));
            if let Some(p) = preset {
                manager.send(Command::SetParametricEq(p.to_parametric()));
            }
        }
        Action::Connect => manager.connect(Selector::Auto),
        Action::Disconnect => manager.disconnect(),
        Action::OpenWindow | Action::Quit => {}
    }
}

/// The next noise mode when cycling with the mouse wheel (the Windows
/// notification area does not report wheel events).
#[cfg_attr(windows, allow(dead_code))]
pub fn cycle_noise(snap: &Snapshot, step: i32) -> Option<Action> {
    if !snap.is_connected() || step == 0 {
        return None;
    }
    let current = noise_index(snap).unwrap_or(0) as i32;
    let next = (current + step.signum()).rem_euclid(NOISE_MODES.len() as i32);
    Some(Action::SetNoise(next as usize))
}

fn noise_index(snap: &Snapshot) -> Option<usize> {
    Some(match snap.state.anc? {
        AncMode::Off => 0,
        AncMode::NoiseCancelling { .. } => 1,
        AncMode::Transparency { .. } | AncMode::VoiceTransparency => 2,
        AncMode::Other(_) => return None,
    })
}

fn noise_mode(snap: &Snapshot, index: usize) -> Option<AncMode> {
    let caps = snap.device.as_ref()?.profile.anc;
    Some(match index {
        0 => AncMode::Off,
        1 => AncMode::NoiseCancelling {
            scene: caps
                .and_then(|c| c.scenes.first().copied())
                .unwrap_or(AncScene::Adaptive),
            level: 0,
        },
        _ => AncMode::Transparency {
            level: caps.map_or(4, |c| c.default_transparency_level),
        },
    })
}

fn device_name(snap: &Snapshot) -> String {
    // In the charging case the earbuds may report an empty name.
    snap.state
        .device_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| snap.device.as_ref().map(|d| d.name.clone()))
        .unwrap_or_else(|| "QCY Earbuds".into())
}

fn battery_line(snap: &Snapshot) -> Option<String> {
    let b = snap.state.battery?;
    let cell = |label: &str, l: Option<BatteryLevel>| {
        l.map(|l| {
            format!(
                "{label} {}%{}",
                l.percent,
                if l.charging { "+" } else { "" }
            )
        })
    };
    let parts: Vec<String> = [cell("L", b.left), cell("R", b.right), cell("Case", b.case)]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}
