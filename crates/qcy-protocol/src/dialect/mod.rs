//! The three ways QCY earbuds are driven, behind one [`Codec`].
//!
//! | dialect | chips | transport |
//! |---|---|---|
//! | [`Dialect::Qcy`] | WuQi, Realtek, most others | TLV frames on `0x1001`/`0x1002`, plus single-purpose characteristics of `0xA001` |
//! | [`Dialect::JieLi`] | JieLi | RCSP frames on `0xA002` ([`rcsp`]); QCY TLVs inside its custom command |
//! | [`Dialect::Zr`] | ZR | like QCY, with its own EQ blob, key order and status characteristic ([`zr`]) |
//!
//! Front-ends keep speaking [`Command`] and reading [`DeviceState`]; the codec
//! turns commands into GATT [`Op`]s for the connected model and incoming
//! bytes into [`Update`]s. It also knows what to read after connecting and
//! what to poll. It does no I/O itself.

pub mod jieli;
pub mod rcsp;
pub mod zr;

use std::time::Duration;

use crate::advert::ChipFamily;
use crate::eq::GraphicEq;
use crate::panel::{EqKind, ItemKind, KeyTarget, Panel};
use crate::state::DeviceState;
use crate::types::{AncMode, KeyAction, KeyBinding};
use crate::{Characteristic, Command, CommandId, ProtocolError, Update, Write, update};

use jieli::JieLi;

/// Most parametric bands sent when the firmware does not say (`MAXEQ_COUNT`).
const DEFAULT_MAX_EQ_BANDS: usize = 20;

/// How a model is driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Qcy,
    /// `new`: settings through custom commands rather than ADV attributes.
    JieLi {
        new: bool,
    },
    Zr,
}

impl Dialect {
    /// The dialect of a model id, as the app decides it.
    pub fn for_vendor(vendor_id: u16) -> Self {
        match ChipFamily::from_vendor_id(vendor_id) {
            ChipFamily::JieLi => Dialect::JieLi {
                new: vendor_id & 0x80F0 == 0x8070,
            },
            ChipFamily::Zr => Dialect::Zr,
            _ => Dialect::Qcy,
        }
    }
}

/// One GATT operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Write(Write),
    Read(Characteristic),
}

/// What the codec needs to know about the link and the device right now.
pub struct Context<'a> {
    pub state: &'a DeviceState,
    /// Largest value of one write.
    pub max_write_len: usize,
    /// Seconds since the Unix epoch (clock sync).
    pub unix_time: u32,
}

/// Models that cannot notify noise control changes; the app polls them
/// (`Devicebind.checkCanNotify`).
const POLL_ANC_MODELS: &[std::ops::RangeInclusive<u16>] = &[
    6513..=6517,
    16753..=16758,
    16760..=16760,
    16762..=16762,
    16767..=16767,
    18289..=18295,
    18545..=18545,
    18801..=18815,
    19057..=19070,
    19313..=19313,
    19569..=19576,
    19825..=19826,
    22897..=22897,
    23153..=23159,
];

/// Translates between [`Command`]/[`Update`] and the bytes of one model.
#[derive(Debug)]
pub struct Codec {
    dialect: Dialect,
    vendor_id: u16,
    jieli: Option<JieLi>,
    /// ZR: the clock is sent once per session before the first EQ write.
    zr_time_sent: bool,
    /// The panel's noise control is the "percent" kind (layout 11).
    percent_anc: bool,
    eq_kind: Option<EqKind>,
}

impl Codec {
    pub fn new(vendor_id: u16, panel: &Panel) -> Self {
        let dialect = Dialect::for_vendor(vendor_id);
        Self {
            dialect,
            vendor_id,
            jieli: match dialect {
                Dialect::JieLi { new } => Some(JieLi::new(new)),
                _ => None,
            },
            zr_time_sent: false,
            percent_anc: matches!(panel.anc, Some(crate::panel::AncControl::Percent(_))),
            eq_kind: panel.eq.as_ref().map(|e| e.kind),
        }
    }

    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Writes made once right after connecting.
    pub fn connect(&mut self, unix_time: u32) -> Result<Vec<Op>, ProtocolError> {
        Ok(match &mut self.jieli {
            Some(jl) => jl.connect(unix_time)?.into_iter().map(Op::Write).collect(),
            None => Vec::new(),
        })
    }

    /// Everything to read to (re)load the state of the model.
    /// `has` tells which characteristics the device exposes.
    pub fn refresh(
        &mut self,
        panel: &Panel,
        has: impl Fn(Characteristic) -> bool,
    ) -> Result<Vec<Op>, ProtocolError> {
        let mut requests: Vec<CommandId> = panel.read_settings.clone();
        if panel.balance.is_some() {
            requests.push(CommandId::Balance);
        }
        match self.eq_kind {
            Some(EqKind::ParametricV1) => requests.push(CommandId::MultiEq),
            Some(EqKind::ParametricV2) => {
                requests.extend([CommandId::MultiEq2, CommandId::MaxEqCount])
            }
            _ => {}
        }
        let mut ops = Vec::new();
        match self.dialect {
            Dialect::JieLi { new } => {
                let jl = self.jieli.as_mut().expect("JieLi codec");
                if new {
                    requests.splice(
                        0..0,
                        [
                            CommandId::Battery,
                            CommandId::Version,
                            CommandId::KeyFunctions,
                            CommandId::SleepMode,
                            CommandId::GameMode,
                            CommandId::DeviceName,
                        ],
                    );
                } else {
                    ops.push(Op::Write(jl.request_all_adv()?));
                    requests.retain(|id| {
                        !matches!(
                            id,
                            CommandId::SleepMode
                                | CommandId::GameMode
                                | CommandId::Balance
                                | CommandId::DeviceName
                        )
                    });
                }
                ops.push(Op::Write(jl.request_in_ear()?));
                if self.eq_kind == Some(EqKind::Graphic) {
                    ops.push(Op::Write(jl.request_graphic_eq()?));
                }
                requests.retain(|id| *id != CommandId::MaxEqCount);
                dedup(&mut requests);
                for id in requests {
                    ops.push(Op::Write(jl.request(id)?));
                }
            }
            Dialect::Qcy | Dialect::Zr => {
                let zr = self.dialect == Dialect::Zr;
                let mut reads = vec![
                    Characteristic::VERSION,
                    Characteristic::BATTERY,
                    Characteristic::SETTINGS,
                    Characteristic::KEYS,
                    Characteristic::EQ,
                ];
                if zr {
                    reads.push(Characteristic::ZR_SETTING);
                } else {
                    reads.push(Characteristic::FUNCTION);
                    if let Some(keys) = panel.keys_for(&has) {
                        for slot in keys.slots() {
                            // Long-press slots (0x0009/0x000A) are never read,
                            // like in the app.
                            if let KeyTarget::Characteristic(c) = slot.target
                                && (1..=6).contains(&c.id)
                            {
                                reads.push(c);
                            }
                        }
                    }
                    if panel
                        .items
                        .iter()
                        .any(|i| matches!(i.kind, ItemKind::LegacyLanguage { .. }))
                    {
                        reads.push(Characteristic::LANGUAGE);
                    }
                }
                ops.extend(reads.into_iter().filter(|c| has(*c)).map(Op::Read));
                if has(Characteristic::COMMAND) {
                    dedup(&mut requests);
                    for id in requests {
                        ops.push(Op::Write(Command::Request(id).encode()?));
                    }
                }
            }
        }
        Ok(ops)
    }

    /// How often [`Self::poll`] should run, if at all (QCY-protocol models
    /// notify battery changes on their own).
    pub fn poll_interval(&self) -> Option<Duration> {
        match self.dialect {
            Dialect::JieLi { .. } if self.polls_anc() => Some(Duration::from_secs(3)),
            Dialect::JieLi { .. } => Some(Duration::from_secs(30)),
            _ => None,
        }
    }

    fn polls_anc(&self) -> bool {
        POLL_ANC_MODELS.iter().any(|r| r.contains(&self.vendor_id))
    }

    /// The periodic status reads.
    pub fn poll(&mut self) -> Result<Vec<Op>, ProtocolError> {
        let polls_anc = self.polls_anc();
        let Some(jl) = &mut self.jieli else {
            return Ok(Vec::new());
        };
        let mut ops = vec![Op::Write(jl.poll()?)];
        if polls_anc {
            ops.push(Op::Write(jl.request(CommandId::Anc)?));
        }
        Ok(ops)
    }

    /// The operations that carry out `command`.
    pub fn encode(
        &mut self,
        command: &Command,
        cx: &Context<'_>,
    ) -> Result<Vec<Op>, ProtocolError> {
        let command = self.adapt(command, cx)?;
        match self.dialect {
            Dialect::Qcy => Ok(vec![Op::Write(command.encode()?)]),
            Dialect::JieLi { .. } => {
                let jl = self.jieli.as_mut().expect("JieLi codec");
                // Keys go through RCSP whatever characteristic the panel
                // names (long press included).
                if let Command::SetKey {
                    target: KeyTarget::Table | KeyTarget::Characteristic(_),
                    event,
                    action,
                } = &command
                {
                    return Ok(jl
                        .keys(&[(*event, *action)])?
                        .into_iter()
                        .map(Op::Write)
                        .collect());
                }
                if self.percent_anc
                    && let Command::SetAnc(mode) = &command
                {
                    let s = mode.to_setting();
                    return Ok(vec![Op::Write(jl.set_noise_percent(s.mode, s.level)?)]);
                }
                Ok(jl
                    .translate(&command.encode()?)?
                    .into_iter()
                    .map(Op::Write)
                    .collect())
            }
            Dialect::Zr => self.encode_zr(&command, cx),
        }
    }

    /// Model specific adjustments shared by all dialects.
    fn adapt(&self, command: &Command, cx: &Context<'_>) -> Result<Command, ProtocolError> {
        Ok(match command {
            // Parametric EQ is padded to as many bands as the firmware and
            // one write allow, like `QCYConnectManager.setEQData`.
            Command::SetParametricEq(eq) | Command::SetParametricEqV1(eq)
                if self.dialect == Dialect::Qcy =>
            {
                let band_len = if matches!(command, Command::SetParametricEq(_)) {
                    7
                } else {
                    6
                };
                let firmware_max = cx
                    .state
                    .max_eq_bands
                    .map_or(DEFAULT_MAX_EQ_BANDS, usize::from);
                let by_mtu = cx.max_write_len.saturating_sub(7) / 6;
                let fits = cx.max_write_len.saturating_sub(4 + 3) / band_len;
                let total = by_mtu.min(firmware_max).min(fits).max(eq.bands.len());
                let padded = eq.clone().padded(total);
                if band_len == 7 {
                    Command::SetParametricEq(padded)
                } else {
                    Command::SetParametricEqV1(padded)
                }
            }
            // Model 0x4161 takes a one-byte prompt volume.
            Command::SetPromptVolume(v) if self.vendor_id == 0x4161 => Command::SetValue {
                id: CommandId::PromptVolume,
                payload: vec![*v],
            },
            // "No action" is written as 0xFF on some vendors from fw 1.1.0.
            Command::SetKey {
                target,
                event,
                action: 0,
            } if self.dialect == Dialect::Qcy && self.no_action_is_ff(cx.state) => {
                Command::SetKey {
                    target: *target,
                    event: *event,
                    action: 0xFF,
                }
            }
            other => other.clone(),
        })
    }

    fn no_action_is_ff(&self, state: &DeviceState) -> bool {
        matches!(self.vendor_id & 0xF0, 0x20 | 0x30 | 0x80)
            && state
                .firmware
                .and_then(|f| f.left.or(f.right))
                .is_some_and(|v| v >= crate::types::Version(1, 1, 0))
    }

    fn encode_zr(&mut self, command: &Command, cx: &Context<'_>) -> Result<Vec<Op>, ProtocolError> {
        let eq = match command {
            Command::SetGraphicEq(eq) => Some((eq.preset, eq.gains.clone())),
            Command::SetParametricEq(eq) | Command::SetParametricEqV1(eq) => Some((
                eq.index,
                eq.active_bands().map(|b| b.gain_db.round() as i8).collect(),
            )),
            _ => None,
        };
        if let Some((preset, gains)) = eq {
            let mut ops = Vec::new();
            if !self.zr_time_sent {
                self.zr_time_sent = true;
                ops.push(Op::Write(Write {
                    characteristic: Characteristic::ZR_SETTING,
                    data: zr::time(cx.unix_time),
                }));
            }
            ops.push(Op::Write(Write {
                characteristic: Characteristic::EQ,
                data: zr::eq_blob(preset, &gains),
            }));
            return Ok(ops);
        }
        // Keys: the whole table, in ZR numbering.
        let changes: Vec<(u16, u8)> = match command {
            Command::SetKey { event, action, .. } => vec![(*event, *action)],
            Command::SetKeyFunctions(pairs) => {
                pairs.iter().map(|(g, a)| (u16::from(*g), a.id())).collect()
            }
            _ => return Ok(vec![Op::Write(command.encode()?)]),
        };
        let mut table: Vec<(u16, u8)> = cx
            .state
            .key_bindings
            .iter()
            .map(|b| (b.gesture_id, b.action.id()))
            .collect();
        if table.is_empty() {
            return Err(ProtocolError::InvalidValue(
                "the key table has not been read yet".into(),
            ));
        }
        for (g, a) in changes {
            match table.iter_mut().find(|(id, _)| *id == g) {
                Some(entry) => entry.1 = a,
                None => table.push((g, a)),
            }
        }
        table.sort_by_key(|(g, _)| *g);
        Ok(vec![Op::Write(Write {
            characteristic: Characteristic::KEYS,
            data: table
                .into_iter()
                .flat_map(|(g, a)| [zr::key_to_zr(g as u8), a])
                .collect(),
        })])
    }

    /// Reads that confirm `command` took effect (sent a moment later).
    pub fn confirm(&mut self, command: &Command) -> Result<Vec<Op>, ProtocolError> {
        let id = command.confirmation_request();
        let wrote = command.encode().ok().map(|w| w.characteristic);
        if let Some(jl) = &mut self.jieli {
            let id = match wrote {
                Some(Characteristic::EQ) => return Ok(vec![Op::Write(jl.request_graphic_eq()?)]),
                Some(Characteristic::FUNCTION) => {
                    return Ok(vec![Op::Write(jl.request_in_ear()?)]);
                }
                _ if matches!(
                    command,
                    Command::SetKey { .. } | Command::SetKeyFunctions(_)
                ) =>
                {
                    CommandId::KeyFunctions
                }
                _ => match id {
                    Some(id) => id,
                    None => return Ok(Vec::new()),
                },
            };
            return Ok(vec![Op::Write(jl.request(id)?)]);
        }
        Ok(match (self.dialect, wrote, id) {
            (Dialect::Zr, Some(Characteristic::FUNCTION), _) => {
                vec![Op::Read(Characteristic::ZR_SETTING)]
            }
            (_, Some(Characteristic::COMMAND), Some(id)) => {
                vec![Op::Write(Command::Request(id).encode()?)]
            }
            // Characteristic writes (keys, EQ, 0x000F, legacy keys) are read back.
            (_, Some(c), _)
                if c != Characteristic::COMMAND
                    && c != Characteristic::RESET
                    && c != Characteristic::ZR_SETTING
                    && c != Characteristic::LANGUAGE =>
            {
                vec![Op::Read(c)]
            }
            _ => Vec::new(),
        })
    }

    /// Decodes a read result or notification.
    pub fn decode(&mut self, c: Characteristic, data: &[u8]) -> Vec<Update> {
        if c == Characteristic::RCSP_NOTIFY {
            return self
                .jieli
                .as_mut()
                .map(|jl| jl.decode(data))
                .unwrap_or_default();
        }
        let raw = || Update::Characteristic {
            characteristic: c,
            data: data.to_vec(),
        };
        match (self.dialect, c) {
            (Dialect::Zr, Characteristic::KEYS) => vec![Update::KeyBindings(
                data.chunks_exact(2)
                    .filter(|p| p[0] != 0)
                    .map(|p| KeyBinding {
                        gesture_id: u16::from(zr::key_from_zr(p[0])),
                        action: KeyAction::from_id(p[1]),
                    })
                    .collect(),
            )],
            (Dialect::Zr, Characteristic::EQ) => match zr::parse_eq_blob(data) {
                Some((preset, gains)) => vec![Update::GraphicEq(GraphicEq { preset, gains })],
                None => Vec::new(),
            },
            (Dialect::Zr, Characteristic::ZR_SETTING) => {
                let mut out = vec![raw()];
                if let Some(on) = zr::in_ear_enabled(data) {
                    out.push(Update::Characteristic {
                        characteristic: Characteristic::FUNCTION,
                        data: vec![if on { 1 } else { 2 }],
                    });
                }
                out
            }
            // Per-gesture key characteristics: also a key binding.
            (
                _,
                Characteristic {
                    service: crate::uuids::Service::Control,
                    id: id @ 1..=6,
                },
            ) if !data.is_empty() => vec![
                raw(),
                Update::KeyBindings(vec![KeyBinding {
                    gesture_id: id,
                    action: KeyAction::from_id(data[0]),
                }]),
            ],
            _ => update::decode(c, data).unwrap_or_else(|_| vec![raw()]),
        }
    }
}

fn dedup(ids: &mut Vec<CommandId>) {
    let mut seen = std::collections::BTreeSet::new();
    ids.retain(|id| seen.insert(*id));
}

/// The noise control value a [`DeviceState`] reports, packed like the panel
/// ranges ([`crate::panel::anc_current`]); legacy models report the bare
/// noise mode (command `0x0C`).
pub fn current_anc(state: &DeviceState) -> Option<u32> {
    state
        .anc
        .map(|m: AncMode| crate::panel::anc_current(m.to_setting()))
        .or_else(|| {
            state
                .setting(CommandId::NoiseMode)
                .and_then(|p| p.first())
                .map(|&m| u32::from(m))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    fn cx(state: &DeviceState) -> Context<'_> {
        Context {
            state,
            max_write_len: 217,
            unix_time: 0x6ACD_7480,
        }
    }

    #[test]
    fn dialects() {
        assert_eq!(Dialect::for_vendor(19786), Dialect::Qcy);
        assert_eq!(Dialect::for_vendor(0x8071), Dialect::JieLi { new: true });
        assert_eq!(Dialect::for_vendor(6513), Dialect::JieLi { new: false });
        assert_eq!(Dialect::for_vendor(19857), Dialect::Zr);
    }

    #[test]
    fn qcy_parametric_eq_is_padded() {
        let panel = catalog::panel(19786);
        let mut codec = Codec::new(19786, &panel);
        let eq = panel.eq.as_ref().unwrap();
        let cmd = eq.apply_preset(eq.preset(10).unwrap()).pop().unwrap();
        let ops = codec.encode(&cmd, &cx(&DeviceState::default())).unwrap();
        let Op::Write(w) = &ops[0] else { panic!() };
        // jieli/panel notes §4.2: 20 bands at MTU 220 → FF 91 22 8F 0A 00 00 …
        assert_eq!(&w.data[..7], [0xFF, 0x91, 0x22, 0x8F, 0x0A, 0x00, 0x00]);
    }

    #[test]
    fn zr_keys_are_written_as_a_whole_table() {
        let panel = catalog::panel(19858);
        let mut codec = Codec::new(19858, &panel);
        let mut state = DeviceState::default();
        for u in codec.decode(Characteristic::KEYS, &[1, 0, 2, 1, 3, 4, 4, 0, 5, 1, 6, 7]) {
            state.apply(u);
        }
        let cmd = Command::SetKey {
            target: KeyTarget::Table,
            event: 6,
            action: 5,
        };
        let ops = codec.encode(&cmd, &cx(&state)).unwrap();
        // zr.md §3.2 example.
        assert_eq!(
            ops,
            [Op::Write(Write {
                characteristic: Characteristic::KEYS,
                data: vec![1, 0, 4, 0, 2, 1, 5, 1, 3, 4, 6, 5],
            })]
        );
    }

    #[test]
    fn zr_eq_sends_the_clock_first() {
        let panel = catalog::panel(19860);
        let mut codec = Codec::new(19860, &panel);
        let eq = panel.eq.as_ref().unwrap();
        assert_eq!(eq.kind, EqKind::Graphic);
        let cmd = eq.apply_preset(eq.preset(2).unwrap()).pop().unwrap();
        let state = DeviceState::default();
        let ops = codec.encode(&cmd, &cx(&state)).unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(
            ops[0],
            Op::Write(Write {
                characteristic: Characteristic::ZR_SETTING,
                data: vec![0, 0, 0x6A, 0xCD, 0x74, 0x80],
            })
        );
        let Op::Write(w) = &ops[1] else { panic!() };
        assert_eq!(w.data.len(), zr::EQ_BLOB_LEN);
        assert_eq!(codec.encode(&cmd, &cx(&state)).unwrap().len(), 1);
    }

    #[test]
    fn jieli_refresh_goes_through_rcsp() {
        let panel = catalog::panel(0x8071);
        let mut codec = Codec::new(0x8071, &panel);
        let ops = codec.refresh(&panel, |_| true).unwrap();
        assert!(ops.iter().all(|op| matches!(
            op,
            Op::Write(Write {
                characteristic: Characteristic::RCSP_WRITE,
                ..
            })
        )));
        assert!(codec.poll_interval().is_some());
    }

    #[test]
    fn legacy_language_and_reset_never_get_key_bytes() {
        let cmd = Command::SetKey {
            target: KeyTarget::Characteristic(Characteristic::RESET),
            event: 10,
            action: 1,
        };
        let panel = catalog::panel(19745);
        let mut codec = Codec::new(19745, &panel);
        assert!(codec.encode(&cmd, &cx(&DeviceState::default())).is_err());
    }
}
