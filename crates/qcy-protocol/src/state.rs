//! An aggregate view of everything known about a pair of earbuds.

use std::collections::{BTreeMap, HashMap};

use crate::eq::{GraphicEq, ParametricEq};
use crate::types::{
    AncMode, AutoPowerOff, BatteryStatus, FirmwareVersions, KeyBinding, PowerOffTimer,
    PromptVolume, Toggle, Volume, WearDetection,
};
use crate::{Characteristic, CommandId, Update};

/// Latest known value of every setting. `None` means "not reported yet".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceState {
    pub battery: Option<BatteryStatus>,
    pub firmware: Option<FirmwareVersions>,
    pub anc: Option<AncMode>,
    pub toggles: HashMap<Toggle, bool>,
    pub volume: Option<Volume>,
    pub balance: Option<u8>,
    pub prompt_volume: Option<PromptVolume>,
    pub prompt_language: Option<String>,
    pub device_name: Option<String>,
    pub wear_detection: Option<WearDetection>,
    pub auto_power_off: Option<AutoPowerOff>,
    pub power_off_timer: Option<PowerOffTimer>,
    pub playing: Option<bool>,
    pub key_bindings: Vec<KeyBinding>,
    pub graphic_eq: Option<GraphicEq>,
    pub parametric_eq: Option<ParametricEq>,
    pub max_eq_bands: Option<u8>,
    /// Raw payloads of settings without a decoder, keyed by id.
    pub unknown: BTreeMap<u8, Vec<u8>>,
    /// Raw payload of every setting reported, keyed by id.
    pub settings: BTreeMap<u8, Vec<u8>>,
    /// Last value of characteristics without a dedicated decoder.
    pub characteristics: BTreeMap<Characteristic, Vec<u8>>,
}

impl DeviceState {
    /// Folds one update into the state.
    pub fn apply(&mut self, update: Update) {
        match update {
            Update::Battery(b) => self.battery = Some(b),
            Update::Firmware(f) => self.firmware = Some(f),
            Update::Anc(a) => self.anc = Some(a),
            Update::Toggle(t, on) => {
                self.toggles.insert(t, on);
            }
            Update::Volume(v) => self.volume = Some(v),
            Update::Balance(b) => self.balance = Some(b),
            Update::PromptVolume(p) => self.prompt_volume = Some(p),
            Update::PromptLanguage(l) => self.prompt_language = Some(l),
            Update::DeviceName(n) => self.device_name = Some(n),
            Update::WearDetection(w) => self.wear_detection = Some(w),
            Update::AutoPowerOff(a) => self.auto_power_off = Some(a),
            Update::PowerOffTimer(t) => self.power_off_timer = Some(t),
            Update::Playing(p) => self.playing = Some(p),
            Update::KeyBindings(new) => {
                // Partial tables (single-binding notifications) are merged.
                for binding in new {
                    match self
                        .key_bindings
                        .iter_mut()
                        .find(|b| b.gesture_id == binding.gesture_id)
                    {
                        Some(existing) => *existing = binding,
                        None => self.key_bindings.push(binding),
                    }
                }
                self.key_bindings.sort_by_key(|b| b.gesture_id);
            }
            Update::GraphicEq(e) => self.graphic_eq = Some(e),
            Update::ParametricEq(e) => self.parametric_eq = Some(e),
            Update::MaxEqBands(n) => self.max_eq_bands = Some(n),
            Update::Unknown { id, payload } => {
                self.unknown.insert(u8::from(id), payload);
            }
            Update::Setting { id, payload } => {
                self.settings.insert(u8::from(id), payload);
            }
            Update::Characteristic {
                characteristic,
                data,
            } => {
                self.characteristics.insert(characteristic, data);
            }
        }
    }

    /// Raw payload of a setting, if reported.
    pub fn setting(&self, id: CommandId) -> Option<&[u8]> {
        self.settings.get(&u8::from(id)).map(Vec::as_slice)
    }

    /// Whether a setting reads back as "on" (first byte 1, like the app).
    pub fn setting_on(&self, id: CommandId) -> Option<bool> {
        self.setting(id).map(|p| p.first() == Some(&1))
    }

    pub fn toggle(&self, t: Toggle) -> Option<bool> {
        self.toggles.get(&t).copied()
    }

    /// Id of the active EQ preset, preferring the parametric report.
    pub fn active_eq_preset(&self) -> Option<u8> {
        self.parametric_eq
            .as_ref()
            .map(|e| e.index)
            .or_else(|| self.graphic_eq.as_ref().map(|e| e.preset))
    }

    /// Whether a raw, undecoded setting was reported.
    pub fn has_unknown(&self, id: CommandId) -> bool {
        self.unknown.contains_key(&u8::from(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::KeyAction;

    #[test]
    fn merges_key_bindings() {
        let mut s = DeviceState::default();
        s.apply(Update::KeyBindings(KeyBinding::parse_table(&[
            1, 0, 2, 0, 3, 1,
        ])));
        s.apply(Update::KeyBindings(KeyBinding::parse_table(&[2, 3])));
        assert_eq!(s.key_bindings.len(), 3);
        assert_eq!(s.key_bindings[1].action, KeyAction::NextTrack);
    }
}
