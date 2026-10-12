//! Framing used on the `0x1001` (command) and `0x1002` (settings) characteristics.
//!
//! A frame is a start-of-frame byte, a length byte, and one or more
//! type-length-value entries:
//!
//! ```text
//! +------+-----+---------+---------+-----------+---------+---------+-----------+-----
//! | 0xFF | len | id₀ (1) | n₀ (1)  | payload₀  | id₁ (1) | n₁ (1)  | payload₁  | …
//! +------+-----+---------+---------+-----------+---------+---------+-----------+-----
//!              \_______________________ len bytes _______________________________/
//! ```
//!
//! This mirrors `DataAnalyse.getSendData` / `DataAnalyse.analyseAllCMD` in the app.

use crate::ProtocolError;

/// Start-of-frame marker.
pub const SOF: u8 = 0xFF;

/// One type-length-value entry inside a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tlv {
    /// Command / setting identifier, see [`crate::CommandId`].
    pub id: u8,
    /// Raw payload (at most 255 bytes).
    pub payload: Vec<u8>,
}

impl Tlv {
    pub fn new(id: impl Into<u8>, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            id: id.into(),
            payload: payload.into(),
        }
    }
}

/// Encodes entries into a single frame.
pub fn encode(entries: &[Tlv]) -> Result<Vec<u8>, ProtocolError> {
    let body_len: usize = entries.iter().map(|e| 2 + e.payload.len()).sum();
    if body_len > u8::MAX as usize {
        return Err(ProtocolError::PayloadTooLong(body_len));
    }

    let mut out = Vec::with_capacity(2 + body_len);
    out.push(SOF);
    out.push(body_len as u8);
    for entry in entries {
        if entry.payload.len() > u8::MAX as usize {
            return Err(ProtocolError::PayloadTooLong(entry.payload.len()));
        }
        out.push(entry.id);
        out.push(entry.payload.len() as u8);
        out.extend_from_slice(&entry.payload);
    }
    Ok(out)
}

/// Convenience wrapper for the overwhelmingly common single-entry frame.
pub fn encode_one(id: impl Into<u8>, payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    encode(&[Tlv::new(id, payload.to_vec())])
}

/// Decodes a frame into its entries.
///
/// The app silently ignores malformed frames; we report precisely what is
/// wrong instead so callers can log it.
pub fn decode(bytes: &[u8]) -> Result<Vec<Tlv>, ProtocolError> {
    if bytes.len() < 2 {
        return Err(ProtocolError::TooShort {
            expected: 2,
            actual: bytes.len(),
        });
    }
    if bytes[0] != SOF {
        return Err(ProtocolError::BadStartOfFrame(bytes[0]));
    }
    let declared = bytes[1] as usize;
    let body = &bytes[2..];
    if body.len() != declared {
        return Err(ProtocolError::LengthMismatch {
            declared,
            actual: body.len(),
        });
    }

    let mut entries = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        let id = rest[0];
        let Some(&len) = rest.get(1) else {
            return Err(ProtocolError::TruncatedEntry { id });
        };
        let len = len as usize;
        let payload = rest
            .get(2..2 + len)
            .ok_or(ProtocolError::TruncatedEntry { id })?;
        entries.push(Tlv::new(id, payload.to_vec()));
        rest = &rest[2 + len..];
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_single() {
        let bytes = encode_one(0x24u8, &[0x01]).unwrap();
        assert_eq!(bytes, [0xFF, 0x03, 0x24, 0x01, 0x01]);
        assert_eq!(decode(&bytes).unwrap(), vec![Tlv::new(0x24u8, vec![1])]);
    }

    #[test]
    fn empty_payload_entry() {
        // "reset to default" as found in the control panel JSON: FF020100
        let bytes = encode_one(0x01u8, &[]).unwrap();
        assert_eq!(bytes, [0xFF, 0x02, 0x01, 0x00]);
    }

    #[test]
    fn decodes_real_settings_dump() {
        // Read from a QCY MeloBuds Pro (fw 2.0.6), characteristic 0x1002.
        let raw = [
            0xff, 0x30, 0x2c, 0x04, 0x00, 0x01, 0x01, 0x01, 0x08, 0x03, 0x46, 0x46, 0x0f, 0x09,
            0x01, 0x02, 0x17, 0x03, 0x01, 0x05, 0x00, 0x10, 0x01, 0x00, 0x16, 0x01, 0x32, 0x04,
            0x01, 0x02, 0x23, 0x01, 0x00, 0x24, 0x01, 0x01, 0x1f, 0x02, 0x05, 0x00, 0x1d, 0x02,
            0x08, 0x0f, 0x19, 0x04, 0x57, 0x51, 0x30, 0x30,
        ];
        let entries = decode(&raw).unwrap();
        let ids: Vec<u8> = entries.iter().map(|e| e.id).collect();
        assert_eq!(
            ids,
            [
                0x2c, 0x08, 0x09, 0x17, 0x10, 0x16, 0x04, 0x23, 0x24, 0x1f, 0x1d, 0x19
            ]
        );
        assert_eq!(entries.last().unwrap().payload, b"WQ00");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            decode(&[0x00, 0x00]),
            Err(ProtocolError::BadStartOfFrame(0))
        );
        assert!(matches!(
            decode(&[0xFF, 0x05, 0x01]),
            Err(ProtocolError::LengthMismatch { .. })
        ));
        assert_eq!(
            decode(&[0xFF, 0x03, 0x01, 0x05, 0x00]),
            Err(ProtocolError::TruncatedEntry { id: 1 })
        );
    }

    #[test]
    fn rejects_oversized() {
        let big = vec![0u8; 300];
        assert!(encode_one(1u8, &big).is_err());
    }
}
