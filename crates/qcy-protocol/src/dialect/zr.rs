//! ZR chip specifics (`research/notes/zr.md`).
//!
//! ZR earbuds use the normal control service, but:
//!
//! * the EQ characteristic `0x000B` takes a 144-byte blob of biquad
//!   coefficients (`ZRDeviceImpl.setEQ`) instead of `[preset, gains…]`;
//! * the key table `0x000D` numbers gestures left 1-3, right 4-6, and is
//!   always written as a whole;
//! * status (in-ear detection) is read from `0x000E`, which also takes the
//!   clock (`00 00 t₃ t₂ t₁ t₀`).

use std::f64::consts::PI;

/// Size of the EQ blob.
pub const EQ_BLOB_LEN: usize = 144;
/// Offset of the preset id inside the blob.
const EQ_TYPE_OFFSET: usize = 120;
/// Centre frequencies (Hz) and Q of the five filters; the app never sends others.
const FREQS: [u16; 5] = [62, 250, 1000, 4000, 8000];
const QS: [f64; 5] = [1.5, 1.5, 3.0, 3.0, 3.0];
const SAMPLE_RATE: f64 = 44_100.0;
/// 1.0 in Q2.21 fixed point.
const ONE: i32 = 1 << 21;

/// Gesture id in QCY numbering (1/2 single L/R, 3/4 double, 5/6 triple)
/// → ZR numbering (1-3 left, 4-6 right). Other ids are unchanged.
pub fn key_to_zr(id: u8) -> u8 {
    match id {
        1 => 1,
        2 => 4,
        3 => 2,
        4 => 5,
        5 => 3,
        6 => 6,
        other => other,
    }
}

/// Inverse of [`key_to_zr`].
pub fn key_from_zr(id: u8) -> u8 {
    match id {
        1 => 1,
        4 => 2,
        2 => 3,
        5 => 4,
        3 => 5,
        6 => 6,
        other => other,
    }
}

/// `x · 2²¹ + 0.5`, truncated toward zero like Java's `(int)(long)` cast.
fn q21(x: f64) -> i32 {
    (x * f64::from(ONE) + 0.5) as i64 as i32
}

/// RBJ peaking filter coefficients as the app computes them (`getCoeff`).
fn coefficients(freq: f64, q: f64, gain_db: i8) -> [i32; 6] {
    if gain_db == 0 {
        return [ONE, 0, 0, ONE, 0, 0];
    }
    let a = 10f64.powf(f64::from(gain_db) / 40.0);
    let w0 = 2.0 * PI * freq / SAMPLE_RATE;
    let qe = (10.0 * q / 7.0) * 2f64.sqrt() / 2.0;
    let alpha = w0.sin() / (2.0 * qe);
    let a0 = 1.0 + alpha / a;
    [
        q21((1.0 + alpha * a) / a0),
        q21(-2.0 * w0.cos() / a0),
        q21((1.0 - alpha * a) / a0),
        ONE,
        -q21(-2.0 * w0.cos() / a0),
        -q21((1.0 - alpha / a) / a0),
    ]
}

/// Encodes the 144-byte EQ blob for preset `eq_type` (1..=6, 255 custom)
/// and integer gains (5 or 10; only the first five drive filters).
pub fn eq_blob(eq_type: u8, gains: &[i8]) -> Vec<u8> {
    let gains = &gains[..gains.len().min(10)];
    let mut out = vec![0u8; EQ_BLOB_LEN];
    for band in 0..5 {
        let c = if eq_type == 1 {
            // "Default" is always flat, whatever the gains say.
            [ONE, 0, 0, ONE, 0, 0]
        } else {
            let gain = gains.get(band).copied().unwrap_or(0);
            coefficients(f64::from(FREQS[band]), QS[band], gain)
        };
        for (i, v) in c.iter().enumerate() {
            let at = band * 24 + i * 4;
            out[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    out[EQ_TYPE_OFFSET] = eq_type;
    let mut at = EQ_TYPE_OFFSET + 1;
    for &g in gains {
        out[at] = g as u8;
        at += 1;
    }
    out[at] = 0x5A;
    out[at + 1] = 0x5A;
    at += 2;
    if eq_type != 1 {
        for f in FREQS {
            out[at..at + 2].copy_from_slice(&f.to_le_bytes());
            at += 2;
        }
    }
    // Sum of every byte except the preset id.
    let sum: u32 = out[..EQ_BLOB_LEN - 1]
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != EQ_TYPE_OFFSET)
        .map(|(_, &b)| u32::from(b))
        .sum();
    out[EQ_BLOB_LEN - 1] = sum as u8;
    out
}

/// Preset id and the first five gains of a blob read back from `0x000B`.
pub fn parse_eq_blob(blob: &[u8]) -> Option<(u8, Vec<i8>)> {
    let tail = blob.get(EQ_TYPE_OFFSET..EQ_TYPE_OFFSET + 6)?;
    Some((tail[0], tail[1..].iter().map(|&b| b as i8).collect()))
}

/// The clock write for `0x000E`: `00 00` + Unix seconds, big endian.
pub fn time(unix_seconds: u32) -> Vec<u8> {
    let mut out = vec![0, 0];
    out.extend_from_slice(&unix_seconds.to_be_bytes());
    out
}

/// In-ear detection state from a `0x000E` read (`byte 1 & 0x7F == 1`).
pub fn in_ear_enabled(status: &[u8]) -> Option<bool> {
    (status.len() >= 3).then(|| status[1] & 0x7F == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    #[test]
    fn key_mapping_is_a_bijection() {
        for id in 0..=20 {
            assert_eq!(key_from_zr(key_to_zr(id)), id);
        }
        assert_eq!(key_to_zr(2), 4);
    }

    #[test]
    fn default_preset_blob() {
        let blob = eq_blob(1, &[0, 0, 0, 0, 0]);
        assert_eq!(&blob[120..128], [0x01, 0, 0, 0, 0, 0, 0x5A, 0x5A]);
        assert_eq!(blob[143], 0xF4);
    }

    #[test]
    fn pop_preset_blob_matches_the_app() {
        // zr.md §3.1, preset 2 "Pop" on a 5-band panel.
        let blob = eq_blob(2, &[5, -2, -4, -3, 6]);
        assert_eq!(
            &blob[..24],
            hex("e8 0d 20 00 60 24 c0 ff 5c ce 1f 00 00 00 20 00 a0 db 3f 00 bc 23 e0 ff")
        );
        assert_eq!(
            &blob[96..],
            hex(
                "3c 0e 23 00 f5 d3 e7 ff db cd 16 00 00 00 20 00 0b 2c 18 00 e9 23 e6 ff \
                 02 05 fe fc fd 06 5a 5a 3e 00 fa 00 e8 03 a0 0f 40 1f 00 00 00 00 00 04"
            )
        );
        assert_eq!(parse_eq_blob(&blob), Some((2, vec![5, -2, -4, -3, 6])));
    }

    #[test]
    fn custom_ten_band_blob_matches_the_app() {
        let blob = eq_blob(255, &[3, 0, -2, 0, 4, 0, 0, 0, 0, 0]);
        assert_eq!(
            &blob[120..],
            hex("ff 03 00 fe 00 04 00 00 00 00 00 5a 5a 3e 00 fa 00 e8 03 a0 0f 40 1f 15")
        );
        assert_eq!(
            &blob[..24],
            hex("45 08 20 00 ba 28 c0 ff a6 cf 1f 00 00 00 20 00 46 d7 3f 00 16 28 e0 ff")
        );
    }

    #[test]
    fn status_and_time() {
        assert_eq!(in_ear_enabled(&[0, 1, 0]), Some(true));
        assert_eq!(in_ear_enabled(&[0, 2, 0]), Some(false));
        assert_eq!(time(0x6ACD_7480), [0, 0, 0x6A, 0xCD, 0x74, 0x80]);
    }
}
