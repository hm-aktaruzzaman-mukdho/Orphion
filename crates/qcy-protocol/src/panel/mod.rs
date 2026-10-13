//! QCY control panels: the per-model description of every control.
//!
//! The official app has no per-model code. It downloads a JSON "control
//! panel" for the connected model and builds its whole UI from it: noise
//! control modes and their command values, EQ presets and bands, the touch
//! gestures and their action lists, and a list of settings rows whose
//! switches carry the exact bytes to write. This module turns such a
//! document into typed data ([`Panel`]); [`crate::catalog`] holds a copy of
//! every model's panel.
//!
//! The format, every layout type and the app's quirks are documented in
//! `research/notes/panel_semantics.md`; what each part does is summarised on
//! the submodules.

mod anc;
mod eq;
mod items;
mod json;
mod keys;

use serde_json::Value;

pub use anc::{
    AncControl, LegacyAnc, ModeKind, PercentAnc, PercentMode, SceneAnc, SceneMode, SubScene,
};
pub use anc::{command as anc_command, current as anc_current};
pub use eq::{ADAPTIVE_PRESET, BandSpec, EqControl, EqKind, EqPreset};
pub use items::{Action, ActionKind, Choice, Item, ItemKind, Switch, WearDetection};
pub use keys::{FixedGesture, KeyControl, KeyGroup, KeyRow, KeySlot, KeyTarget};

use crate::{Characteristic, CommandId};

/// Everything a front-end needs to build the controls of one model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Panel {
    pub anc: Option<AncControl>,
    pub eq: Option<EqControl>,
    /// Game EQ presets (layout 23), applied like graphic presets.
    pub game_eq: Option<EqControl>,
    /// Touch gestures written to the key table `0x000D` (layout 10).
    pub touch_keys: Option<KeyControl>,
    /// Touch gestures written to per-gesture characteristics (layout 1).
    pub legacy_keys: Option<KeyControl>,
    /// Headphone buttons (layout 14).
    pub buttons: Option<KeyControl>,
    /// Title of the left/right balance control (layout 8), if offered.
    pub balance: Option<String>,
    /// Settings rows, in panel order.
    pub items: Vec<Item>,
    /// Settings the app requests after connecting (`readCmdIndexList`).
    pub read_settings: Vec<CommandId>,
}

/// A malformed panel document.
#[derive(Debug, thiserror::Error)]
#[error("invalid control panel: {0}")]
pub struct PanelError(String);

impl Panel {
    /// Parses a `controlPanel` JSON document.
    pub fn parse_str(json: &str) -> Result<Self, PanelError> {
        let value: Value = serde_json::from_str(json).map_err(|e| PanelError(e.to_string()))?;
        Self::parse(&value)
    }

    /// Parses a `controlPanel` JSON value (`{"layouts": [...]}`).
    ///
    /// Unknown layouts and entries are skipped, like in the app, so a panel
    /// from a newer server never fails as a whole.
    pub fn parse(value: &Value) -> Result<Self, PanelError> {
        let layouts = value
            .get("layouts")
            .and_then(Value::as_array)
            .ok_or_else(|| PanelError("no \"layouts\" array".into()))?;
        let mut p = Panel::default();
        let mut read = Vec::new();
        for layout in layouts {
            let Some(kind) = json::int(layout, "type") else {
                continue;
            };
            match kind {
                1 => p.legacy_keys = p.legacy_keys.or_else(|| keys::parse_legacy(layout)),
                2 => p.eq = p.eq.or_else(|| eq::parse(layout)),
                4 => {
                    for raw in json::array(layout, "items") {
                        let id = json::int(raw, "cmdid").and_then(|i| u8::try_from(i).ok());
                        // A multipoint layout (113) already covers command 0x24.
                        if id == Some(0x24) && read.contains(&0x24) {
                            continue;
                        }
                        let Some(item) = items::parse_type4_item(raw) else {
                            continue;
                        };
                        read.extend(id);
                        p.items.push(item);
                    }
                }
                6 => p.anc = p.anc.or_else(|| anc::parse_legacy(layout)),
                8 => {
                    p.balance =
                        Some(json::string(layout, "name").unwrap_or_else(|| "Sound balance".into()))
                }
                9 => {
                    p.anc = p.anc.or_else(|| anc::parse_scenes(layout));
                    read.push(CommandId::Anc.into());
                    let adaptive = json::array(layout, "modes")
                        .iter()
                        .any(|m| m.get("envAdaptation").is_some());
                    if adaptive {
                        read.push(CommandId::EnvAdaptation.into());
                    }
                }
                10 => p.touch_keys = p.touch_keys.or_else(|| keys::parse_touch(layout)),
                11 => p.anc = p.anc.or_else(|| anc::parse_percent(layout)),
                // "Ids to read", ignored by the app but harmless to honour.
                13 => read.extend(
                    json::array(layout, "cmdIndexs")
                        .iter()
                        .filter_map(|v| json::as_int(v).and_then(|i| u8::try_from(i).ok())),
                ),
                14 => p.buttons = p.buttons.or_else(|| keys::parse_buttons(layout)),
                23 => p.game_eq = p.game_eq.or_else(|| eq::parse_game(layout)),
                other => {
                    if let Some(item) = items::parse_layout(other, layout) {
                        read.extend(item_reads(&item, layout));
                        p.items.push(item);
                    }
                }
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        read.retain(|id| seen.insert(*id));
        p.read_settings = read.into_iter().map(CommandId::from).collect();
        Ok(p)
    }

    /// The touch gesture section to use for a device, mirroring the app's
    /// choice (`getKEyFuncList`): the per-gesture layout when the device has
    /// those characteristics, otherwise the key table layout.
    pub fn keys_for(&self, has: impl Fn(Characteristic) -> bool) -> Option<&KeyControl> {
        let legacy_usable = self.legacy_keys.as_ref().is_some_and(|k| {
            k.slots().any(|s| match s.target {
                KeyTarget::Characteristic(c) => has(c),
                KeyTarget::Table => false,
            })
        });
        if legacy_usable {
            return self.legacy_keys.as_ref();
        }
        self.touch_keys.as_ref().or(self.legacy_keys.as_ref())
    }

    /// Settings rows shown on the main page.
    pub fn main_items(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| !i.more)
    }

    /// Settings rows of the secondary "More" page.
    pub fn more_items(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| i.more)
    }
}

/// Settings a single-value layout reads back after connecting.
fn item_reads(item: &Item, layout: &Value) -> Vec<u8> {
    let cmdid = json::int(layout, "cmdid").and_then(|i| u8::try_from(i).ok());
    let mut ids: Vec<u8> = match &item.kind {
        ItemKind::Rename { .. } => vec![CommandId::DeviceName.into()],
        ItemKind::SleepTimer => vec![CommandId::PowerOffTimer.into()],
        ItemKind::PromptVolume { .. } => vec![CommandId::PromptVolume.into()],
        ItemKind::Language { .. } => vec![CommandId::PromptLanguage.into()],
        ItemKind::WearDetection(w) => {
            let mut ids = vec![CommandId::WearDetection.into()];
            if !w.sensitivity.is_empty() {
                ids.push(CommandId::InEarSensitivity.into());
            }
            ids
        }
        ItemKind::GameConfig { .. } => vec![CommandId::GameConfig.into()],
        ItemKind::Multipoint => vec![CommandId::DualDevice.into(), CommandId::PhoneInfo.into()],
        ItemKind::AiAnc => vec![CommandId::AiAnc.into()],
        ItemKind::FocusMode => vec![CommandId::FocusMode.into()],
        ItemKind::Volume => vec![CommandId::Volume.into()],
        _ => Vec::new(),
    };
    ids.extend(cmdid);
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn melobuds_pro_panel() {
        let p = catalog::panel(19786);
        let Some(AncControl::Scenes(anc)) = &p.anc else {
            panic!("scene ANC expected")
        };
        assert_eq!(anc.modes.len(), 3);
        assert!(matches!(&anc.modes[0].kind, ModeKind::Scenes(s) if s.len() == 5));
        assert!(matches!(
            anc.modes[1].kind,
            ModeKind::Level {
                voice: Some(0x03_0100)
            }
        ));
        let eq = p.eq.as_ref().unwrap();
        assert_eq!(eq.kind, EqKind::ParametricV2);
        assert!(eq.preset(10).is_some());
        assert!(p.touch_keys.is_some() && p.balance.is_some());
        let switches = p
            .items
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::Switch(_)))
            .count();
        assert_eq!(switches, 4);
        assert!(
            p.items
                .iter()
                .any(|i| matches!(i.kind, ItemKind::WearDetection(_)))
        );
        assert!(p.read_settings.contains(&CommandId::Anc));
        // No firmware update row.
        assert!(
            !p.items
                .iter()
                .any(|i| i.title.to_lowercase().contains("firmware"))
        );
    }

    #[test]
    fn every_model_has_controls() {
        for m in catalog::models() {
            let p = catalog::panel(m.id);
            assert!(
                p.eq.is_some() || p.anc.is_some() || p.touch_keys.is_some() || !p.items.is_empty(),
                "{} ({}) has no controls",
                m.name,
                m.id
            );
        }
    }
}
