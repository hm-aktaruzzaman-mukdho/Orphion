//! Touch and button function layouts (panel types 10, 14 and 1).
//!
//! Every configurable gesture is a *slot* with an event id and a list of
//! `(action byte, label)` choices. Two write paths exist:
//!
//! * the key table, characteristic `0x000D`: `[event id, action]` (types 10
//!   and 14; a partial table is accepted);
//! * one characteristic per gesture (type 1, "V1"): `0x0001`..`0x0006`
//!   (left/right single, double, triple), `0x0009`/`0x000A` (long press),
//!   written with the action byte alone.
//!
//! Action labels are model specific (10 and 11 both mean "ANC" on different
//! models), so they always come from the panel.

use serde_json::Value;

use super::json::{array, as_int, hex, int, string};
use crate::Characteristic;
use crate::types::Side;
use crate::uuids;

/// Where a slot's action is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyTarget {
    /// The key table `0x000D`: `[event id, action]`.
    Table,
    /// A per-gesture characteristic, written with the action byte.
    Characteristic(Characteristic),
}

/// One configurable gesture of one bud (or one button).
#[derive(Debug, Clone, PartialEq)]
pub struct KeySlot {
    /// `None` for headphone buttons (layout 14).
    pub side: Option<Side>,
    /// Gesture id: 1/2 single, 3/4 double, 5/6 triple, 9/10 long press, …
    pub event: u16,
    pub target: KeyTarget,
    /// Factory default action, if the panel names one.
    pub default: Option<u8>,
    /// `(action byte, label)` in panel order.
    pub choices: Vec<(u8, String)>,
}

impl KeySlot {
    pub fn label_of(&self, action: u8) -> Option<&str> {
        self.choices
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, l)| l.as_str())
    }
}

/// A gesture ("Double tap") with its left/right slots.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyRow {
    pub name: String,
    pub slots: Vec<KeySlot>,
}

/// A titled group of rows: "Music mode", or one headphone button.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyGroup {
    pub name: String,
    pub rows: Vec<KeyRow>,
}

/// A fixed, non-configurable gesture shown for information ("Call mode").
#[derive(Debug, Clone, PartialEq)]
pub struct FixedGesture {
    pub gesture: String,
    pub left: String,
    pub right: String,
}

/// A key function section.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyControl {
    pub title: String,
    pub groups: Vec<KeyGroup>,
    pub fixed: Vec<FixedGesture>,
}

impl KeyControl {
    pub fn slots(&self) -> impl Iterator<Item = &KeySlot> {
        self.groups
            .iter()
            .flat_map(|g| &g.rows)
            .flat_map(|r| &r.slots)
    }

    pub fn slot(&self, event: u16) -> Option<&KeySlot> {
        self.slots().find(|s| s.event == event)
    }

    fn is_empty(&self) -> bool {
        self.slots().next().is_none()
    }
}

/// Choices of a V2 event (`cmd` is a decimal number, coerced).
fn choices_v2(event: &Value) -> Vec<(u8, String)> {
    array(event, "list")
        .iter()
        .filter_map(|c| {
            let action = u8::try_from(int(c, "cmd")?).ok()?;
            Some((
                action,
                string(c, "name").unwrap_or_else(|| action.to_string()),
            ))
        })
        .collect()
}

fn slot_v2(event: &Value, side: Option<Side>) -> Option<KeySlot> {
    let id = u16::try_from(int(event, "eventID")?).ok()?;
    let choices = choices_v2(event);
    if choices.is_empty() {
        return None;
    }
    let default = int(event, "defaultFun").and_then(|d| u8::try_from(d).ok());
    Some(KeySlot {
        side,
        event: id,
        target: KeyTarget::Table,
        default,
        choices,
    })
}

/// Layout 10: `music.event[]` rows with `left`/`right` slots, plus the
/// read-only call gestures in `voice`.
pub(super) fn parse_touch(layout: &Value) -> Option<KeyControl> {
    let mut groups = Vec::new();
    if let Some(music) = layout.get("music") {
        let rows: Vec<KeyRow> = array(music, "event")
            .iter()
            .filter_map(|e| {
                let slots: Vec<KeySlot> = [("left", Side::Left), ("right", Side::Right)]
                    .into_iter()
                    .filter_map(|(k, side)| slot_v2(e.get(k)?, Some(side)))
                    .collect();
                (!slots.is_empty()).then(|| KeyRow {
                    name: string(e, "name").unwrap_or_default(),
                    slots,
                })
            })
            .collect();
        groups.push(KeyGroup {
            name: string(music, "name").unwrap_or_else(|| "Music".into()),
            rows,
        });
    }
    let fixed = layout
        .get("voice")
        .map(|voice| {
            array(voice, "event")
                .iter()
                .map(|e| FixedGesture {
                    gesture: string(e, "name").unwrap_or_default(),
                    left: string(e, "left").unwrap_or_default(),
                    right: string(e, "right").unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    let control = KeyControl {
        title: string(layout, "name").unwrap_or_else(|| "Touch controls".into()),
        groups,
        fixed,
    };
    (!control.is_empty()).then_some(control)
}

/// Layout 14: headphone buttons, each with its own gestures.
pub(super) fn parse_buttons(layout: &Value) -> Option<KeyControl> {
    let groups: Vec<KeyGroup> = array(layout, "buttons")
        .iter()
        .map(|b| KeyGroup {
            name: string(b, "keyName").unwrap_or_default(),
            rows: array(b, "events")
                .iter()
                .filter_map(|e| {
                    Some(KeyRow {
                        name: string(e, "name").unwrap_or_default(),
                        slots: vec![slot_v2(e, None)?],
                    })
                })
                .collect(),
        })
        .filter(|g| !g.rows.is_empty())
        .collect();
    let control = KeyControl {
        title: string(layout, "name").unwrap_or_else(|| "Buttons".into()),
        groups,
        fixed: Vec::new(),
    };
    (!control.is_empty()).then_some(control)
}

/// Layout 1: one characteristic per gesture, action bytes as hex strings.
pub(super) fn parse_legacy(layout: &Value) -> Option<KeyControl> {
    const GESTURES: [(&str, &str); 4] = [
        ("single_click", "Single tap"),
        ("double_click", "Double tap"),
        ("three_click", "Triple tap"),
        ("long_press", "Long press"),
    ];
    let rows: Vec<KeyRow> = GESTURES
        .iter()
        .filter_map(|&(key, fallback)| {
            let mut name = None;
            let slots: Vec<KeySlot> = [("left", Side::Left), ("right", Side::Right)]
                .into_iter()
                .filter_map(|(side_key, side)| {
                    let g = layout.get(side_key)?.get(key)?;
                    name.get_or_insert_with(|| string(g, "name").unwrap_or(fallback.into()));
                    let id = uuids::shorten(&string(g, "character")?)?;
                    let choices: Vec<(u8, String)> = array(g, "list")
                        .iter()
                        .filter_map(|c| {
                            let action = match c.get("cmd")? {
                                Value::String(s) => *hex(s)?.first()?,
                                other => u8::try_from(as_int(other)?).ok()?,
                            };
                            Some((action, string(c, "name").unwrap_or_default()))
                        })
                        .collect();
                    // `music` names the default entry.
                    let default = string(g, "music")
                        .and_then(|m| choices.iter().find(|(_, l)| *l == m).map(|(a, _)| *a));
                    (!choices.is_empty()).then(|| KeySlot {
                        side: Some(side),
                        event: id,
                        target: KeyTarget::Characteristic(Characteristic::control(id)),
                        default,
                        choices,
                    })
                })
                .collect();
            (!slots.is_empty()).then(|| KeyRow {
                name: name.unwrap_or_else(|| fallback.into()),
                slots,
            })
        })
        .collect();
    let control = KeyControl {
        title: string(layout, "name").unwrap_or_else(|| "Touch controls".into()),
        groups: vec![KeyGroup {
            name: String::new(),
            rows,
        }],
        fixed: Vec::new(),
    };
    (!control.is_empty()).then_some(control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn touch_layout() {
        let k = parse_touch(&json!({"name": "Keys", "music": {"name": "Music mode", "event": [
            {"name": "Touch", "left": {"eventID": 1, "defaultFun": 0, "list": [{"name": "None", "cmd": 0}, {"name": "ANC", "cmd": "11"}]},
                              "right": {"eventID": 2, "list": [{"name": "None", "cmd": 0}]}}]},
            "voice": {"event": [{"name": "Double touch", "left": "Answer", "right": "Answer"}]}}))
        .unwrap();
        let slot = k.slot(1).unwrap();
        assert_eq!(slot.choices[1], (11, "ANC".to_string()));
        assert_eq!(slot.target, KeyTarget::Table);
        assert_eq!(k.fixed[0].left, "Answer");
    }

    #[test]
    fn legacy_layout() {
        let k = parse_legacy(&json!({"left": {"single_click": {"name": "Touch",
            "character": "00000001-0000-1000-8000-00805f9b34fb", "music": "Play/pause",
            "list": [{"name": "Not work", "cmd": "00"}, {"name": "Play/pause", "cmd": "01"}, {"name": "ANC", "cmd": "0B"}]}}}))
        .unwrap();
        let slot = k.slot(1).unwrap();
        assert_eq!(
            slot.target,
            KeyTarget::Characteristic(Characteristic::legacy_key(1))
        );
        assert_eq!(slot.default, Some(1));
        assert_eq!(slot.label_of(11), Some("ANC"));
    }
}
