//! The StatusNotifierItem (tray icon + menu) shown by KDE Plasma and other
//! desktops implementing the freedesktop/KDE tray protocol.

use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Category, Status, ToolTip};
use qcy_bluez::discovery::Selector;
use qcy_bluez::manager::{LinkStatus, Manager, Snapshot};
use qcy_protocol::Command;
use qcy_protocol::types::{AncMode, AncScene, BatteryLevel, Toggle};
use tokio::sync::mpsc;

/// Requests the tray cannot handle inside a menu callback.
#[derive(Debug)]
pub enum TrayRequest {
    OpenWindow,
    Quit,
}

pub struct QcyTray {
    pub manager: Manager,
    pub snapshot: Snapshot,
    pub requests: mpsc::UnboundedSender<TrayRequest>,
}

const NOISE_MODES: [&str; 3] = ["Off", "Noise cancelling", "Transparency"];

impl QcyTray {
    fn name(&self) -> String {
        self.snapshot
            .state
            .device_name
            .clone()
            .or_else(|| self.snapshot.device.as_ref().map(|d| d.name.clone()))
            .unwrap_or_else(|| "QCY Earbuds".into())
    }

    fn battery_line(&self) -> Option<String> {
        let b = self.snapshot.state.battery?;
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

    fn noise_index(&self) -> Option<usize> {
        Some(match self.snapshot.state.anc? {
            AncMode::Off => 0,
            AncMode::NoiseCancelling { .. } => 1,
            AncMode::Transparency { .. } | AncMode::VoiceTransparency => 2,
            AncMode::Other(_) => return None,
        })
    }

    /// Cycles Off → Noise cancelling → Transparency (mouse wheel / middle click).
    fn cycle_noise(&mut self, step: i32) {
        let current = self.noise_index().unwrap_or(0) as i32;
        let next = (current + step).rem_euclid(NOISE_MODES.len() as i32) as usize;
        self.set_noise(next);
    }

    fn set_noise(&mut self, index: usize) {
        let Some(device) = &self.snapshot.device else {
            return;
        };
        let caps = device.profile.anc;
        let mode = match index {
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
        };
        self.manager.send(Command::SetAnc(mode));
    }
}

impl ksni::Tray for QcyTray {
    fn id(&self) -> String {
        "qcy-tray".into()
    }

    fn title(&self) -> String {
        "QCY Earbuds".into()
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    fn icon_name(&self) -> String {
        "audio-headphones".into()
    }

    fn status(&self) -> Status {
        if self.snapshot.is_connected() {
            Status::Active
        } else {
            Status::Passive
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let mut lines = vec![self.snapshot.link.label()];
        if let Some(b) = self.battery_line() {
            lines.push(b);
        }
        if let Some(anc) = self.snapshot.state.anc {
            lines.push(anc.describe());
        }
        ToolTip {
            icon_name: "audio-headphones".into(),
            title: self.name(),
            description: lines.join("\n"),
            ..Default::default()
        }
    }

    /// Left click opens the control panel.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.requests.send(TrayRequest::OpenWindow).ok();
    }

    /// Mouse wheel over the icon cycles noise control.
    fn scroll(&mut self, delta: i32, _orientation: ksni::Orientation) {
        if self.snapshot.is_connected() && delta != 0 {
            self.cycle_noise(delta.signum());
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let connected = self.snapshot.is_connected();
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem {
                label: format!("{} — {}", self.name(), self.snapshot.link.label()),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];
        if let Some(line) = self.battery_line() {
            items.push(
                StandardItem {
                    label: format!("Battery: {line}"),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);

        let profile = self.snapshot.device.as_ref().map(|d| d.profile);
        if connected && profile.is_some_and(|p| p.anc.is_some()) {
            items.push(
                RadioGroup {
                    selected: self.noise_index().unwrap_or(usize::MAX),
                    select: Box::new(|tray: &mut Self, index| tray.set_noise(index)),
                    options: NOISE_MODES
                        .iter()
                        .map(|label| RadioItem {
                            label: (*label).into(),
                            ..Default::default()
                        })
                        .collect(),
                }
                .into(),
            );
            items.push(MenuItem::Separator);
        }

        if connected && let Some(profile) = profile {
            if profile.toggles.contains(&Toggle::GameMode) {
                let on = self
                    .snapshot
                    .state
                    .toggle(Toggle::GameMode)
                    .unwrap_or(false);
                items.push(
                    CheckmarkItem {
                        label: "Gaming mode".into(),
                        checked: on,
                        activate: Box::new(move |tray: &mut Self| {
                            tray.manager.send(Command::SetToggle(Toggle::GameMode, !on));
                        }),
                        ..Default::default()
                    }
                    .into(),
                );
            }
            // Presets that need a confirmation (e.g. ones that reboot the
            // earbuds) are left to the control panel.
            let presets: Vec<u8> = profile
                .eq_presets
                .iter()
                .filter(|p| p.warning.is_none())
                .map(|p| p.id)
                .collect();
            if !presets.is_empty() {
                let active = self.snapshot.state.active_eq_preset();
                let selected = presets
                    .iter()
                    .position(|&id| Some(id) == active)
                    .unwrap_or(usize::MAX);
                let options = presets
                    .iter()
                    .filter_map(|&id| profile.eq_preset(id))
                    .map(|p| RadioItem {
                        label: p.name.into(),
                        ..Default::default()
                    })
                    .collect();
                items.push(
                    SubMenu {
                        label: "Equalizer".into(),
                        submenu: vec![
                            RadioGroup {
                                selected,
                                select: Box::new(move |tray: &mut Self, index| {
                                    if let Some(p) =
                                        presets.get(index).and_then(|&id| profile.eq_preset(id))
                                    {
                                        tray.manager
                                            .send(Command::SetParametricEq(p.to_parametric()));
                                    }
                                }),
                                options,
                            }
                            .into(),
                        ],
                        ..Default::default()
                    }
                    .into(),
                );
            }
            items.push(MenuItem::Separator);
        }

        items.push(
            StandardItem {
                label: "Open control panel…".into(),
                icon_name: "configure".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.requests.send(TrayRequest::OpenWindow).ok();
                }),
                ..Default::default()
            }
            .into(),
        );
        let (label, icon) = match self.snapshot.link {
            LinkStatus::Idle => ("Connect", "network-connect"),
            _ => ("Disconnect", "network-disconnect"),
        };
        items.push(
            StandardItem {
                label: label.into(),
                icon_name: icon.into(),
                activate: Box::new(|tray: &mut Self| {
                    if tray.snapshot.link == LinkStatus::Idle {
                        tray.manager.connect(Selector::Auto);
                    } else {
                        tray.manager.disconnect();
                    }
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.requests.send(TrayRequest::Quit).ok();
                }),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}
