//! What the tray shows and does, independent of how a platform draws it.
//!
//! [`content`] turns a [`Snapshot`] into a tooltip and a list of [`Entry`]s;
//! each platform module renders them with its own toolkit and hands clicked
//! [`Action`]s to [`perform`].

use qcy_client::Selector;
use qcy_client::manager::{LinkStatus, Manager, Snapshot};
use qcy_protocol::panel::{AncControl, ItemKind, Panel, anc_command};
use qcy_protocol::types::BatteryLevel;
use qcy_protocol::{Command, CommandId, dialect};

/// Something the user can trigger from the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A noise control mode, by index into the model's panel modes.
    SetNoise(usize),
    /// A panel switch, by index into the panel's items.
    Switch {
        item: usize,
        on: bool,
    },
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

/// Settings that restart the earbuds; left to the control panel, which
/// asks first.
fn restarts(id: Option<CommandId>) -> bool {
    matches!(
        id,
        Some(CommandId::Ldac | CommandId::DualDevice | CommandId::Aptx)
    )
}

/// Names of the model's noise control modes.
fn noise_modes(panel: &Panel) -> Vec<String> {
    match &panel.anc {
        Some(AncControl::Scenes(a)) => a.modes.iter().map(|m| m.name.clone()).collect(),
        Some(AncControl::Legacy(a)) => a.modes.iter().map(|(n, _)| n.clone()).collect(),
        Some(AncControl::Percent(a)) => a.modes.iter().map(|m| m.name.clone()).collect(),
        None => Vec::new(),
    }
}

pub fn content(snap: &Snapshot) -> Content {
    let name = device_name(snap);
    let battery = battery_line(snap);

    let mut tooltip = vec![snap.link.label()];
    tooltip.extend(battery.clone());
    if let Some(d) = &snap.device
        && let Some(i) = noise_index(snap)
    {
        tooltip.extend(noise_modes(&d.panel).get(i).cloned());
    }

    let mut entries = vec![Entry::Label(format!("{name} — {}", snap.link.label()))];
    if let Some(b) = battery {
        entries.push(Entry::Label(format!("Battery: {b}")));
    }
    entries.push(Entry::Separator);

    let panel = snap.device.as_ref().map(|d| &d.panel);
    if snap.is_connected()
        && let Some(panel) = panel
    {
        let modes = noise_modes(panel);
        if !modes.is_empty() {
            entries.push(Entry::Radio {
                options: modes
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| (label, Action::SetNoise(i)))
                    .collect(),
                selected: noise_index(snap),
            });
            entries.push(Entry::Separator);
        }
        // Quick switches; ones that restart the earbuds or need a
        // confirmation are left to the control panel.
        for (i, item) in panel.items.iter().enumerate() {
            if let ItemKind::Switch(sw) = &item.kind
                && sw.warning.is_none()
                && !restarts(sw.setting)
            {
                let on = sw.is_on(&snap.state).unwrap_or(false);
                entries.push(Entry::Check {
                    label: item.title.clone(),
                    checked: on,
                    action: Action::Switch { item: i, on: !on },
                });
            }
        }
        if let Some(eq) = &panel.eq {
            let presets: Vec<_> = eq.presets.iter().filter(|p| p.warning.is_none()).collect();
            if !presets.is_empty() {
                let active = match eq.kind {
                    qcy_protocol::panel::EqKind::Graphic => {
                        snap.state.graphic_eq.as_ref().map(|g| g.preset)
                    }
                    _ => snap.state.active_eq_preset(),
                };
                entries.push(Entry::Submenu {
                    label: eq.title.clone(),
                    entries: vec![Entry::Radio {
                        selected: presets.iter().position(|p| Some(p.id) == active),
                        options: presets
                            .iter()
                            .map(|p| (p.name.clone(), Action::EqPreset(p.id)))
                            .collect(),
                    }],
                });
            }
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
    let panel = snap.device.as_ref().map(|d| &d.panel);
    match action {
        Action::SetNoise(index) => {
            if let Some(command) = panel.and_then(|p| noise_command(p, *index)) {
                manager.send(command);
            }
        }
        Action::Switch { item, on } => {
            if let Some(ItemKind::Switch(sw)) =
                panel.and_then(|p| p.items.get(*item)).map(|i| &i.kind)
            {
                manager.send(sw.command(*on));
            }
        }
        Action::EqPreset(id) => {
            if let Some(eq) = panel.and_then(|p| p.eq.as_ref())
                && let Some(preset) = eq.preset(*id)
            {
                for command in eq.apply_preset(preset) {
                    manager.send(command);
                }
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
    let count = noise_modes(&snap.device.as_ref()?.panel).len() as i32;
    if count == 0 {
        return None;
    }
    let current = noise_index(snap).unwrap_or(0) as i32;
    let next = (current + step.signum()).rem_euclid(count);
    Some(Action::SetNoise(next as usize))
}

/// Index of the active noise control mode in the panel's list.
fn noise_index(snap: &Snapshot) -> Option<usize> {
    let panel = &snap.device.as_ref()?.panel;
    match panel.anc.as_ref()? {
        AncControl::Scenes(a) => {
            let v = dialect::current_anc(&snap.state)?;
            a.modes.iter().position(|m| m.contains(v))
        }
        AncControl::Legacy(a) => {
            let m = *snap.state.setting(CommandId::NoiseMode)?.first()?;
            a.modes.iter().position(|(_, id)| *id == m)
        }
        AncControl::Percent(a) => {
            let mode = snap.state.anc?.to_setting().mode;
            a.modes.iter().position(|m| m.mode == mode)
        }
    }
}

/// The command that selects mode `index` with its default setting.
fn noise_command(panel: &Panel, index: usize) -> Option<Command> {
    Some(match panel.anc.as_ref()? {
        AncControl::Scenes(a) => anc_command(a.modes.get(index)?.default),
        AncControl::Legacy(a) => Command::SetNoiseMode(a.modes.get(index)?.1),
        AncControl::Percent(a) => {
            let m = a.modes.get(index)?;
            anc_command(m.packed(m.max_percent))
        }
    })
}

fn device_name(snap: &Snapshot) -> String {
    // In the charging case the earbuds may report an empty name.
    snap.state
        .device_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| snap.device.as_ref().map(|d| d.name.clone()))
        .unwrap_or_else(|| "QCY earbuds".into())
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
