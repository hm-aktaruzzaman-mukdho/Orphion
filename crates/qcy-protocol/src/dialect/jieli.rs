//! QCY's JieLi models (`research/notes/jieli.md`).
//!
//! Everything goes through RCSP on service `0xA002`, without authentication.
//! QCY's own TLV settings travel inside the custom opcode `0xFF` (one TLV per
//! frame, without the `FF len` header). The *old* models (`vendorId & 0x80F0
//! != 0x8070`) use JieLi "ADV" attributes instead for keys, game and sleep
//! mode, name, balance and battery. Both use JieLi system info for the
//! graphic EQ and an ADV attribute for in-ear detection.
//!
//! This module translates the writes the QCY dialect would make (as produced
//! by [`Command::encode`](crate::Command::encode)) into RCSP frames, and RCSP
//! frames back into [`Update`]s.

use super::rcsp::{self, Opcode, Reassembler};
use crate::frame::{self, Tlv};
use crate::types::{BatteryStatus, FirmwareVersions, KeyAction, KeyBinding, Version};
use crate::update::decode_tlvs;
use crate::{Characteristic, CommandId, ProtocolError, Update, Write};

/// ADV attribute types (`AttrAndFunCode.ADV_TYPE_*`).
mod adv {
    pub const BATTERY: u8 = 0;
    pub const NAME: u8 = 1;
    pub const KEYS: u8 = 2;
    pub const WORK_MODE: u8 = 5;
    pub const TIME: u8 = 7;
    pub const IN_EAR: u8 = 8;
    pub const LANGUAGE: u8 = 9;
    pub const SLEEP: u8 = 10;
    pub const NOISE: u8 = 11;
    pub const BALANCE: u8 = 12;
    /// Battery, work mode, in-ear, sleep and noise mode: QCY's battery poll.
    pub const POLL_MASK: u32 = 0x0D21;
}

/// The JieLi key function value for "no action" (QCY uses 0).
const JL_NO_ACTION: u8 = 127;
/// System info function for public attributes; attribute type 4 is the EQ.
const SYS_PUBLIC: u8 = 0xFF;
const SYS_EQ: u8 = 4;
/// Most bands the JieLi path sends.
pub const MAX_EQ_BANDS: usize = 10;

/// RCSP session state.
#[derive(Debug)]
pub struct JieLi {
    /// Whether the model uses custom commands for everything ("new" JieLi).
    pub new: bool,
    sn: u8,
    rx: Reassembler,
    /// `(min, max)` of the noise mode strength, from ADV attribute 11.
    noise_range: Option<(u16, u16)>,
}

fn write(data: Vec<u8>) -> Write {
    Write {
        characteristic: Characteristic::RCSP_WRITE,
        data,
    }
}

impl JieLi {
    pub fn new(new: bool) -> Self {
        Self {
            new,
            sn: 0,
            rx: Reassembler::default(),
            noise_range: None,
        }
    }

    fn frame(&mut self, op: Opcode, params: &[u8]) -> Result<Write, ProtocolError> {
        let data = rcsp::command(op, self.sn, params)?;
        self.sn = self.sn.wrapping_add(1);
        Ok(write(data))
    }

    fn custom(&mut self, tlv: &Tlv) -> Result<Write, ProtocolError> {
        let mut params = vec![tlv.id, tlv.payload.len() as u8];
        params.extend_from_slice(&tlv.payload);
        self.frame(Opcode::Custom, &params)
    }

    fn set_adv(&mut self, kind: u8, data: &[u8]) -> Result<Write, ProtocolError> {
        self.frame(Opcode::SetAdv, &rcsp::ltv_record(kind, data))
    }

    fn get_adv(&mut self, mask: u32) -> Result<Write, ProtocolError> {
        self.frame(Opcode::GetAdv, &mask.to_be_bytes())
    }

    /// Writes made right after connecting: device info, then the clock.
    pub fn connect(&mut self, unix_time: u32) -> Result<Vec<Write>, ProtocolError> {
        Ok(vec![
            self.frame(Opcode::TargetInfo, &[0xFF, 0xFF, 0xFF, 0xFF, 0x00])?,
            self.set_adv(adv::TIME, &unix_time.to_be_bytes())?,
        ])
    }

    /// Asks for a QCY setting.
    pub fn request(&mut self, id: CommandId) -> Result<Write, ProtocolError> {
        if !self.new {
            if let Some(mask) = old_request_mask(id) {
                return self.get_adv(mask);
            }
            if id == CommandId::Version {
                return self.frame(Opcode::TargetInfo, &[0, 0, 0, 1 << 5, 0]);
            }
        }
        self.custom(&Tlv::new(CommandId::RequestData, vec![id.into()]))
    }

    /// Reads the whole state of an old model in one go.
    pub fn request_all_adv(&mut self) -> Result<Write, ProtocolError> {
        self.get_adv(u32::MAX)
    }

    /// Asks for the in-ear detection switch.
    pub fn request_in_ear(&mut self) -> Result<Write, ProtocolError> {
        self.get_adv(1 << adv::IN_EAR)
    }

    /// Asks for the graphic EQ.
    pub fn request_graphic_eq(&mut self) -> Result<Write, ProtocolError> {
        self.frame(Opcode::GetSysInfo, &[SYS_PUBLIC, 0, 0, 0, 1 << SYS_EQ])
    }

    /// The periodic battery (and status) poll.
    pub fn poll(&mut self) -> Result<Write, ProtocolError> {
        if self.new {
            self.custom(&Tlv::new(
                CommandId::RequestData,
                vec![CommandId::Battery.into()],
            ))
        } else {
            self.get_adv(adv::POLL_MASK)
        }
    }

    /// Sets the noise mode of a "percent" panel through ADV attribute 11.
    pub fn set_noise_percent(&mut self, mode: u8, percent: u8) -> Result<Write, ProtocolError> {
        let data = match self.noise_range {
            Some((min, max)) => {
                let span = u32::from(max.saturating_sub(min));
                let cur = min + (span * u32::from(percent.min(100)) / 100) as u16;
                let mut d = vec![mode, 6];
                for v in [min, max, cur] {
                    d.extend_from_slice(&v.to_be_bytes());
                }
                d
            }
            // Strength unknown yet: the app sends a bare mode like this.
            None => vec![mode, 0, 0, 0, 0, 0, 0, 0],
        };
        self.set_adv(adv::NOISE, &data)
    }

    /// Translates a write of the QCY dialect.
    pub fn translate(&mut self, w: &Write) -> Result<Vec<Write>, ProtocolError> {
        let c = w.characteristic;
        let d = w.data.as_slice();
        Ok(match c {
            Characteristic::COMMAND => {
                let mut out = Vec::new();
                for tlv in frame::decode(d)? {
                    out.push(self.translate_tlv(&tlv)?);
                }
                out
            }
            Characteristic::KEYS => {
                let pairs: Vec<(u16, u8)> =
                    d.chunks_exact(2).map(|p| (u16::from(p[0]), p[1])).collect();
                self.keys(&pairs)?
            }
            // Per-gesture key characteristics of "type 1" panels (including
            // the long-press ones 0x0009/0x000A, which are keys here).
            Characteristic {
                service: crate::uuids::Service::Control,
                id: 1..=6 | 9 | 10,
            } if d.len() == 1 => self.keys(&[(c.id, d[0])])?,
            Characteristic::FUNCTION => vec![self.set_adv(adv::IN_EAR, &d[..1.min(d.len())])?],
            // The legacy reset characteristic: 1 restore settings, 2 clear
            // pairing, 3 factory reset (`QCYConnectManager.sendBleData`).
            Characteristic::RESET => {
                let id = match d {
                    [1] => CommandId::ResetDefault,
                    [2] => CommandId::ClearPairing,
                    [3] => CommandId::FactoryReset,
                    _ => {
                        return Err(ProtocolError::InvalidValue(format!(
                            "unknown reset operation {d:02x?}"
                        )));
                    }
                };
                vec![self.translate_tlv(&Tlv::new(id, Vec::new()))?]
            }
            Characteristic::EQ => {
                let (&mode, gains) = d.split_first().ok_or(ProtocolError::TooShort {
                    expected: 1,
                    actual: 0,
                })?;
                let mut params = vec![SYS_PUBLIC];
                let mut eq = vec![mode];
                if mode & 0x80 != 0 {
                    eq.extend(gains.iter().take(10));
                    eq.resize(11, 0);
                } else {
                    // Presets are selected by id with every gain set to 127.
                    eq.extend([127u8; 10]);
                }
                params.extend(rcsp::ltv_record(SYS_EQ, &eq));
                vec![self.frame(Opcode::SetSysInfo, &params)?]
            }
            other => {
                return Err(ProtocolError::InvalidValue(format!(
                    "{other:?} has no equivalent on JieLi earbuds"
                )));
            }
        })
    }

    fn translate_tlv(&mut self, tlv: &Tlv) -> Result<Write, ProtocolError> {
        let id = CommandId::from(tlv.id);
        let p = tlv.payload.as_slice();
        if id == CommandId::RequestData && p.len() == 1 {
            return self.request(CommandId::from(p[0]));
        }
        if !self.new {
            match (id, p) {
                (CommandId::SleepMode, [v]) => return self.set_adv(adv::SLEEP, &[*v]),
                // Work mode: 2 = game, 1 = normal.
                (CommandId::GameMode, [v]) => {
                    return self.set_adv(adv::WORK_MODE, &[if *v == 1 { 2 } else { 1 }]);
                }
                (CommandId::DeviceName, name) => return self.set_adv(adv::NAME, name),
                (CommandId::Balance, [v]) => return self.set_adv(adv::BALANCE, &[*v]),
                (CommandId::ResetDefault, []) => return self.frame(Opcode::SystemOp, &[1]),
                (CommandId::ClearPairing, []) => return self.frame(Opcode::SystemOp, &[2]),
                _ => {}
            }
        }
        let tlv = match id {
            // At most ten bands, no padding.
            CommandId::MultiEq | CommandId::MultiEq2 => {
                let band = if id == CommandId::MultiEq2 { 7 } else { 6 };
                let mut payload = p.to_vec();
                let bands = payload.len().saturating_sub(3) / band;
                let used = (0..bands)
                    .take_while(|&i| {
                        let at = 3 + i * band;
                        payload[at] != 0 || payload[at + 1] != 0
                    })
                    .count()
                    .min(MAX_EQ_BANDS);
                payload.truncate(3 + used * band);
                Tlv::new(id, payload)
            }
            _ => tlv.clone(),
        };
        self.custom(&tlv)
    }

    /// Key writes: `(gesture id, action)` pairs in QCY numbering.
    pub fn keys(&mut self, pairs: &[(u16, u8)]) -> Result<Vec<Write>, ProtocolError> {
        if self.new {
            pairs
                .iter()
                .map(|&(g, a)| self.custom(&Tlv::new(CommandId::KeyFunctions, vec![g as u8, a])))
                .collect()
        } else {
            let mut data = Vec::new();
            for &(g, a) in pairs {
                let (key, clicks) = jl_key(g);
                data.extend([key, clicks, if a == 0 { JL_NO_ACTION } else { a }]);
            }
            Ok(vec![self.set_adv(adv::KEYS, &data)?])
        }
    }

    /// Decodes notification data from `0xA002`.
    pub fn decode(&mut self, data: &[u8]) -> Vec<Update> {
        let mut out = Vec::new();
        for f in self.rx.push(data) {
            if f.status != 0 {
                continue;
            }
            match f.op() {
                Some(Opcode::Custom) => out.extend(self.custom_payload(&f.params)),
                Some(Opcode::TargetInfo) => out.extend(target_info(&f.params)),
                Some(Opcode::GetAdv) if !f.is_command => out.extend(self.adv(&f.params)),
                Some(Opcode::GetSysInfo | Opcode::SysInfoChanged) => {
                    out.extend(sys_info(&f.params));
                }
                _ => {}
            }
        }
        out
    }

    fn custom_payload(&self, p: &[u8]) -> Vec<Update> {
        let [id, len, ..] = *p else {
            return Vec::new();
        };
        let Some(payload) = p.get(2..2 + usize::from(len)) else {
            return Vec::new();
        };
        if id == u8::from(CommandId::Version) && payload.len() >= 3 {
            // One version for both buds.
            let v = Version(payload[0], payload[1], payload[2]);
            return vec![Update::Firmware(FirmwareVersions {
                left: Some(v),
                right: Some(v),
            })];
        }
        decode_tlvs(vec![Tlv::new(id, payload.to_vec())])
    }

    fn adv(&mut self, params: &[u8]) -> Vec<Update> {
        let mut out = Vec::new();
        for (kind, d) in rcsp::ltv(params) {
            match (kind, d) {
                (adv::BATTERY, _) if !d.is_empty() => {
                    let mut b = [0u8; 3];
                    for (i, v) in d.iter().take(3).enumerate() {
                        b[i] = *v;
                    }
                    if let Ok(status) = BatteryStatus::parse(&b) {
                        out.push(Update::Battery(status));
                    }
                }
                (adv::NAME, name) => {
                    out.push(Update::DeviceName(
                        String::from_utf8_lossy(name).into_owned(),
                    ));
                }
                (adv::KEYS, triplets) => {
                    let bindings = triplets
                        .chunks_exact(3)
                        .map(|t| KeyBinding {
                            gesture_id: qcy_gesture(t[0], t[1]),
                            action: KeyAction::from_id(if t[2] == JL_NO_ACTION { 0 } else { t[2] }),
                        })
                        .collect();
                    out.push(Update::KeyBindings(bindings));
                }
                (adv::WORK_MODE, [m, ..]) => {
                    out.push(setting(CommandId::GameMode, if *m == 2 { 1 } else { 2 }));
                }
                (adv::IN_EAR, [v, ..]) => out.push(Update::Characteristic {
                    characteristic: Characteristic::FUNCTION,
                    data: vec![*v],
                }),
                (adv::LANGUAGE, code) => {
                    out.push(Update::PromptLanguage(
                        String::from_utf8_lossy(code).into_owned(),
                    ));
                }
                (adv::SLEEP, [v, ..]) => out.push(setting(CommandId::SleepMode, *v)),
                (adv::BALANCE, [v, ..]) => {
                    out.push(setting(CommandId::Balance, *v));
                    out.push(Update::Balance(*v));
                }
                (adv::NOISE, [mode, _len, a, b, c, d2, e, f, ..]) => {
                    let (min, max, cur) = (
                        u16::from_be_bytes([*a, *b]),
                        u16::from_be_bytes([*c, *d2]),
                        u16::from_be_bytes([*e, *f]),
                    );
                    self.noise_range = Some((min, max));
                    let percent = if max > min {
                        (u32::from(cur.saturating_sub(min)) * 100 / u32::from(max - min)) as u8
                    } else {
                        0
                    };
                    let scene = u8::from(*mode != 2);
                    out.push(setting(CommandId::Anc, *mode));
                    out.push(Update::Anc(crate::types::AncMode::from_setting(
                        crate::types::AncSetting::new(*mode, scene, percent),
                    )));
                }
                _ => {}
            }
        }
        out
    }
}

fn setting(id: CommandId, v: u8) -> Update {
    Update::Setting {
        id,
        payload: vec![v],
    }
}

/// Settings an old model reports through ADV attributes: the mask to ask.
fn old_request_mask(id: CommandId) -> Option<u32> {
    Some(match id {
        CommandId::SleepMode => 1 << adv::SLEEP,
        CommandId::GameMode => 1 << adv::WORK_MODE,
        CommandId::Balance => 1 << adv::BALANCE,
        CommandId::DeviceName => 1 << adv::NAME,
        CommandId::KeyFunctions => 1 << adv::KEYS,
        CommandId::Battery => adv::POLL_MASK,
        _ => return None,
    })
}

/// QCY gesture id → JieLi `(key, clicks)`: odd ids are the left bud (key 1),
/// even ids the right one (key 2); clicks 1-3 single-triple, 4 quadruple,
/// 5 long press. Headphone keys are `key << 8 | clicks` already.
fn jl_key(g: u16) -> (u8, u8) {
    if g > 0xFF {
        return ((g >> 8) as u8, g as u8);
    }
    let g = g as u8;
    let key = if g % 2 == 1 { 1 } else { 2 };
    (key, g.div_ceil(2))
}

/// Inverse of [`jl_key`].
fn qcy_gesture(key: u8, clicks: u8) -> u16 {
    match key {
        1 => u16::from(clicks) * 2 - 1,
        2 => u16::from(clicks) * 2,
        _ => u16::from(key) << 8 | u16::from(clicks),
    }
}

/// Target info (`0x03`): the firmware version of old models (attribute 5).
fn target_info(params: &[u8]) -> Vec<Update> {
    let mut out = Vec::new();
    for (kind, d) in rcsp::ltv(params) {
        match (kind, d) {
            (5, [hi, lo, ..]) => {
                let v = Version(hi >> 4, hi & 0x0F, (lo >> 4) * 10 + (lo & 0x0F));
                out.push(Update::Firmware(FirmwareVersions {
                    left: Some(v),
                    right: Some(v),
                }));
            }
            (19, [flags, ..]) => {
                out.push(setting(
                    CommandId::GameMode,
                    if flags & 2 != 0 { 1 } else { 2 },
                ));
            }
            _ => {}
        }
    }
    out
}

/// System info: the graphic EQ `[mode, g0..g9]`.
fn sys_info(params: &[u8]) -> Vec<Update> {
    let Some((&function, rest)) = params.split_first() else {
        return Vec::new();
    };
    if function != SYS_PUBLIC {
        return Vec::new();
    }
    rcsp::ltv(rest)
        .into_iter()
        .filter(|(kind, d)| *kind == SYS_EQ && !d.is_empty())
        .map(|(_, d)| {
            Update::GraphicEq(crate::eq::GraphicEq {
                preset: d[0],
                gains: d[1..].iter().map(|&g| g as i8).collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use crate::types::{AncMode, AncScene};

    fn frames(jl: &mut JieLi, c: Command) -> Vec<Vec<u8>> {
        jl.translate(&c.encode().unwrap())
            .unwrap()
            .into_iter()
            .map(|w| w.data)
            .collect()
    }

    #[test]
    fn qcy_settings_use_the_custom_command() {
        let mut jl = JieLi::new(true);
        jl.sn = 3;
        // jieli.md §2.5: ANC mode 1 scene 1 level 2, sn 3.
        let anc = Command::SetAnc(AncMode::NoiseCancelling {
            scene: AncScene::Indoor,
            level: 2,
        });
        assert_eq!(
            frames(&mut jl, anc),
            [vec![
                0xFE, 0xDC, 0xBA, 0xC0, 0xFF, 0x00, 0x06, 0x03, 0x17, 0x03, 0x01, 0x01, 0x02, 0xEF
            ]]
        );
        // A read request becomes `FE 01 id`.
        let req = frames(&mut jl, Command::Request(CommandId::Battery));
        assert_eq!(&req[0][7..11], [4, 0xFE, 0x01, 0x2F]);
    }

    #[test]
    fn old_models_use_adv_attributes() {
        let mut jl = JieLi::new(false);
        // Left double tap → next track (jieli.md §6 example), sn 0.
        let w = jl.translate(&Write {
            characteristic: Characteristic::legacy_key(3),
            data: vec![3],
        });
        assert_eq!(
            w.unwrap()[0].data,
            [
                0xFE, 0xDC, 0xBA, 0xC0, 0xC0, 0x00, 0x06, 0x00, 0x04, 0x02, 0x01, 0x02, 0x03, 0xEF
            ]
        );
        // Game mode on → work mode 2.
        let f = frames(
            &mut jl,
            Command::SetValue {
                id: CommandId::GameMode,
                payload: vec![1],
            },
        );
        assert_eq!(&f[0][8..11], [2, adv::WORK_MODE, 2]);
        // Factory reset is never an RCSP system operation.
        let f = frames(&mut jl, Command::FactoryReset);
        assert_eq!(f[0][4], 0xFF);
    }

    #[test]
    fn decodes_custom_responses_and_adv() {
        let mut jl = JieLi::new(false);
        let battery = [
            0xFE, 0xDC, 0xBA, 0x00, 0xFF, 0x00, 0x07, 0x00, 0x02, 0x2F, 0x03, 0x5A, 0x5C, 0x64,
            0xEF,
        ];
        let u = jl.decode(&battery);
        assert!(
            u.iter()
                .any(|u| matches!(u, Update::Battery(b) if b.left.unwrap().percent == 90))
        );

        // ADV dump: keys (left single = play/pause, right double = none) and sleep on.
        let params = [7, 2, 1, 1, 1, 2, 2, 127, 2, 10, 1];
        let mut f = vec![
            0xFE,
            0xDC,
            0xBA,
            0x00,
            0xC1,
            0x00,
            (params.len() + 2) as u8,
            0x00,
            0x05,
        ];
        f.extend(params);
        f.push(0xEF);
        let u = jl.decode(&f);
        let Some(Update::KeyBindings(k)) = u.iter().find(|u| matches!(u, Update::KeyBindings(_)))
        else {
            panic!("{u:?}")
        };
        assert_eq!((k[0].gesture_id, k[0].action), (1, KeyAction::PlayPause));
        assert_eq!((k[1].gesture_id, k[1].action), (4, KeyAction::None));
        assert!(u.contains(&setting(CommandId::SleepMode, 1)));
    }

    #[test]
    fn graphic_eq_goes_through_system_info() {
        let mut jl = JieLi::new(true);
        let w = jl
            .translate(&Write {
                characteristic: Characteristic::EQ,
                data: vec![2, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            })
            .unwrap();
        // Presets: FF 0C 04 mode + ten times 127.
        assert_eq!(&w[0].data[8..12], [0xFF, 0x0C, 0x04, 0x02]);
        assert!(w[0].data[12..22].iter().all(|&g| g == 127));
    }

    #[test]
    fn gesture_mapping() {
        for g in 1..=10 {
            let (k, c) = jl_key(g);
            assert_eq!(qcy_gesture(k, c), g);
        }
        assert_eq!(jl_key(0x0702), (7, 2));
    }
}
