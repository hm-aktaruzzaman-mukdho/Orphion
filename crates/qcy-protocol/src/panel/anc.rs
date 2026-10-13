//! Noise control layouts (panel types 9, 6 and 11).
//!
//! Every noise control UI of the app ends in one packed integer `v`
//! (`QCYConnectManager.setNoiseMode`):
//!
//! * `v <= 255`: legacy noise mode, command `0x0C`: `FF 03 0C 01 v`.
//! * otherwise `mode = v >> 16`, `scene = v >> 8`, `level = v` (bytes),
//!   command `0x17`: `FF 05 17 03 mode scene level`; `(3, 1, 0)` is sent as
//!   `(3, 2, 0)` ("voice focus" transparency).
//!
//! So a panel picks the command by the magnitude of its numbers.

use serde_json::Value;

use super::json::{array, int, string};
use crate::Command;
use crate::types::{AncMode, AncSetting};

/// The noise control section of a panel.
#[derive(Debug, Clone, PartialEq)]
pub enum AncControl {
    /// Layout 9: modes with scenes, level sliders and voice focus.
    Scenes(SceneAnc),
    /// Layout 6: plain mode buttons using the legacy command `0x0C`.
    Legacy(LegacyAnc),
    /// Layout 11: modes with a strength in percent.
    Percent(PercentAnc),
}

impl AncControl {
    pub fn title(&self) -> &str {
        match self {
            AncControl::Scenes(a) => &a.title,
            AncControl::Legacy(a) => &a.title,
            AncControl::Percent(a) => &a.title,
        }
    }
}

/// Layout 9.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneAnc {
    pub title: String,
    pub modes: Vec<SceneMode>,
    /// Labels of the values of a [`ModeKind::Ruler`] (`levelNames`).
    pub level_names: Vec<String>,
}

/// A top-level noise control mode ("Noise cancelling", "Transparency", …).
///
/// The mode is active while the device's packed value lies in
/// `start..=end`.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneMode {
    pub name: String,
    pub start: u32,
    pub end: u32,
    /// Sent when the mode is picked and no value of it was seen yet.
    pub default: u32,
    pub kind: ModeKind,
    /// Labels for the optional "environment adaptation" switch
    /// (command `0x32`); empty when the mode has none.
    pub env_adaptation: Vec<String>,
}

/// How a [`SceneMode`] is refined (`viewtype` in the panel).
#[derive(Debug, Clone, PartialEq)]
pub enum ModeKind {
    /// viewtype 1: a choice of scenes, each with its own level range.
    Scenes(Vec<SubScene>),
    /// viewtype 2: a level slider over `start+1 ..= end`, plus an optional
    /// "voice focus" variant.
    Level { voice: Option<u32> },
    /// viewtype 4: a ruler over `start & 0xFF ..= end & 0xFF`, sent as
    /// `(default & 0xFFFF00) + n`.
    Ruler,
    /// viewtype 3 (and anything unknown): just a button.
    Button,
}

/// A scene inside a [`ModeKind::Scenes`] mode ("Indoor", "Commuting", …).
/// Its levels are `start ..= end`; a single value means "no levels".
#[derive(Debug, Clone, PartialEq)]
pub struct SubScene {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub default: u32,
    pub description: Option<String>,
}

impl SubScene {
    pub fn contains(&self, v: u32) -> bool {
        (self.start..=self.end).contains(&v)
    }

    /// Number of selectable levels beyond the first (0 = no slider).
    pub fn max_level(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }
}

impl SceneMode {
    pub fn contains(&self, v: u32) -> bool {
        (self.start..=self.end).contains(&v)
            || matches!(self.kind, ModeKind::Level { voice: Some(x) } if x == v)
    }

    /// Level slider range of a [`ModeKind::Level`] mode, as packed values.
    pub fn levels(&self) -> std::ops::RangeInclusive<u32> {
        match self.kind {
            ModeKind::Ruler => {
                let base = self.default & 0xFF_FF00;
                base + (self.start & 0xFF)..=base + (self.end & 0xFF)
            }
            _ => self.start + 1..=self.end,
        }
    }
}

/// Layout 6.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyAnc {
    pub title: String,
    /// `(label, noise mode)`; meaning is given only by the label.
    pub modes: Vec<(String, u8)>,
}

/// Layout 11.
#[derive(Debug, Clone, PartialEq)]
pub struct PercentAnc {
    pub title: String,
    pub modes: Vec<PercentMode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PercentMode {
    pub name: String,
    /// Noise mode: 1 ANC, 2 off, 3 transparency (some models use others).
    pub mode: u8,
    /// Highest strength in percent; 0 = no strength slider.
    pub max_percent: u8,
}

impl PercentMode {
    /// Packed value for this mode at `percent` (`setNoiseLevelPercent`).
    pub fn packed(&self, percent: u8) -> u32 {
        let scene = u8::from(self.mode != 2);
        AncSetting::new(self.mode, scene, percent.min(self.max_percent)).packed()
    }
}

/// The command that applies a packed noise control value.
pub fn command(v: u32) -> Command {
    if v <= 0xFF {
        return Command::SetNoiseMode(v as u8);
    }
    let mut s = AncSetting::from_packed(v);
    if (s.mode, s.scene, s.level) == (3, 1, 0) {
        s.scene = 2;
    }
    Command::SetAnc(AncMode::from_setting(s))
}

/// The packed value the panel ranges are matched against
/// (`Devicebind.getCurrentANCModeIndex`): voice focus `(3, 2, x)` is
/// reported as `0x030100`.
pub fn current(setting: AncSetting) -> u32 {
    if setting.mode == 3 && setting.scene == 2 {
        0x03_0100
    } else {
        setting.packed()
    }
}

fn packed(v: &Value, key: &str) -> Option<u32> {
    int(v, key).and_then(|n| u32::try_from(n).ok())
}

pub(super) fn parse_scenes(layout: &Value) -> Option<AncControl> {
    let modes: Vec<SceneMode> = array(layout, "modes")
        .iter()
        .filter_map(|m| {
            let start = packed(m, "startcmdid")?;
            let end = packed(m, "endcmdid").unwrap_or(start).max(start);
            let default = packed(m, "defaultcmd").unwrap_or(start);
            let kind = match int(m, "viewtype") {
                Some(1) => ModeKind::Scenes(
                    array(m, "items")
                        .iter()
                        .filter_map(|i| {
                            let start = packed(i, "startcmdid")?;
                            let end = packed(i, "endcmdid").unwrap_or(start).max(start);
                            Some(SubScene {
                                name: string(i, "name").unwrap_or_default(),
                                start,
                                end,
                                default: packed(i, "defaultcmd").unwrap_or(start),
                                description: string(i, "describe"),
                            })
                        })
                        .collect(),
                ),
                Some(2) => ModeKind::Level {
                    voice: packed(m, "voicecmdid").filter(|&v| v != 0),
                },
                Some(4) => ModeKind::Ruler,
                _ => ModeKind::Button,
            };
            Some(SceneMode {
                name: string(m, "name").unwrap_or_default(),
                start,
                end,
                default,
                kind,
                env_adaptation: array(m, "envAdaptation")
                    .iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect(),
            })
        })
        .collect();
    (!modes.is_empty()).then(|| {
        AncControl::Scenes(SceneAnc {
            title: string(layout, "name").unwrap_or_else(|| "Noise control".into()),
            modes,
            level_names: array(layout, "levelNames")
                .iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect(),
        })
    })
}

pub(super) fn parse_legacy(layout: &Value) -> Option<AncControl> {
    let modes: Vec<(String, u8)> = array(layout, "modes")
        .iter()
        .filter_map(|m| {
            let id = u8::try_from(int(m, "cmdid")?).ok()?;
            Some((
                string(m, "name").unwrap_or_else(|| format!("Mode {id}")),
                id,
            ))
        })
        .collect();
    (!modes.is_empty()).then(|| {
        AncControl::Legacy(LegacyAnc {
            title: string(layout, "name").unwrap_or_else(|| "Noise control".into()),
            modes,
        })
    })
}

pub(super) fn parse_percent(layout: &Value) -> Option<AncControl> {
    let modes: Vec<PercentMode> = array(layout, "modes")
        .iter()
        .filter_map(|m| {
            Some(PercentMode {
                name: string(m, "name").unwrap_or_default(),
                mode: u8::try_from(int(m, "ancindex")?).ok()?,
                max_percent: int(m, "progress").unwrap_or(0).clamp(0, 100) as u8,
            })
        })
        .collect();
    (!modes.is_empty()).then(|| {
        AncControl::Percent(PercentAnc {
            title: string(layout, "name").unwrap_or_else(|| "Noise control".into()),
            modes,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AncScene;

    #[test]
    fn commands_follow_the_app() {
        assert_eq!(command(2), Command::SetNoiseMode(2));
        assert_eq!(
            command(0x01_0201),
            Command::SetAnc(AncMode::NoiseCancelling {
                scene: AncScene::Commuting,
                level: 1
            })
        );
        // Voice focus is rewritten from (3,1,0) to (3,2,0).
        assert_eq!(
            command(0x03_0100),
            Command::SetAnc(AncMode::VoiceTransparency)
        );
        assert_eq!(current(AncMode::VoiceTransparency.to_setting()), 0x03_0100);
    }
}
