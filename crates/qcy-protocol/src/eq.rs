//! Equalizer encodings.
//!
//! QCY firmwares know two EQ formats:
//!
//! * **Graphic EQ** on characteristic `0x000B`: `[preset_id, g₀ … g₉]`, ten
//!   signed one-dB steps. Still readable on current firmwares; it reports the
//!   active preset id.
//! * **Parametric ("multi") EQ** sent inside a frame on `0x1001` with id
//!   [`CommandId::MultiEq2`](crate::CommandId::MultiEq2) (`0x22`):
//!
//!   ```text
//!   [index, master_lo, master_hi, { f_lo f_hi g_lo g_hi q_lo q_hi type } × n]
//!   ```
//!
//!   frequency in Hz, gain and Q in hundredths (signed / unsigned LE), `type`
//!   is the filter shape (`iitType` in the panel JSON: 0 = peaking, 4 is used
//!   for the 30 Hz low band of the factory presets). `index` identifies the
//!   preset (1..=10) or 255 for a custom curve.
//!
//! `0x20` ([`CommandId::MultiEq`](crate::CommandId::MultiEq)) is the same
//! without the `type` byte, used by older models.

use crate::ProtocolError;

/// Filter type used for every band of a user curve (`iitType` = 0).
pub const FILTER_PEAKING: u8 = 0;
/// Filter type the app uses for padding bands.
pub const FILTER_UNUSED: u8 = 255;
/// Preset index the app uses for a user-defined curve.
pub const CUSTOM_INDEX: u8 = 255;
/// Centre frequencies of the 10-band user EQ (from the panel JSON).
pub const GRAPHIC_FREQUENCIES: [u16; 10] = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
/// Gain range the app allows for user bands (`mindb` / `maxdb`).
pub const GRAPHIC_GAIN_RANGE: std::ops::RangeInclusive<i8> = -8..=8;

/// One band of a parametric EQ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqBand {
    pub freq_hz: u16,
    pub gain_db: f32,
    pub q: f32,
    pub filter: u8,
}

impl EqBand {
    pub const fn new(freq_hz: u16, gain_db: f32, q: f32, filter: u8) -> Self {
        Self {
            freq_hz,
            gain_db,
            q,
            filter,
        }
    }

    /// Empty band used to pad a curve to the length the device expects.
    pub const fn unused() -> Self {
        Self::new(0, 0.0, 0.0, FILTER_UNUSED)
    }

    pub fn is_unused(&self) -> bool {
        self.freq_hz == 0
    }
}

/// A complete parametric EQ curve.
#[derive(Debug, Clone, PartialEq)]
pub struct ParametricEq {
    /// Preset id (1..=10) or [`CUSTOM_INDEX`].
    pub index: u8,
    /// Global gain applied before the filters, in dB (usually ≤ 0 to avoid clipping).
    pub master_gain_db: f32,
    pub bands: Vec<EqBand>,
}

impl ParametricEq {
    /// Builds the curve the app sends for a 10-band user EQ (`gains` in dB).
    pub fn from_graphic(gains: &[i8; 10]) -> Self {
        let bands: Vec<EqBand> = GRAPHIC_FREQUENCIES
            .iter()
            .zip(gains)
            .map(|(&f, &g)| {
                let g = g.clamp(*GRAPHIC_GAIN_RANGE.start(), *GRAPHIC_GAIN_RANGE.end());
                EqBand::new(f, f32::from(g), 1.0, FILTER_PEAKING)
            })
            .collect();
        let master_gain_db = auto_master_gain(&bands, DEFAULT_VOLUME_LEVEL_DB, 0.0);
        Self {
            index: CUSTOM_INDEX,
            master_gain_db,
            bands,
        }
    }

    /// Returns the curve padded with [`EqBand::unused`] up to `total` bands,
    /// as the app does before sending (`QCYConnectManager.setEQData`).
    pub fn padded(mut self, total: usize) -> Self {
        while self.bands.len() < total {
            self.bands.push(EqBand::unused());
        }
        self
    }

    /// Payload for [`CommandId::MultiEq2`](crate::CommandId::MultiEq2) (7-byte bands).
    pub fn encode_v2(&self) -> Result<Vec<u8>, ProtocolError> {
        self.encode(true)
    }

    /// Payload for [`CommandId::MultiEq`](crate::CommandId::MultiEq) (6-byte bands).
    pub fn encode_v1(&self) -> Result<Vec<u8>, ProtocolError> {
        self.encode(false)
    }

    fn encode(&self, with_type: bool) -> Result<Vec<u8>, ProtocolError> {
        let band_len = if with_type { 7 } else { 6 };
        let len = 3 + self.bands.len() * band_len;
        if len > u8::MAX as usize {
            return Err(ProtocolError::PayloadTooLong(len));
        }
        let mut out = Vec::with_capacity(len);
        out.push(self.index);
        // The app truncates the master gain but rounds band values.
        out.extend_from_slice(&((self.master_gain_db * 100.0) as i16).to_le_bytes());
        for band in &self.bands {
            out.extend_from_slice(&band.freq_hz.to_le_bytes());
            out.extend_from_slice(&((band.gain_db * 100.0).round() as i16).to_le_bytes());
            out.extend_from_slice(&((band.q * 100.0).round() as u16).to_le_bytes());
            if with_type {
                out.push(band.filter);
            }
        }
        Ok(out)
    }

    /// Parses a [`CommandId::MultiEq2`](crate::CommandId::MultiEq2) payload.
    pub fn parse_v2(p: &[u8]) -> Result<Self, ProtocolError> {
        Self::parse(p, true)
    }

    /// Parses a [`CommandId::MultiEq`](crate::CommandId::MultiEq) payload.
    pub fn parse_v1(p: &[u8]) -> Result<Self, ProtocolError> {
        Self::parse(p, false)
    }

    fn parse(p: &[u8], with_type: bool) -> Result<Self, ProtocolError> {
        if p.len() < 3 {
            return Err(ProtocolError::TooShort {
                expected: 3,
                actual: p.len(),
            });
        }
        let band_len = if with_type { 7 } else { 6 };
        let bands = p[3..]
            .chunks_exact(band_len)
            .map(|b| EqBand {
                freq_hz: u16::from_le_bytes([b[0], b[1]]),
                gain_db: f32::from(i16::from_le_bytes([b[2], b[3]])) / 100.0,
                q: f32::from(u16::from_le_bytes([b[4], b[5]])) / 100.0,
                filter: if with_type { b[6] } else { FILTER_UNUSED },
            })
            .collect();
        Ok(Self {
            index: p[0],
            master_gain_db: f32::from(i16::from_le_bytes([p[1], p[2]])) / 100.0,
            bands,
        })
    }

    /// Bands that are actually in use (padding removed).
    pub fn active_bands(&self) -> impl Iterator<Item = &EqBand> {
        self.bands.iter().filter(|b| !b.is_unused())
    }

    /// Approximate magnitude response in dB at `freq_hz`, including the
    /// master gain (all bands treated as peaking filters, as the app does).
    pub fn response_db(&self, freq_hz: f64) -> f64 {
        let filters: f64 = self
            .active_bands()
            .map(|b| Biquad::peaking(f64::from(b.freq_hz), f64::from(b.q), f64::from(b.gain_db)))
            .map(|f| f.response_db(freq_hz))
            .sum();
        filters + f64::from(self.master_gain_db)
    }
}

/// Legacy graphic EQ from characteristic `0x000B`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicEq {
    /// Active preset id (values ≥ 128 denote stored custom curves).
    pub preset: u8,
    pub gains: Vec<i8>,
}

impl GraphicEq {
    pub fn parse(p: &[u8]) -> Result<Self, ProtocolError> {
        let (&preset, rest) = p.split_first().ok_or(ProtocolError::TooShort {
            expected: 1,
            actual: 0,
        })?;
        // 13-byte variant carries game EQ type + current mode after 10 gains.
        let gains = if rest.len() == 12 { &rest[..10] } else { rest };
        Ok(Self {
            preset,
            gains: gains.iter().map(|&b| b as i8).collect(),
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        std::iter::once(self.preset)
            .chain(self.gains.iter().map(|&g| g as u8))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Automatic master gain (port of QCYConnectManager.getMaxGain)
// ---------------------------------------------------------------------------

/// `volumelevel` from the MeloBuds Pro panel JSON.
pub const DEFAULT_VOLUME_LEVEL_DB: f64 = 8.0;
const SAMPLE_RATE: f64 = 44_100.0;

/// Computes the master gain the app would send for a user curve.
///
/// The app sums the magnitude responses of all peaking filters between 20 Hz
/// and 20 kHz and, if the resulting peak exceeds `volume_level` (below
/// 1.5 kHz) or `volume_level + 4` (above), lowers the master gain by the
/// excess so the earbuds do not clip. `base` is the previous preset's master
/// gain (0 for every MeloBuds Pro preset).
pub fn auto_master_gain(bands: &[EqBand], volume_level: f64, base: f32) -> f32 {
    let filters: Vec<Biquad> = bands
        .iter()
        .filter(|b| b.freq_hz > 0)
        .map(|b| Biquad::peaking(f64::from(b.freq_hz), f64::from(b.q), f64::from(b.gain_db)))
        .collect();

    let mut peak_db = 0.0_f64;
    let mut peak_freq = 0u32;
    for (i, freq) in (20..=20_000u32).step_by(5).enumerate() {
        let total: f64 = filters.iter().map(|f| f.response_db(f64::from(freq))).sum();
        if total > peak_db {
            peak_db = total;
            peak_freq = i as u32 * 5;
        }
    }
    let peak_db = peak_db + 0.5;

    let limit = if peak_db > volume_level && peak_freq < 1500 {
        volume_level
    } else if peak_db > volume_level + 4.0 && peak_freq >= 1500 {
        volume_level + 4.0
    } else {
        return base;
    };
    base + (limit - peak_db) as f32
}

/// RBJ cookbook peaking filter, exactly as `Filter.getPeakEQ`.
struct Biquad {
    b: [f64; 3],
    a: [f64; 3],
}

impl Biquad {
    fn peaking(freq: f64, q: f64, gain_db: f64) -> Self {
        let w0 = std::f64::consts::TAU * freq / SAMPLE_RATE;
        let alpha = w0.sin() / (2.0 * q);
        let a = 10f64.powf(gain_db / 40.0);
        let cos = w0.cos();
        Self {
            b: [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
            a: [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
        }
    }

    /// Magnitude response in dB, using the app's slightly odd ω mapping
    /// (`FrequencyResponse.getFreqOneFreq`).
    fn response_db(&self, freq: f64) -> f64 {
        let w = (freq * 20_000.0 + 1.0) * std::f64::consts::PI / (SAMPLE_RATE * 10_000.0);
        let eval = |c: &[f64; 3]| {
            // c0 + c1·e^{-jw} + c2·e^{-2jw}
            let re = c[0] + c[1] * w.cos() + c[2] * (2.0 * w).cos();
            let im = -c[1] * w.sin() - c[2] * (2.0 * w).sin();
            (re, im)
        };
        let (nr, ni) = eval(&self.b);
        let (dr, di) = eval(&self.a);
        let mag = (nr * nr + ni * ni).sqrt() / (dr * dr + di * di).sqrt();
        20.0 * mag.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parametric_roundtrip() {
        let eq = ParametricEq {
            index: 3,
            master_gain_db: -2.5,
            bands: vec![
                EqBand::new(30, 0.0, 0.7, 4),
                EqBand::new(220, -8.0, 1.0, 0),
                EqBand::new(12000, 8.0, 0.8, 0),
            ],
        }
        .padded(5);
        let bytes = eq.encode_v2().unwrap();
        assert_eq!(bytes.len(), 3 + 5 * 7);
        assert_eq!(&bytes[..3], &[3, 0x06, 0xff]); // -250 LE
        assert_eq!(&bytes[3..10], &[30, 0, 0, 0, 70, 0, 4]);
        assert_eq!(ParametricEq::parse_v2(&bytes).unwrap(), eq);
    }

    #[test]
    fn flat_curve_needs_no_master_gain() {
        let eq = ParametricEq::from_graphic(&[0; 10]);
        assert_eq!(eq.master_gain_db, 0.0);
        assert_eq!(eq.index, CUSTOM_INDEX);
    }

    #[test]
    fn heavy_bass_is_attenuated() {
        let eq = ParametricEq::from_graphic(&[8, 8, 8, 8, 0, 0, 0, 0, 0, 0]);
        assert!(eq.master_gain_db < -1.0, "got {}", eq.master_gain_db);
    }

    #[test]
    fn biquad_peak_matches_gain() {
        let f = Biquad::peaking(1000.0, 1.0, 6.0);
        assert!((f.response_db(1000.0) - 6.0).abs() < 0.05);
        assert!(f.response_db(20.0).abs() < 0.1);
    }

    #[test]
    fn graphic_real_sample() {
        let g = GraphicEq::parse(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(g.preset, 1);
        assert_eq!(g.gains, vec![0; 10]);
    }
}
