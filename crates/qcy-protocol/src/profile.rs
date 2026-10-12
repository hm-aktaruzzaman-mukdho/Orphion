//! Per-model capability descriptions.
//!
//! The official app downloads a "control panel" JSON for every model id from
//! `https://api.watch.qcy.com/product/findControlPanels` and builds its UI from
//! it. The relevant parts of those documents are transcribed here as static
//! data, so the desktop app works offline and needs no QCY account.
//!
//! **Adding a model:** capture its panel JSON (see `research/panel_json/` and
//! `docs/PROTOCOL.md`), add a `DeviceProfile` constant and list it in
//! [`PROFILES`]. Unknown models fall back to [`GENERIC`], and front-ends also
//! show any setting the device reports on its own.

use crate::CommandId;
use crate::eq::{EqBand, ParametricEq};
use crate::types::{AncScene, AutoPowerOff, Gesture, KeyAction, Side, Tap, Toggle};

/// A factory EQ preset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqPreset {
    pub id: u8,
    pub name: &'static str,
    pub master_gain_db: f32,
    pub bands: &'static [EqBand],
    /// Warning shown before applying, if any.
    pub warning: Option<&'static str>,
}

impl EqPreset {
    pub fn to_parametric(&self) -> ParametricEq {
        ParametricEq {
            index: self.id,
            master_gain_db: self.master_gain_db,
            bands: self.bands.to_vec(),
        }
    }
}

/// Noise control capabilities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AncCapabilities {
    pub scenes: &'static [AncScene],
    /// Inclusive `(min, max)` transparency strength, if adjustable.
    pub transparency_levels: Option<(u8, u8)>,
    pub default_transparency_level: u8,
    pub voice_transparency: bool,
}

/// A selectable value with a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice<T: 'static> {
    pub value: T,
    pub label: &'static str,
}

/// Everything a front-end needs to know to build the controls for a model.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceProfile {
    pub name: &'static str,
    pub vendor_ids: &'static [u16],
    pub anc: Option<AncCapabilities>,
    pub toggles: &'static [Toggle],
    pub eq_presets: &'static [EqPreset],
    /// Supports the 10-band user EQ via [`CommandId::MultiEq2`].
    pub custom_eq: bool,
    pub gestures: &'static [Gesture],
    pub key_actions: &'static [KeyAction],
    /// Fixed (non configurable) call gestures: `(gesture, action)`.
    pub call_controls: &'static [(&'static str, &'static str)],
    pub prompt_languages: &'static [Choice<&'static str>],
    pub prompt_volumes: &'static [Choice<u8>],
    pub auto_power_off: &'static [AutoPowerOff],
    pub wear_detection: bool,
    pub balance: bool,
    pub rename: bool,
    pub sleep_timer: bool,
    /// Settings to explicitly request after connecting (beyond the 0x1002 dump).
    pub requested_settings: &'static [CommandId],
}

impl DeviceProfile {
    /// A caution to show next to a toggle, if the firmware has side effects.
    pub fn toggle_warning(&self, toggle: Toggle) -> Option<&'static str> {
        match toggle {
            Toggle::Ldac => Some(
                "Enabling LDAC restarts the earbuds and disables dual device connection and spatial audio.",
            ),
            Toggle::DualDevice => Some("Enabling it restarts the earbuds and disables LDAC."),
            Toggle::SleepMode => Some(
                "Touch controls are disabled in sleep mode. Put the earbuds back in the case to exit.",
            ),
            _ => None,
        }
    }

    pub fn eq_preset(&self, id: u8) -> Option<&EqPreset> {
        self.eq_presets.iter().find(|p| p.id == id)
    }
}

/// All known model profiles.
pub const PROFILES: &[&DeviceProfile] = &[&MELOBUDS_PRO];

/// Looks up the profile for a model id, falling back to [`GENERIC`].
pub fn for_vendor(vendor_id: u16) -> &'static DeviceProfile {
    PROFILES
        .iter()
        .copied()
        .find(|p| p.vendor_ids.contains(&vendor_id))
        .unwrap_or(&GENERIC)
}

const GESTURES_SINGLE_DOUBLE_TRIPLE: &[Gesture] = &[
    Gesture::new(Side::Left, Tap::Single),
    Gesture::new(Side::Right, Tap::Single),
    Gesture::new(Side::Left, Tap::Double),
    Gesture::new(Side::Right, Tap::Double),
    Gesture::new(Side::Left, Tap::Triple),
    Gesture::new(Side::Right, Tap::Triple),
];

/// Bands shared by the MeloBuds Pro presets (freq, gain, q, filter type).
macro_rules! bands {
    ($( ($f:expr, $g:expr, $q:expr, $t:expr) ),* $(,)?) => {
        &[ $( EqBand::new($f, $g, $q, $t) ),* ]
    };
}

/// QCY MeloBuds Pro (HT07, WuQi chip), model id `0x4D4A`.
pub const MELOBUDS_PRO: DeviceProfile = DeviceProfile {
    name: "QCY MeloBuds Pro",
    vendor_ids: &[0x4D4A],
    anc: Some(AncCapabilities {
        scenes: &[
            AncScene::Adaptive,
            AncScene::Indoor,
            AncScene::Commuting,
            AncScene::Noisy,
            AncScene::AntiWind,
        ],
        transparency_levels: Some((1, 6)),
        default_transparency_level: 4,
        voice_transparency: true,
    }),
    toggles: &[
        Toggle::GameMode,
        Toggle::SleepMode,
        Toggle::Ldac,
        Toggle::DualDevice,
    ],
    eq_presets: &[
        EqPreset {
            id: 1,
            name: "Default",
            master_gain_db: 0.0,
            bands: bands![
                (30, 0.0, 0.7, 4),
                (220, -8.0, 1.0, 0),
                (900, -2.0, 1.5, 0),
                (2800, 4.0, 1.0, 0),
                (12000, 3.0, 0.8, 0),
                (5300, -3.0, 2.0, 0)
            ],
            warning: None,
        },
        EqPreset {
            id: 2,
            name: "Pop",
            master_gain_db: 0.0,
            bands: bands![
                (30, 0.0, 0.4, 4),
                (220, -8.0, 1.0, 0),
                (2800, 3.5, 1.0, 0),
                (12000, 3.0, 0.8, 0),
                (5300, -3.0, 2.0, 0),
                (60, -3.0, 0.5, 0)
            ],
            warning: None,
        },
        EqPreset {
            id: 3,
            name: "Heavy Bass",
            master_gain_db: 0.0,
            bands: bands![
                (30, 0.0, 0.7, 4),
                (220, -8.0, 1.0, 0),
                (2800, 1.0, 1.0, 0),
                (12000, 8.0, 0.8, 0),
                (5300, -5.0, 2.0, 0),
                (60, 5.0, 0.6, 0)
            ],
            warning: None,
        },
        EqPreset {
            id: 4,
            name: "Rock",
            master_gain_db: 0.0,
            bands: bands![
                (30, 0.0, 0.7, 4),
                (220, -10.0, 1.0, 0),
                (2800, 2.0, 1.0, 0),
                (12000, 3.0, 0.8, 0),
                (5300, -1.0, 2.0, 0),
                (60, 3.0, 0.6, 0)
            ],
            warning: None,
        },
        EqPreset {
            id: 5,
            name: "Soft",
            master_gain_db: 0.0,
            bands: bands![
                (30, 0.0, 0.4, 4),
                (220, -8.0, 1.0, 0),
                (2800, 1.0, 1.0, 0),
                (12000, 1.0, 0.8, 0),
                (5300, -3.0, 2.0, 0),
                (60, -2.0, 0.5, 0)
            ],
            warning: None,
        },
        EqPreset {
            id: 6,
            name: "Classic",
            master_gain_db: 0.0,
            bands: bands![
                (30, 0.0, 0.7, 4),
                (220, -5.0, 1.0, 0),
                (2800, 1.0, 1.0, 0),
                (12000, 1.0, 0.8, 0),
                (5300, -5.0, 2.0, 0),
                (60, 2.0, 0.6, 0)
            ],
            warning: None,
        },
        EqPreset {
            id: 10,
            name: "Spatial sound",
            master_gain_db: 0.0,
            bands: bands![
                (30, 0.0, 0.7, 4),
                (220, -8.0, 1.0, 0),
                (900, -2.0, 1.5, 0),
                (2800, 4.0, 1.0, 0),
                (12000, 3.0, 0.8, 0),
                (5300, -3.0, 2.0, 0)
            ],
            warning: Some(
                "Spatial audio is incompatible with LDAC; the earbuds restart to disable LDAC.",
            ),
        },
    ],
    custom_eq: true,
    gestures: GESTURES_SINGLE_DOUBLE_TRIPLE,
    key_actions: &KeyAction::KNOWN,
    call_controls: &[
        ("Double tap", "Answer / end call"),
        ("Long press 1.5 s", "Decline call"),
    ],
    prompt_languages: &[
        Choice {
            value: "WQ00",
            label: "English",
        },
        Choice {
            value: "WQ01",
            label: "Chinese",
        },
    ],
    prompt_volumes: &[
        Choice {
            value: 4,
            label: "Low",
        },
        Choice {
            value: 6,
            label: "Medium",
        },
        Choice {
            value: 8,
            label: "High",
        },
        Choice {
            value: 10,
            label: "Maximum",
        },
    ],
    auto_power_off: &AutoPowerOff::PRESETS,
    wear_detection: true,
    balance: true,
    rename: true,
    sleep_timer: true,
    requested_settings: &[
        CommandId::DeviceName,
        CommandId::MultiEq2,
        CommandId::PowerOffTimer,
        CommandId::MaxEqCount,
    ],
};

/// Fallback for unknown models: the subset of the app's default panel that is
/// safe on every QCY-protocol firmware.
pub const GENERIC: DeviceProfile = DeviceProfile {
    name: "QCY earbuds",
    vendor_ids: &[],
    anc: None,
    toggles: &[],
    eq_presets: &[],
    custom_eq: false,
    gestures: GESTURES_SINGLE_DOUBLE_TRIPLE,
    key_actions: &[
        KeyAction::None,
        KeyAction::PlayPause,
        KeyAction::PreviousTrack,
        KeyAction::NextTrack,
        KeyAction::VoiceAssistant,
        KeyAction::VolumeUp,
        KeyAction::VolumeDown,
        KeyAction::GameMode,
    ],
    call_controls: &[("Double tap", "Answer / hang up")],
    prompt_languages: &[],
    prompt_volumes: &[],
    auto_power_off: &[],
    wear_detection: false,
    balance: false,
    rename: false,
    sleep_timer: false,
    requested_settings: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup() {
        assert_eq!(for_vendor(19786).name, "QCY MeloBuds Pro");
        assert_eq!(for_vendor(1).name, GENERIC.name);
    }

    #[test]
    fn preset_payload_fits_in_a_frame() {
        for p in MELOBUDS_PRO.eq_presets {
            let eq = p.to_parametric().padded(20);
            assert!(eq.encode_v2().unwrap().len() + 4 <= 255);
        }
    }
}
