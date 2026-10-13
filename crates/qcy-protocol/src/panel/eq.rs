//! EQ layouts (panel types 2 and 23).
//!
//! A panel's EQ is one of three families, decided by which keys it has:
//!
//! | family        | panel keys          | written as                              |
//! |---------------|---------------------|-----------------------------------------|
//! | graphic       | no `q`              | characteristic `0x000B`: `[id, g0..g9]` |
//! | parametric v1 | `q`, no `iitType`   | command `0x20`, 6-byte bands            |
//! | parametric v2 | `q` and `iitType`   | command `0x22`, 7-byte bands            |
//!
//! Presets carry their own bands; the custom curve uses the layout's bands
//! with the user's gains (`QCYConnectManager.setEQData`).

use serde_json::Value;

use super::json::{array, boolean, csv, csv_numbers, float, frequency, int, string};
use crate::Command;
use crate::eq::{
    CUSTOM_INDEX, DEFAULT_VOLUME_LEVEL_DB, EqBand, FILTER_UNUSED, GraphicEq, ParametricEq,
    auto_master_gain,
};

/// How the EQ is sent to the earbuds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqKind {
    Graphic,
    /// Command `0x20`.
    ParametricV1,
    /// Command `0x22`.
    ParametricV2,
}

impl EqKind {
    pub fn is_parametric(self) -> bool {
        self != EqKind::Graphic
    }
}

/// One band of the custom curve editor.
#[derive(Debug, Clone, PartialEq)]
pub struct BandSpec {
    /// Label as written in the panel (`"1k"`).
    pub label: String,
    pub freq_hz: u32,
    pub q: f32,
    pub filter: u8,
}

/// The EQ section of a panel.
#[derive(Debug, Clone, PartialEq)]
pub struct EqControl {
    pub title: String,
    pub kind: EqKind,
    /// Custom curve gains are limited to `-max_db..=max_db`.
    pub max_db: i8,
    /// Added to every gain of a custom graphic EQ before writing.
    pub base_db: i8,
    /// Bands of the custom curve.
    pub bands: Vec<BandSpec>,
    /// Default gains of the custom curve.
    pub custom_gains: Vec<f32>,
    /// Peak level used by the automatic master gain.
    pub volume_level: f64,
    pub presets: Vec<EqPreset>,
    /// Shown when leaving preset 10 or 18 (they reboot the earbuds).
    pub reboot_tip: Option<String>,
}

/// A factory preset.
#[derive(Debug, Clone, PartialEq)]
pub struct EqPreset {
    pub id: u8,
    pub name: String,
    pub gains: Vec<f32>,
    /// Parametric presets only: their own band layout.
    pub bands: Vec<EqBand>,
    pub master_gain_db: f32,
    /// Confirmation text shown before applying, if any.
    pub warning: Option<String>,
    /// Refused by the app unless both buds are connected.
    pub needs_both_buds: bool,
}

/// Preset id of the adaptive EQ, which also enables command `0x27`.
pub const ADAPTIVE_PRESET: u8 = 9;

impl EqControl {
    pub fn preset(&self, id: u8) -> Option<&EqPreset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// Commands that apply a preset, in order.
    pub fn apply_preset(&self, preset: &EqPreset) -> Vec<Command> {
        let mut out = Vec::new();
        if preset.id == ADAPTIVE_PRESET {
            out.push(Command::SetValue {
                id: crate::CommandId::AdaptiveEq,
                payload: vec![1],
            });
        }
        out.push(match self.kind {
            EqKind::Graphic => Command::SetGraphicEq(GraphicEq {
                preset: preset.id,
                gains: preset.gains.iter().map(|&g| g.trunc() as i8).collect(),
            }),
            kind => parametric(
                kind,
                ParametricEq {
                    index: preset.id,
                    master_gain_db: preset.master_gain_db,
                    bands: preset.bands.clone(),
                },
            ),
        });
        out
    }

    /// The command for a custom curve with `gains` (one per [`Self::bands`]).
    pub fn apply_custom(&self, gains: &[f32]) -> Command {
        let max = f32::from(self.max_db);
        let gains = gains.iter().map(|g| g.clamp(-max, max));
        match self.kind {
            EqKind::Graphic => Command::SetGraphicEq(GraphicEq {
                preset: CUSTOM_INDEX,
                gains: gains
                    .map(|g| (g.trunc() as i32 + i32::from(self.base_db)).clamp(-128, 127) as i8)
                    .collect(),
            }),
            kind => {
                let bands: Vec<EqBand> = self
                    .bands
                    .iter()
                    .zip(gains)
                    .map(|(b, g)| EqBand::new(b.freq_hz as u16, g, b.q, b.filter))
                    .collect();
                let master_gain_db = auto_master_gain(&bands, self.volume_level, 0.0);
                parametric(
                    kind,
                    ParametricEq {
                        index: CUSTOM_INDEX,
                        master_gain_db,
                        bands,
                    },
                )
            }
        }
    }
}

fn parametric(kind: EqKind, eq: ParametricEq) -> Command {
    if kind == EqKind::ParametricV1 {
        Command::SetParametricEqV1(eq)
    } else {
        Command::SetParametricEq(eq)
    }
}

/// Bands from parallel `freq`/`eqs`/`q`/`iitType` lists of one object.
fn bands(v: &Value, gains: &[f32]) -> Vec<EqBand> {
    let freqs: Vec<u32> = csv(v, "freq").iter().filter_map(|f| frequency(f)).collect();
    let q = csv_numbers(v, "q");
    let types = csv_numbers(v, "iitType");
    freqs
        .iter()
        .zip(&q)
        .zip(gains)
        .enumerate()
        .map(|(i, ((&f, &q), &g))| {
            let filter = types.get(i).map_or(FILTER_UNUSED, |&t| t as u8);
            EqBand::new(f.min(u32::from(u16::MAX)) as u16, g, q as f32, filter)
        })
        .collect()
}

fn gains(v: &Value) -> Vec<f32> {
    csv_numbers(v, "eqs")
        .into_iter()
        .map(|g| g as f32)
        .collect()
}

pub(super) fn parse(layout: &Value) -> Option<EqControl> {
    let kind = match (
        layout.get("q").is_some() && !csv_numbers(layout, "q").is_empty(),
        layout.get("iitType").is_some(),
    ) {
        (false, _) => EqKind::Graphic,
        (true, false) => EqKind::ParametricV1,
        (true, true) => EqKind::ParametricV2,
    };
    let labels = csv(layout, "freq");
    let q = csv_numbers(layout, "q");
    let types = csv_numbers(layout, "iitType");
    let count = int(layout, "count")
        .and_then(|c| usize::try_from(c).ok())
        .filter(|&c| c > 0)
        .unwrap_or(labels.len());
    let band_specs: Vec<BandSpec> = labels
        .iter()
        .take(count)
        .enumerate()
        .map(|(i, label)| BandSpec {
            label: label.clone(),
            freq_hz: frequency(label).unwrap_or(0),
            q: q.get(i).map_or(1.0, |&q| q as f32),
            filter: types.get(i).map_or(0, |&t| t as u8),
        })
        .collect();

    let mut presets: Vec<EqPreset> = array(layout, "sys_eq")
        .iter()
        .enumerate()
        .filter_map(|(i, p)| preset(p, i + 1))
        .collect();
    if let Some(extra) = layout.get("individuation_eq")
        && let Some(p) = preset(extra, presets.len() + 1)
    {
        presets.push(p);
    }
    if presets.is_empty() && band_specs.is_empty() {
        return None;
    }
    let mut custom_gains = gains(layout);
    custom_gains.resize(band_specs.len(), 0.0);
    Some(EqControl {
        title: string(layout, "name").unwrap_or_else(|| "Equalizer".into()),
        kind,
        max_db: int(layout, "maxdb").unwrap_or(8).clamp(1, 24) as i8,
        base_db: int(layout, "basedb").unwrap_or(0).clamp(-24, 24) as i8,
        bands: band_specs,
        custom_gains,
        volume_level: float(layout, "volumelevel").unwrap_or(DEFAULT_VOLUME_LEVEL_DB),
        presets,
        reboot_tip: string(layout, "rebootTip"),
    })
}

fn preset(p: &Value, fallback_id: usize) -> Option<EqPreset> {
    let id = int(p, "id")
        .and_then(|id| u8::try_from(id).ok())
        .or_else(|| u8::try_from(fallback_id).ok())?;
    let gains = gains(p);
    Some(EqPreset {
        id,
        name: string(p, "name").unwrap_or_else(|| format!("Preset {id}")),
        bands: bands(p, &gains),
        gains,
        master_gain_db: float(p, "mastergain").unwrap_or(0.0) as f32,
        warning: string(p, "alarmMsg"),
        needs_both_buds: boolean(p, "onlyTws"),
    })
}

/// Layout 23 (game EQ): graphic presets with their own ids.
pub(super) fn parse_game(layout: &Value) -> Option<EqControl> {
    let mut eq = parse(layout)?;
    // Missing ids are 0-based here, unlike layout 2.
    for (i, (p, raw)) in eq
        .presets
        .iter_mut()
        .zip(array(layout, "sys_eq"))
        .enumerate()
    {
        if int(raw, "id").is_none() {
            p.id = i as u8;
        }
    }
    eq.kind = EqKind::Graphic;
    Some(eq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn graphic_preset_matches_the_app() {
        // 19393 HT03 preset 2 "Pop" → 0x000B ← 02 FA 05 FD FE 05 04 FC FD 06 04.
        let eq = parse(&json!({
            "name": "EQ", "count": 10, "maxdb": 6, "freq": "31,62,125,250,500,1k,2k,4k,8k,16k",
            "sys_eq": [{"name": "Default", "eqs": "0,0,0,0,0,0,0,0,0,0"},
                       {"name": "Pop", "eqs": "-6,5,-3,-2,5,4,-4,-3,6,4"}]
        }))
        .unwrap();
        assert_eq!(eq.kind, EqKind::Graphic);
        let cmds = eq.apply_preset(eq.preset(2).unwrap());
        assert_eq!(
            cmds[0].encode().unwrap().data,
            [
                0x02, 0xFA, 0x05, 0xFD, 0xFE, 0x05, 0x04, 0xFC, 0xFD, 0x06, 0x04
            ]
        );
        assert_eq!(eq.bands[5].freq_hz, 1000);
    }

    #[test]
    fn parametric_kinds() {
        let v2 = parse(&json!({"freq": "30,220", "q": "0.7,1", "iitType": "4,0",
            "sys_eq": [{"id": 1, "name": "A", "eqs": "0,-8", "freq": "30,220", "q": "0.7,1", "iitType": "4,0"}]}))
        .unwrap();
        assert_eq!(v2.kind, EqKind::ParametricV2);
        assert_eq!(v2.presets[0].bands[0].filter, 4);
        let v1 = parse(&json!({"freq": "30,220", "q": "0.7,1", "sys_eq": []})).unwrap();
        assert_eq!(v1.kind, EqKind::ParametricV1);
        assert!(matches!(
            v1.apply_custom(&[1.0, 2.0]),
            Command::SetParametricEqV1(_)
        ));
    }
}
