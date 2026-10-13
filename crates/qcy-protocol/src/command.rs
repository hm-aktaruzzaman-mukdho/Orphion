//! Typed requests and their wire encoding.

use crate::eq::{GraphicEq, ParametricEq};
use crate::frame;
use crate::panel::KeyTarget;
use crate::types::{AncMode, AutoPowerOff, KeyAction, KeyBinding, Toggle, WearDetection};
use crate::{Characteristic, CommandId, ProtocolError, Update};

/// Longest Bluetooth name the app lets you set (UTF-8 bytes).
pub const MAX_NAME_LEN: usize = 32;

/// Bytes to write to one characteristic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    pub characteristic: Characteristic,
    pub data: Vec<u8>,
}

/// A write described by a control panel, with how to track its effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelWrite {
    pub write: Write,
    /// The setting the write changes, read back afterwards.
    pub setting: Option<CommandId>,
    /// The value the setting is expected to read back as.
    pub value: Option<Vec<u8>>,
}

/// Everything a client can ask the earbuds to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Ask the earbuds to (re-)send the value of one setting on `0x1002`.
    Request(CommandId),
    /// Switch noise control mode.
    SetAnc(AncMode),
    /// Legacy noise mode (command `0x0C`, older models): the meaning of each
    /// value comes from the model's control panel.
    SetNoiseMode(u8),
    /// Flip one of the simple boolean settings.
    SetToggle(Toggle, bool),
    /// Left/right balance, 0 = full left, 50 = centre, 100 = full right.
    SetBalance(u8),
    /// Volume of voice prompts / beeps (the app offers 4, 6, 8, 10 out of 15).
    SetPromptVolume(u8),
    /// Voice prompt language code, e.g. `"WQ00"` (English) / `"WQ01"` (Chinese).
    SetPromptLanguage(String),
    /// Bluetooth name (≤ 32 UTF-8 bytes). The buds should both be out of the case.
    SetDeviceName(String),
    SetWearDetection(WearDetection),
    SetAutoPowerOff(AutoPowerOff),
    /// Sleep timer in minutes; 0 (or `u16::MAX`) turns it off.
    ///
    /// On the wire "off" is `0xFFFF`. A literal 0 is the app's "Shut down"
    /// button and powers the earbuds off at once, so it is never sent.
    SetPowerOffTimer(u16),
    /// Media volume for the left and right bud.
    SetVolume {
        left: u8,
        right: u8,
    },
    /// Parametric EQ with 7-byte bands (command `0x22`).
    SetParametricEq(ParametricEq),
    /// Parametric EQ with 6-byte bands (command `0x20`, older firmwares).
    SetParametricEqV1(ParametricEq),
    /// Legacy graphic EQ on characteristic `0x000B` (older models).
    SetGraphicEq(GraphicEq),
    /// Re-bind touch gestures: `(gesture id, action)` pairs.
    SetKeyFunctions(Vec<(u8, KeyAction)>),
    /// Binds an action byte to one gesture slot of a model's control panel.
    SetKey {
        target: KeyTarget,
        event: u16,
        action: u8,
    },
    /// Restore settings to defaults (keeps pairings).
    ResetDefaults,
    /// Forget every paired phone.
    ClearPairing,
    /// Factory reset.
    FactoryReset,
    /// Writes one setting: `FF len id n payload`. Used for settings that are
    /// only known from control panels (anti-wind noise, eco mode, …).
    SetValue {
        id: CommandId,
        payload: Vec<u8>,
    },
    /// Bytes taken verbatim from a model's control panel.
    Panel(PanelWrite),
    /// Escape hatch for experiments: write raw bytes.
    Raw(Write),
}

impl Command {
    /// Encodes the command into the bytes to write.
    pub fn encode(&self) -> Result<Write, ProtocolError> {
        use Characteristic as C;
        let cmd = |id: CommandId, payload: &[u8]| -> Result<Write, ProtocolError> {
            Ok(Write {
                characteristic: C::COMMAND,
                data: frame::encode_one(id, payload)?,
            })
        };

        match self {
            Command::Request(id) => cmd(CommandId::RequestData, &[u8::from(*id)]),
            Command::SetAnc(mode) => {
                let s = mode.to_setting();
                cmd(CommandId::Anc, &[s.mode, s.scene, s.level])
            }
            Command::SetNoiseMode(m) => cmd(CommandId::NoiseMode, &[*m]),
            Command::SetToggle(toggle, on) => cmd(toggle.command_id(), &[Toggle::encode(*on)]),
            Command::SetBalance(v) => {
                if *v > 100 {
                    return Err(ProtocolError::InvalidValue(format!(
                        "balance must be 0..=100, got {v}"
                    )));
                }
                cmd(CommandId::Balance, &[*v])
            }
            // `[level, 0]`, as the app sends it to every model except vendor
            // 0x4161 (which gets just `[level]`). The MeloBuds Pro ignores
            // the one-byte form.
            Command::SetPromptVolume(v) => cmd(CommandId::PromptVolume, &[*v, 0]),
            Command::SetPromptLanguage(code) => {
                if code.is_empty() || !code.is_ascii() {
                    return Err(ProtocolError::InvalidValue(format!(
                        "language code must be non-empty ASCII, got {code:?}"
                    )));
                }
                cmd(CommandId::PromptLanguage, code.as_bytes())
            }
            Command::SetDeviceName(name) => {
                let bytes = name.trim().as_bytes();
                if bytes.is_empty() || bytes.len() > MAX_NAME_LEN {
                    return Err(ProtocolError::InvalidValue(format!(
                        "name must be 1..={MAX_NAME_LEN} UTF-8 bytes"
                    )));
                }
                cmd(CommandId::DeviceName, bytes)
            }
            Command::SetWearDetection(w) => cmd(CommandId::WearDetection, &w.encode()),
            Command::SetAutoPowerOff(a) => cmd(CommandId::AutoPowerOff, &a.encode()),
            Command::SetPowerOffTimer(minutes) => {
                let minutes = if *minutes == 0 { u16::MAX } else { *minutes };
                let [lo, hi] = minutes.to_le_bytes();
                cmd(CommandId::PowerOffTimer, &[lo, hi, 0, 0])
            }
            Command::SetVolume { left, right } => cmd(CommandId::Volume, &[*left, *right, 0]),
            Command::SetParametricEq(eq) => cmd(CommandId::MultiEq2, &eq.encode_v2()?),
            Command::SetParametricEqV1(eq) => cmd(CommandId::MultiEq, &eq.encode_v1()?),
            Command::SetGraphicEq(eq) => Ok(Write {
                characteristic: C::EQ,
                data: eq.encode(),
            }),
            Command::SetKeyFunctions(pairs) => {
                if pairs.is_empty() {
                    return Err(ProtocolError::InvalidValue("no key bindings given".into()));
                }
                Ok(Write {
                    characteristic: C::KEYS,
                    data: pairs.iter().flat_map(|(g, a)| [*g, a.id()]).collect(),
                })
            }
            Command::SetKey {
                target,
                event,
                action,
            } => match target {
                KeyTarget::Table => Ok(Write {
                    characteristic: C::KEYS,
                    data: vec![
                        u8::try_from(*event).map_err(|_| {
                            ProtocolError::InvalidValue(format!(
                                "gesture {event} does not fit the key table"
                            ))
                        })?,
                        *action,
                    ],
                }),
                // The legacy long-press characteristics share their UUIDs
                // with the language and reset characteristics; on anything
                // but JieLi (handled by its dialect) a key byte there could
                // reset the earbuds.
                KeyTarget::Characteristic(c) if *c == C::LANGUAGE || *c == C::RESET => {
                    Err(ProtocolError::InvalidValue(format!(
                        "refusing to write a key function to {c:?}"
                    )))
                }
                KeyTarget::Characteristic(c) => Ok(Write {
                    characteristic: *c,
                    data: vec![*action],
                }),
            },
            Command::ResetDefaults => cmd(CommandId::ResetDefault, &[]),
            Command::ClearPairing => cmd(CommandId::ClearPairing, &[]),
            Command::FactoryReset => cmd(CommandId::FactoryReset, &[]),
            Command::SetValue { id, payload } => cmd(*id, payload),
            Command::Panel(p) => Ok(p.write.clone()),
            Command::Raw(w) => Ok(w.clone()),
        }
    }

    /// The state change this command should cause, used for optimistic UI
    /// updates until the earbuds confirm it.
    pub fn implied_update(&self) -> Option<Update> {
        Some(match self {
            Command::SetAnc(mode) => Update::Anc(*mode),
            Command::SetNoiseMode(m) => Update::Setting {
                id: CommandId::NoiseMode,
                payload: vec![*m],
            },
            Command::SetValue { id, payload } => Update::Setting {
                id: *id,
                payload: payload.clone(),
            },
            Command::Panel(PanelWrite {
                setting: Some(id),
                value: Some(v),
                ..
            }) => Update::Setting {
                id: *id,
                payload: v.clone(),
            },
            Command::SetToggle(t, on) => Update::Toggle(*t, *on),
            Command::SetBalance(v) => Update::Balance(*v),
            Command::SetPromptLanguage(code) => Update::PromptLanguage(code.clone()),
            Command::SetDeviceName(name) => Update::DeviceName(name.trim().to_string()),
            Command::SetWearDetection(w) => Update::WearDetection(*w),
            Command::SetAutoPowerOff(a) => Update::AutoPowerOff(*a),
            Command::SetParametricEq(eq) | Command::SetParametricEqV1(eq) => {
                Update::ParametricEq(eq.clone())
            }
            Command::SetKeyFunctions(pairs) => Update::KeyBindings(
                pairs
                    .iter()
                    .map(|&(gesture_id, action)| KeyBinding {
                        gesture_id: u16::from(gesture_id),
                        action,
                    })
                    .collect(),
            ),
            Command::SetKey { event, action, .. } => Update::KeyBindings(vec![KeyBinding {
                gesture_id: *event,
                action: KeyAction::from_id(*action),
            }]),
            _ => return None,
        })
    }

    /// The setting to ask for afterwards to confirm the change, if any.
    pub fn confirmation_request(&self) -> Option<CommandId> {
        Some(match self {
            Command::SetAnc(_) => CommandId::Anc,
            Command::SetNoiseMode(_) => CommandId::NoiseMode,
            Command::SetValue { id, .. } => *id,
            Command::Panel(PanelWrite {
                setting: Some(id), ..
            }) => *id,
            Command::SetToggle(t, _) => t.command_id(),
            Command::SetBalance(_) => CommandId::Balance,
            Command::SetPromptVolume(_) => CommandId::PromptVolume,
            Command::SetPromptLanguage(_) => CommandId::PromptLanguage,
            Command::SetDeviceName(_) => CommandId::DeviceName,
            Command::SetWearDetection(_) => CommandId::WearDetection,
            Command::SetAutoPowerOff(_) => CommandId::AutoPowerOff,
            Command::SetPowerOffTimer(_) => CommandId::PowerOffTimer,
            Command::SetVolume { .. } => CommandId::Volume,
            Command::SetParametricEq(_) => CommandId::MultiEq2,
            Command::SetParametricEqV1(_) => CommandId::MultiEq,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AncScene, Gesture, Side, Tap};

    fn bytes(c: Command) -> Vec<u8> {
        c.encode().unwrap().data
    }

    /// Every expected value below is copied from the control panel JSON or the
    /// decompiled app, so these tests pin us to the official behaviour.
    #[test]
    fn matches_official_app() {
        // Panel JSON "sub_items" commands.
        assert_eq!(
            bytes(Command::SetToggle(Toggle::GameMode, true)),
            [0xFF, 3, 0x09, 1, 1]
        );
        assert_eq!(
            bytes(Command::SetToggle(Toggle::SleepMode, false)),
            [0xFF, 3, 0x10, 1, 2]
        );
        assert_eq!(
            bytes(Command::SetToggle(Toggle::Ldac, true)),
            [0xFF, 3, 0x23, 1, 1]
        );
        assert_eq!(
            bytes(Command::SetToggle(Toggle::DualDevice, false)),
            [0xFF, 3, 0x24, 1, 2]
        );
        assert_eq!(bytes(Command::ResetDefaults), [0xFF, 2, 1, 0]);
        assert_eq!(bytes(Command::FactoryReset), [0xFF, 2, 3, 0]);
        // DataBean.getRequestDataCMD(34)
        assert_eq!(
            bytes(Command::Request(CommandId::MultiEq2)),
            [0xFF, 3, 0xFE, 1, 0x22]
        );
        // ANCSettingDataBean.getANCSettingCMD for 0x010102 (default ANC)
        assert_eq!(
            bytes(Command::SetAnc(AncMode::NoiseCancelling {
                scene: AncScene::Indoor,
                level: 2
            })),
            [0xFF, 5, 0x17, 3, 1, 1, 2]
        );
        // setNoiseMode special-cases 0x030100 to (3, 2, 0)
        assert_eq!(
            bytes(Command::SetAnc(AncMode::VoiceTransparency)),
            [0xFF, 5, 0x17, 3, 3, 2, 0]
        );
        assert_eq!(
            bytes(Command::SetAnc(AncMode::Off)),
            [0xFF, 5, 0x17, 3, 2, 0, 0]
        );
        // Panel "Disconnect and power off": key "0A00" = 10 minutes
        assert_eq!(
            bytes(Command::SetAutoPowerOff(AutoPowerOff::After {
                minutes: 10
            })),
            [0xFF, 4, 0x1F, 2, 0x0A, 0x00]
        );
        assert_eq!(
            bytes(Command::SetPromptLanguage("WQ00".into())),
            [0xFF, 6, 0x19, 4, b'W', b'Q', b'0', b'0']
        );
        assert_eq!(
            bytes(Command::SetPowerOffTimer(30)),
            [0xFF, 6, 0x14, 4, 30, 0, 0, 0]
        );
    }

    #[test]
    fn sleep_timer_off_is_never_shut_down() {
        // The app's "Not activated" entry sends 65535; minutes = 0 is its
        // "Shut down" button, which powers the earbuds off.
        let off = [0xFF, 6, 0x14, 4, 0xFF, 0xFF, 0, 0];
        assert_eq!(bytes(Command::SetPowerOffTimer(0)), off);
        assert_eq!(bytes(Command::SetPowerOffTimer(u16::MAX)), off);
    }

    #[test]
    fn prompt_volume_is_two_bytes() {
        // ToneVolumeManagerActivity: setBytesValue(mac, 29, {level, 0}).
        assert_eq!(bytes(Command::SetPromptVolume(6)), [0xFF, 4, 0x1D, 2, 6, 0]);
    }

    #[test]
    fn key_functions_go_to_their_own_characteristic() {
        let g = Gesture::new(Side::Right, Tap::Double).id();
        let w = Command::SetKeyFunctions(vec![(g, KeyAction::NextTrack)])
            .encode()
            .unwrap();
        assert_eq!(w.characteristic, Characteristic::KEYS);
        assert_eq!(w.data, [4, 3]);
    }

    #[test]
    fn validates_input() {
        assert!(Command::SetBalance(101).encode().is_err());
        assert!(Command::SetDeviceName("x".repeat(33)).encode().is_err());
        assert!(Command::SetDeviceName("   ".into()).encode().is_err());
    }
}
