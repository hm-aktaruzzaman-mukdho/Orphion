//! Parsing of the manufacturer specific data QCY earbuds advertise over BLE.
//!
//! QCY buds advertise on a *separate, random BLE address* (e.g.
//! `C4:AC:60:89:A4:B1` for a set whose classic address is `84:AC:60:89:A4:B0`).
//! The advertisement embeds the classic address, which is how we match the BLE
//! control endpoint to the headset paired for audio.
//!
//! Layout (company id `0x521C`, port of `Devicebind.refreshInfoFromAVData`):
//!
//! | Offset | Meaning                                                     |
//! |--------|-------------------------------------------------------------|
//! | 0..2   | model / vendor id, big endian                               |
//! | 3      | bits 1..=3: colour index                                    |
//! | 5,6,7  | left / right / case battery (bit 7 = charging)              |
//! | 11..17 | classic MAC of this bud, scrambled (see [`Advertisement::parse`]) |
//! | 18..24 | classic MAC of the other bud (all zero if same)             |

use std::fmt;

use crate::ProtocolError;
use crate::types::BatteryStatus;

/// A Bluetooth device address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    pub fn is_zero(&self) -> bool {
        self.0 == [0; 6]
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.0;
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            b[0], b[1], b[2], b[3], b[4], b[5]
        )
    }
}

impl std::str::FromStr for MacAddress {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || ProtocolError::InvalidValue(format!("not a MAC address: {s:?}"));
        let mut out = [0u8; 6];
        let mut parts = s.split([':', '_', '-']);
        for byte in &mut out {
            let part = parts.next().ok_or_else(invalid)?;
            *byte = u8::from_str_radix(part, 16).map_err(|_| invalid())?;
        }
        if parts.next().is_some() {
            return Err(invalid());
        }
        Ok(Self(out))
    }
}

/// Chip vendor family, derived from the model id exactly like the app's
/// `Devicebind.isJLDevice()/isZRDevice()/...`. Only [`ChipFamily::WuQi`],
/// [`ChipFamily::Realtek`] and [`ChipFamily::Generic`] speak the protocol
/// implemented by this crate; JieLi and ZR use vendor SDKs instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChipFamily {
    WuQi,
    JieLi,
    Zr,
    Realtek,
    Generic,
}

impl ChipFamily {
    pub fn from_vendor_id(id: u16) -> Self {
        match id & 0xF0 {
            _ if id == 512 => ChipFamily::JieLi,
            0x40 => ChipFamily::WuQi,
            0x70 => ChipFamily::JieLi,
            0x90 => ChipFamily::Zr,
            0xC0 => ChipFamily::Realtek,
            _ => ChipFamily::Generic,
        }
    }

    /// Whether this crate's protocol applies to the chip.
    pub fn uses_qcy_protocol(self) -> bool {
        !matches!(self, ChipFamily::JieLi | ChipFamily::Zr)
    }
}

/// Decoded QCY advertisement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertisement {
    pub vendor_id: u16,
    pub colour: u8,
    pub battery: BatteryStatus,
    /// Classic (BR/EDR) address of the advertising bud.
    pub classic_address: MacAddress,
    /// Classic address of the other bud (equal to `classic_address` if unknown).
    pub peer_address: MacAddress,
}

impl Advertisement {
    /// Minimum length the app accepts.
    pub const MIN_LEN: usize = 20;

    /// Parses the manufacturer data payload (without the company id).
    pub fn parse(data: &[u8]) -> Result<Self, ProtocolError> {
        if data.len() < Self::MIN_LEN {
            return Err(ProtocolError::TooShort {
                expected: Self::MIN_LEN,
                actual: data.len(),
            });
        }
        let mut d = [0u8; 24];
        let n = data.len().min(24);
        d[..n].copy_from_slice(&data[..n]);

        // The address bytes are stored in a peculiar order; this is the app's
        // `String.format("%02x:…", b[12], b[11], b[13], b[16], b[15], b[14])`.
        let classic_address = MacAddress([d[12], d[11], d[13], d[16], d[15], d[14]]);
        let mut peer_address = MacAddress([d[19], d[18], d[20], d[23], d[22], d[21]]);
        if peer_address.is_zero() {
            peer_address = classic_address;
        }

        Ok(Self {
            vendor_id: u16::from_be_bytes([d[0], d[1]]),
            colour: (d[3] & 0x0E) >> 1,
            battery: BatteryStatus::parse(&d[5..8])?,
            classic_address,
            peer_address,
        })
    }

    pub fn chip(&self) -> ChipFamily {
        ChipFamily::from_vendor_id(self.vendor_id)
    }

    /// Whether this advertisement belongs to the headset with the given classic address.
    pub fn matches_classic(&self, addr: MacAddress) -> bool {
        self.classic_address == addr || self.peer_address == addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_melobuds_pro_advertisement() {
        // Captured from C4:AC:60:89:A4:B1 (manufacturer 0x521C).
        let data = [
            0x4d, 0x4a, 0xad, 0x80, 0x6e, 0x5e, 0x5c, 0x00, 0x5c, 0x9a, 0x0f, 0xac, 0x84, 0x60,
            0xb0, 0xa4, 0x89, 0x02, 0xac, 0x84, 0x60, 0x2b, 0xa5, 0x89,
        ];
        let adv = Advertisement::parse(&data).unwrap();
        assert_eq!(adv.vendor_id, 19786);
        assert_eq!(adv.chip(), ChipFamily::WuQi);
        assert_eq!(adv.classic_address.to_string(), "84:AC:60:89:A4:B0");
        assert_eq!(adv.peer_address.to_string(), "84:AC:60:89:A5:2B");
        assert_eq!(adv.battery.left.unwrap().percent, 94);
        assert_eq!(adv.battery.right.unwrap().percent, 92);
        assert!(adv.matches_classic("84:AC:60:89:A4:B0".parse().unwrap()));
    }

    #[test]
    fn mac_parsing() {
        let m: MacAddress = "84_ac_60_89_a4_b0".parse().unwrap();
        assert_eq!(m.to_string(), "84:AC:60:89:A4:B0");
        assert!("84:AC".parse::<MacAddress>().is_err());
    }
}
