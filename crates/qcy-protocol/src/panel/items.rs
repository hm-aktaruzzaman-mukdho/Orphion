//! Settings rows: the type-4 item list and the single-value layouts
//! (101, 102, 103, 104, 106, 110, 111, 112, 113, 115, 1000, 1001, 20).
//!
//! Switches and buttons carry the exact bytes the app writes, so they work
//! for settings this crate has no name for (anti-wind noise, adaptive
//! volume, eco mode, …). A switch is "on" when its setting reads back as 1.

use serde_json::Value;

use super::json::{array, boolean, hex, int, string};
use crate::uuids::{self, Service};
use crate::{Characteristic, CommandId, Write};

/// A row of the settings list.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub title: String,
    pub description: Option<String>,
    /// The app shows it on the secondary "More" page.
    pub more: bool,
    pub kind: ItemKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Switch(Switch),
    /// A one-shot command (reset, clear pairing, …), confirmed first.
    Action(Action),
    /// A choice of byte strings written to one setting (layouts 1000, 111).
    Choice(Choice),
    /// Bluetooth name (command `0x18`), at most `max_len` UTF-8 bytes.
    Rename {
        max_len: usize,
    },
    /// Sleep timer (command `0x14`).
    SleepTimer,
    /// Prompt tone volume (command `0x1D`): `(level, label)` choices.
    PromptVolume {
        levels: Vec<(u8, String)>,
    },
    /// Voice prompt language (command `0x19`): `(code, label)`.
    Language {
        languages: Vec<(String, String)>,
    },
    /// Voice prompt language on characteristic `0x0009` (older models):
    /// `(code byte, label)`.
    LegacyLanguage {
        languages: Vec<(u8, String)>,
    },
    /// Wear detection (command `0x2C`, sensitivity `0x48`).
    WearDetection(WearDetection),
    /// Ear tip fit test (command `0x11`).
    FitTest,
    /// Game mode with optional game EQ sync (command `0x4A`, reboots).
    GameConfig {
        sync_label: Option<String>,
    },
    /// Multipoint (command `0x24`), with the list of paired phones.
    Multipoint,
    /// AI scene noise cancelling on/off (command `0x54`).
    AiAnc,
    /// Bluetooth vs. "focus" (SD card) playback (command `0x39`).
    FocusMode,
    /// Media volume of both buds (command `0x08`).
    Volume,
    /// Head gestures for calls: enabled with command `0x50`, actions
    /// through command `0x51`.
    HeadGestures(super::KeyControl),
}

/// An on/off setting.
#[derive(Debug, Clone, PartialEq)]
pub struct Switch {
    /// Setting whose value 1 means "on"; `None` when the state lives in a
    /// characteristic instead (see [`Switch::state_characteristic`]).
    pub setting: Option<CommandId>,
    pub on: Write,
    pub off: Write,
    /// Confirmation text shown before changing it.
    pub warning: Option<String>,
}

impl Switch {
    /// The command that turns the switch on or off.
    pub fn command(&self, on: bool) -> crate::Command {
        let write = if on { &self.on } else { &self.off };
        // The value the setting reads back as: the payload of the frame.
        let value = match crate::frame::decode(&write.data).as_deref() {
            Ok([tlv]) if write.characteristic == Characteristic::COMMAND => {
                Some(tlv.payload.clone())
            }
            _ => None,
        };
        crate::Command::Panel(crate::PanelWrite {
            write: write.clone(),
            setting: self.setting,
            value,
        })
    }

    /// Whether the switch is on in `state`, if known.
    pub fn is_on(&self, state: &crate::state::DeviceState) -> Option<bool> {
        if let Some((c, index)) = self.state_characteristic()
            && let Some(&b) = state.characteristics.get(&c).and_then(|d| d.get(index))
            && b != 0xFF
        {
            return Some(b == 1);
        }
        state.setting_on(self.setting?)
    }

    /// For older models the in-ear and monitor switches are a byte of the
    /// characteristic `0x000F` (0 = in-ear, 1 = monitor).
    pub fn state_characteristic(&self) -> Option<(Characteristic, usize)> {
        (self.on.characteristic == Characteristic::FUNCTION).then(|| {
            let index = match self.setting {
                Some(CommandId::Monitor) => 1,
                _ => 0,
            };
            (Characteristic::FUNCTION, index)
        })
    }
}

/// What an [`Action`] does, for the confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    ResetDefaults,
    ClearPairing,
    FactoryReset,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub kind: ActionKind,
    pub write: Write,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub setting: CommandId,
    /// `(payload, label)`; the payload is written as `FF len id n payload`.
    pub options: Vec<(Vec<u8>, String)>,
}

impl Choice {
    /// The option matching a value read back (the app compares the low byte
    /// first, then both bytes).
    pub fn selected(&self, value: &[u8]) -> Option<usize> {
        let first = value.first()?;
        self.options
            .iter()
            .position(|(p, _)| p.as_slice() == value)
            .or_else(|| self.options.iter().position(|(p, _)| p == &[*first]))
            .or_else(|| {
                self.options
                    .iter()
                    .position(|(p, _)| p.len() <= value.len() && value.starts_with(p))
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WearDetection {
    /// `(id, label)` of the music behaviours ("pause and resume", …).
    pub music: Vec<(u8, String)>,
    pub music_title: Option<String>,
    /// Label of the "ANC follows wear" switch, if offered.
    pub anc_title: Option<String>,
    pub anc_description: Option<String>,
    /// `(level, label)` of the sensor sensitivity (command `0x48`).
    pub sensitivity: Vec<(u8, String)>,
}

/// Normalises a panel `cmd` for a characteristic: raw TLVs (`"230101"`, found
/// on JieLi panels) get the `FF len` frame the command characteristic needs.
fn panel_write(character: Characteristic, bytes: Vec<u8>) -> Write {
    let framed = bytes.first() == Some(&0xFF) && bytes.get(1) == Some(&((bytes.len() - 2) as u8));
    let data = if character == Characteristic::COMMAND && !framed {
        let mut out = vec![0xFF, bytes.len() as u8];
        out.extend(bytes);
        out
    } else {
        bytes
    };
    Write {
        characteristic: character,
        data,
    }
}

/// A frame `FF 03 id 01 v`.
fn single(id: CommandId, v: u8) -> Write {
    Write {
        characteristic: Characteristic::COMMAND,
        data: vec![0xFF, 3, id.into(), 1, v],
    }
}

/// The characteristic of an item; panels default to `0x1001`. Anything that
/// is not a characteristic of the control service is rejected.
fn character(v: &Value) -> Option<Characteristic> {
    match string(v, "character") {
        None => Some(Characteristic::COMMAND),
        Some(uuid) => uuids::shorten(&uuid).map(|id| Characteristic::new(Service::Control, id)),
    }
}

fn setting(v: &Value, key: &str) -> Option<CommandId> {
    int(v, key)
        .and_then(|id| u8::try_from(id).ok())
        .map(CommandId::from)
}

fn title(v: &Value) -> String {
    string(v, "title")
        .or_else(|| string(v, "name"))
        .unwrap_or_default()
}

/// `jump: 1` puts a row on the "More" page.
fn more(v: &Value) -> bool {
    int(v, "jump") == Some(1)
}

/// Parses one entry of a type-4 list. `None` for rows the desktop app does
/// not offer (firmware update, malformed entries).
pub(super) fn parse_type4_item(v: &Value) -> Option<Item> {
    let item_type = int(v, "type").unwrap_or(0);
    let c = character(v)?;
    let base = |kind| Item {
        title: title(v),
        description: string(v, "msg"),
        more: more(v),
        kind,
    };
    match item_type {
        // Firmware update: never offered (project rule).
        1 => None,
        200..=299 => {
            let subs = array(v, "sub_items");
            let pick = |state: bool| {
                subs.iter()
                    .find(|s| boolean(s, "boolVal") == state)
                    .and_then(|s| hex(&string(s, "cmd")?))
            };
            let mut id = setting(v, "cmdid").or(match item_type {
                201 => Some(CommandId::InEar),
                202 => Some(CommandId::Monitor),
                203 => Some(CommandId::SleepMode),
                _ if c == Characteristic::FUNCTION => {
                    // The app infers in-ear (byte 0) or monitor (byte 1).
                    let on = pick(true).unwrap_or_default();
                    if on.first() == Some(&1) {
                        Some(CommandId::InEar)
                    } else {
                        Some(CommandId::Monitor)
                    }
                }
                _ => None,
            });
            let (on, off) = if item_type == 206 {
                // Panels give type 206 "Gaming mode" the sleep mode bytes
                // (FF0310…); the official app really toggles sleep mode on
                // non-JieLi models. Build the gaming mode frame instead.
                id = Some(CommandId::GameMode);
                (
                    single(CommandId::GameMode, 1),
                    single(CommandId::GameMode, 2),
                )
            } else {
                (panel_write(c, pick(true)?), panel_write(c, pick(false)?))
            };
            Some(Item {
                description: string(v, "msg"),
                ..base(ItemKind::Switch(Switch {
                    setting: id,
                    on,
                    off,
                    warning: string(v, "alert"),
                }))
            })
        }
        300..=399 => Some(Item {
            more: false,
            ..base(ItemKind::FitTest)
        }),
        2 if c == Characteristic::LANGUAGE => {
            let languages: Vec<(u8, String)> = array(v, "sub_items")
                .iter()
                .filter_map(|s| {
                    let code = *hex(&string(s, "cmd")?)?.first()?;
                    Some((code, string(s, "name").unwrap_or_default()))
                })
                .collect();
            (!languages.is_empty()).then(|| Item {
                description: None,
                ..base(ItemKind::LegacyLanguage { languages })
            })
        }
        _ => {
            let bytes = hex(&string(v, "cmd")?)?;
            let write = panel_write(c, bytes);
            let kind = match (c, write.data.as_slice()) {
                (Characteristic::COMMAND, [0xFF, 2, 1, 0]) => ActionKind::ResetDefaults,
                (Characteristic::COMMAND, [0xFF, 2, 2, 0]) => ActionKind::ClearPairing,
                (Characteristic::COMMAND, [0xFF, 2, 3, 0]) => ActionKind::FactoryReset,
                (Characteristic::RESET | Characteristic::ZR_SETTING, [1]) => {
                    ActionKind::ResetDefaults
                }
                (Characteristic::RESET | Characteristic::ZR_SETTING, [2]) => {
                    ActionKind::ClearPairing
                }
                (Characteristic::RESET, [3]) => ActionKind::FactoryReset,
                _ => ActionKind::Other,
            };
            Some(base(ItemKind::Action(Action {
                kind,
                write,
                message: string(v, "msg"),
            })))
        }
    }
}

/// `itemName` / `subItem` of layouts 1000 and 103: `key → label`, in order.
fn key_names(v: &Value) -> Vec<(String, String)> {
    let sub = array(v, "subItem");
    if !sub.is_empty() {
        return sub
            .iter()
            .filter_map(|s| Some((string(s, "key")?, string(s, "name").unwrap_or_default())))
            .collect();
    }
    match v.get("itemName") {
        Some(Value::Object(map)) => map
            .iter()
            .filter_map(|(k, name)| Some((k.clone(), name.as_str()?.to_string())))
            .collect(),
        _ => Vec::new(),
    }
}

/// Parses a single-value layout (one row). `None` for unsupported layouts.
pub(super) fn parse_layout(layout_type: i64, v: &Value) -> Option<Item> {
    let item = |title: String, kind| Item {
        title,
        description: string(v, "desc"),
        more: more(v),
        kind,
    };
    match layout_type {
        101 => Some(Item {
            more: true,
            ..item(
                string(v, "title").unwrap_or_else(|| "Bluetooth name".into()),
                ItemKind::Rename {
                    max_len: int(v, "maxLength")
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|&n| n > 0)
                        .unwrap_or(crate::command::MAX_NAME_LEN),
                },
            )
        }),
        102 => Some(item(title(v).if_empty("Sleep timer"), ItemKind::SleepTimer)),
        103 => {
            let mut levels: Vec<(u8, String)> = key_names(v)
                .into_iter()
                .filter_map(|(k, l)| Some((k.trim().parse().ok()?, l)))
                .collect();
            levels.sort_by_key(|(l, _)| *l);
            Some(item(
                title(v).if_empty("Prompt volume"),
                ItemKind::PromptVolume { levels },
            ))
        }
        104 => {
            let pairs = |list: &[Value], id_key: &str| -> Vec<(u8, String)> {
                list.iter()
                    .filter_map(|s| {
                        Some((
                            u8::try_from(int(s, id_key)?).ok()?,
                            string(s, "name").unwrap_or_default(),
                        ))
                    })
                    .collect()
            };
            let sensitivity = v
                .get("sensitivity")
                .map(|s| pairs(array(s, "items"), "cid"))
                .unwrap_or_default();
            Some(Item {
                description: string(v, "msg"),
                ..item(
                    title(v).if_empty("Wear detection"),
                    ItemKind::WearDetection(WearDetection {
                        music: pairs(array(v, "subItem"), "cid"),
                        music_title: string(v, "subTitle"),
                        anc_title: string(v, "valueTitle"),
                        anc_description: string(v, "ancMsg"),
                        sensitivity,
                    }),
                )
            })
        }
        106 => Some(item(title(v).if_empty("Focus mode"), ItemKind::FocusMode)),
        110 => Some(item(title(v).if_empty("Fit test"), ItemKind::FitTest)),
        114 => super::keys::parse_head(v).map(|keys| {
            item(
                title(v).if_empty("Head gestures"),
                ItemKind::HeadGestures(keys),
            )
        }),
        111 | 107 | 116 => {
            let id = setting(v, "cmdid")?;
            let options: Vec<(Vec<u8>, String)> = array(v, "subItem")
                .iter()
                .filter_map(|s| {
                    let index = u8::try_from(int(s, "index")?).ok()?;
                    Some((vec![index], string(s, "name").unwrap_or_default()))
                })
                .collect();
            (!options.is_empty()).then(|| {
                item(
                    title(v).if_empty("Sound mode"),
                    ItemKind::Choice(Choice {
                        setting: id,
                        options,
                    }),
                )
            })
        }
        112 => Some(item(
            title(v).if_empty("Game mode"),
            ItemKind::GameConfig {
                sync_label: string(v, "confTip"),
            },
        )),
        113 => Some(item(
            title(v).if_empty("Dual device connection"),
            ItemKind::Multipoint,
        )),
        115 => Some(item(
            title(v).if_empty("AI noise cancelling"),
            ItemKind::AiAnc,
        )),
        1000 => {
            let id = setting(v, "cmdid")?;
            let options: Vec<(Vec<u8>, String)> = key_names(v)
                .into_iter()
                .filter_map(|(k, l)| Some((hex(&k)?, l)))
                .collect();
            (!options.is_empty()).then(|| {
                item(
                    title(v),
                    ItemKind::Choice(Choice {
                        setting: id,
                        options,
                    }),
                )
            })
        }
        1001 => {
            let keys: Vec<String> = array(v, "itemKey")
                .iter()
                .filter_map(|k| {
                    let bytes = hex(k.as_str()?)?;
                    String::from_utf8(bytes).ok()
                })
                .collect();
            let names = array(v, "itemName");
            let languages: Vec<(String, String)> = keys
                .into_iter()
                .zip(names.iter().filter_map(Value::as_str))
                .map(|(k, n)| (k, n.to_string()))
                .collect();
            (!languages.is_empty()).then(|| {
                item(
                    title(v).if_empty("Voice prompt language"),
                    ItemKind::Language { languages },
                )
            })
        }
        20 => Some(item(
            string(v, "title").unwrap_or_else(|| "Earbud volume".into()),
            ItemKind::Volume,
        )),
        _ => None,
    }
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn switches() {
        let it = parse_type4_item(&json!({"type": 200, "cmdid": 36, "title": "Dual",
            "sub_items": [{"cmd": "240101", "boolVal": true}, {"cmd": "240100", "boolVal": false}]}))
        .unwrap();
        let ItemKind::Switch(s) = it.kind else {
            panic!()
        };
        // Raw JieLi TLVs get the frame.
        assert_eq!(s.on.data, [0xFF, 3, 0x24, 1, 1]);
        assert_eq!(s.off.data, [0xFF, 3, 0x24, 1, 0]);
        assert_eq!(s.setting, Some(CommandId::DualDevice));

        // The 206 panel bug: sleep bytes for gaming mode.
        let it = parse_type4_item(&json!({"type": 206, "cmdid": 9, "title": "Gaming mode",
            "sub_items": [{"cmd": "FF03100101", "boolVal": true}, {"cmd": "FF03100102", "boolVal": false}]}))
        .unwrap();
        let ItemKind::Switch(s) = it.kind else {
            panic!()
        };
        assert_eq!(s.on.data, [0xFF, 3, 0x09, 1, 1]);
    }

    #[test]
    fn firmware_update_is_skipped() {
        assert!(parse_type4_item(&json!({"type": 1, "title": "Firmware update"})).is_none());
    }

    #[test]
    fn actions() {
        let it =
            parse_type4_item(&json!({"type": 100, "title": "Reset", "cmd": "FF020300"})).unwrap();
        assert!(matches!(
            it.kind,
            ItemKind::Action(Action {
                kind: ActionKind::FactoryReset,
                ..
            })
        ));
        let it = parse_type4_item(&json!({"type": 100, "title": "Clear",
            "character": "0000000A-0000-1000-8000-00805f9b34fb", "cmd": "02"}))
        .unwrap();
        assert!(matches!(
            it.kind,
            ItemKind::Action(Action {
                kind: ActionKind::ClearPairing,
                ..
            })
        ));
    }

    #[test]
    fn value_lists() {
        let it = parse_layout(
            1000,
            &json!({"cmdid": 31, "title": "Auto off",
            "itemName": {"0500": "5 min", "FFFF": "Never"}}),
        )
        .unwrap();
        let ItemKind::Choice(c) = it.kind else {
            panic!()
        };
        assert_eq!(c.selected(&[0xFF, 0xFF]), Some(1));
        assert_eq!(c.selected(&[5, 0]), Some(0));

        let it = parse_layout(
            1001,
            &json!({"cmdid": 25, "itemKey": ["57513030", "57513031"],
            "itemName": ["English", "Chinese"]}),
        )
        .unwrap();
        assert_eq!(
            it.kind,
            ItemKind::Language {
                languages: vec![
                    ("WQ00".into(), "English".into()),
                    ("WQ01".into(), "Chinese".into())
                ]
            }
        );
    }
}
