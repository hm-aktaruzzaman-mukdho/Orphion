//! Decoding of data sent by the earbuds.

use crate::eq::{GraphicEq, ParametricEq};
use crate::frame::{self, Tlv};
use crate::types::{
    AncMode, AncSetting, AutoPowerOff, BatteryStatus, FirmwareVersions, KeyBinding, PowerOffTimer,
    PromptVolume, Toggle, Volume, WearDetection,
};
use crate::{Characteristic, CommandId, ProtocolError};

/// A piece of state reported by the earbuds.
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    Battery(BatteryStatus),
    Firmware(FirmwareVersions),
    Anc(AncMode),
    Toggle(Toggle, bool),
    Volume(Volume),
    Balance(u8),
    PromptVolume(PromptVolume),
    PromptLanguage(String),
    DeviceName(String),
    WearDetection(WearDetection),
    AutoPowerOff(AutoPowerOff),
    PowerOffTimer(PowerOffTimer),
    /// Playback active (`true`) or paused, from [`CommandId::MusicAction`].
    Playing(bool),
    KeyBindings(Vec<KeyBinding>),
    GraphicEq(GraphicEq),
    ParametricEq(ParametricEq),
    /// How many parametric bands the firmware accepts.
    MaxEqBands(u8),
    /// A setting this crate does not decode (yet); kept for logging / debugging.
    Unknown {
        id: CommandId,
        payload: Vec<u8>,
    },
}

/// Decodes the value of any characteristic (read result or notification).
///
/// Malformed TLV entries inside an otherwise valid frame are returned as
/// [`Update::Unknown`] instead of failing the whole frame.
pub fn decode(characteristic: Characteristic, data: &[u8]) -> Result<Vec<Update>, ProtocolError> {
    Ok(match characteristic {
        Characteristic::BATTERY => vec![Update::Battery(BatteryStatus::parse(data)?)],
        Characteristic::VERSION => vec![Update::Firmware(FirmwareVersions::parse(data))],
        Characteristic::EQ => vec![Update::GraphicEq(GraphicEq::parse(data)?)],
        Characteristic::KEYS => vec![Update::KeyBindings(KeyBinding::parse_table(data))],
        Characteristic::SETTINGS | Characteristic::COMMAND => frame::decode(data)?
            .into_iter()
            .map(|tlv| decode_entry(&tlv).unwrap_or_else(|_| unknown(&tlv)))
            .collect(),
        // Dialect specific characteristics are decoded elsewhere.
        _ => Vec::new(),
    })
}

fn unknown(tlv: &Tlv) -> Update {
    Update::Unknown {
        id: tlv.id.into(),
        payload: tlv.payload.clone(),
    }
}

/// Decodes one TLV entry of a settings frame
/// (port of `DataBean.getDataBean` for the settings this crate supports).
pub fn decode_entry(tlv: &Tlv) -> Result<Update, ProtocolError> {
    let p = tlv.payload.as_slice();
    let need = |n: usize| {
        if p.len() < n {
            Err(ProtocolError::TooShort {
                expected: n,
                actual: p.len(),
            })
        } else {
            Ok(())
        }
    };
    let id = CommandId::from(tlv.id);

    if let Some(toggle) = Toggle::from_command_id(id) {
        need(1)?;
        return Ok(Update::Toggle(toggle, p[0] == 1));
    }

    Ok(match id {
        CommandId::Anc => {
            need(3)?;
            Update::Anc(AncMode::from_setting(AncSetting::new(p[0], p[1], p[2])))
        }
        CommandId::Volume => {
            need(3)?;
            Update::Volume(Volume {
                left: p[0],
                right: p[1],
                max: p[2],
            })
        }
        CommandId::Balance => {
            need(1)?;
            Update::Balance(p[0])
        }
        CommandId::PromptVolume => {
            need(1)?;
            Update::PromptVolume(PromptVolume {
                level: p[0],
                max: p.get(1).copied().unwrap_or(100),
            })
        }
        CommandId::PromptLanguage => {
            Update::PromptLanguage(String::from_utf8_lossy(p).into_owned())
        }
        CommandId::DeviceName => {
            let end = p.iter().position(|&b| b == 0).unwrap_or(p.len());
            Update::DeviceName(String::from_utf8_lossy(&p[..end]).into_owned())
        }
        CommandId::WearDetection => Update::WearDetection(WearDetection::parse(p)?),
        CommandId::AutoPowerOff => Update::AutoPowerOff(AutoPowerOff::parse(p)?),
        CommandId::PowerOffTimer => Update::PowerOffTimer(PowerOffTimer::parse(p)?),
        CommandId::MusicAction => {
            need(1)?;
            Update::Playing(p[0] == 1)
        }
        CommandId::KeyFunctions => Update::KeyBindings(KeyBinding::parse_table(p)),
        CommandId::MultiEq2 => Update::ParametricEq(ParametricEq::parse_v2(p)?),
        CommandId::MultiEq => Update::ParametricEq(ParametricEq::parse_v1(p)?),
        CommandId::MaxEqCount => {
            // A `SingleDataBean` in the app: 1 or 2 bytes LE, never above 255 in practice.
            need(1)?;
            Update::MaxEqBands(p[0])
        }
        CommandId::Battery => Update::Battery(BatteryStatus::parse(p)?),
        CommandId::Version => Update::Firmware(FirmwareVersions::parse(p)),
        _ => unknown(tlv),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AncScene, WearMusicBehavior};

    #[test]
    fn decodes_real_settings_dump() {
        // Characteristic 0x1002 of a QCY MeloBuds Pro, fw 2.0.6.
        let raw = [
            0xff, 0x30, 0x2c, 0x04, 0x00, 0x01, 0x01, 0x01, 0x08, 0x03, 0x46, 0x46, 0x0f, 0x09,
            0x01, 0x02, 0x17, 0x03, 0x01, 0x05, 0x00, 0x10, 0x01, 0x00, 0x16, 0x01, 0x32, 0x04,
            0x01, 0x02, 0x23, 0x01, 0x00, 0x24, 0x01, 0x01, 0x1f, 0x02, 0x05, 0x00, 0x1d, 0x02,
            0x08, 0x0f, 0x19, 0x04, 0x57, 0x51, 0x30, 0x30,
        ];
        let updates = decode(Characteristic::SETTINGS, &raw).unwrap();
        let expected = vec![
            Update::WearDetection(WearDetection {
                enabled: false,
                music: WearMusicBehavior::PauseAndResume,
                anc_follows_wear: true,
                tone: Some(true),
            }),
            Update::Volume(Volume {
                left: 70,
                right: 70,
                max: 15,
            }),
            Update::Toggle(Toggle::GameMode, false),
            Update::Anc(AncMode::NoiseCancelling {
                scene: AncScene::Adaptive,
                level: 0,
            }),
            Update::Toggle(Toggle::SleepMode, false),
            Update::Balance(50),
            Update::Playing(false),
            Update::Toggle(Toggle::Ldac, false),
            Update::Toggle(Toggle::DualDevice, true),
            Update::AutoPowerOff(AutoPowerOff::After { minutes: 5 }),
            Update::PromptVolume(PromptVolume { level: 8, max: 15 }),
            Update::PromptLanguage("WQ00".into()),
        ];
        assert_eq!(updates, expected);
    }

    #[test]
    fn malformed_entry_becomes_unknown() {
        // ANC entry with a 1-byte payload is invalid; must not poison the frame.
        let updates = decode(Characteristic::SETTINGS, &[0xFF, 3, 0x17, 1, 9]).unwrap();
        assert!(matches!(updates[0], Update::Unknown { .. }));
    }
}
