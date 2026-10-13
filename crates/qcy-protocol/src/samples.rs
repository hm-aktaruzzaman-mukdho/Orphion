//! Raw data captured from a real QCY MeloBuds Pro (model `0x4D4A`,
//! firmware 2.0.6). Used by unit tests and by front-end demo modes.

use crate::state::DeviceState;
use crate::{Characteristic, update};

/// Manufacturer data (company `0x521C`) advertised by the BLE endpoint.
pub const ADVERTISEMENT: [u8; 24] = [
    0x4d, 0x4a, 0xad, 0x80, 0x6e, 0x5e, 0x5c, 0x00, 0x5c, 0x9a, 0x0f, 0xac, 0x84, 0x60, 0xb0, 0xa4,
    0x89, 0x02, 0xac, 0x84, 0x60, 0x2b, 0xa5, 0x89,
];

/// Value of the settings characteristic `0x1002`.
pub const SETTINGS: [u8; 50] = [
    0xff, 0x30, 0x2c, 0x04, 0x00, 0x01, 0x01, 0x01, 0x08, 0x03, 0x46, 0x46, 0x0f, 0x09, 0x01, 0x02,
    0x17, 0x03, 0x01, 0x05, 0x00, 0x10, 0x01, 0x00, 0x16, 0x01, 0x32, 0x04, 0x01, 0x02, 0x23, 0x01,
    0x00, 0x24, 0x01, 0x01, 0x1f, 0x02, 0x05, 0x00, 0x1d, 0x02, 0x08, 0x0f, 0x19, 0x04, 0x57, 0x51,
    0x30, 0x30,
];

/// Value of the battery characteristic `0x0008` (65 %, 75 %, case unknown).
pub const BATTERY: [u8; 3] = [0x41, 0x4b, 0x00];

/// Value of the version characteristic `0x0007` (2.0.6 / 2.0.6).
pub const VERSION: [u8; 6] = [2, 0, 6, 2, 0, 6];

/// Value of the key table characteristic `0x000D`.
pub const KEYS: [u8; 20] = [1, 0, 2, 0, 3, 1, 4, 1, 5, 4, 6, 3, 7, 0, 8, 0, 0, 0, 0, 0];

/// Reply to "request `0x18`" (Bluetooth name, NUL padded).
pub const NAME_REPLY: [u8; 36] = [
    0xff, 0x22, 0x18, 0x20, b'Q', b'C', b'Y', b' ', b'M', b'e', b'l', b'o', b'B', b'u', b'd', b's',
    b' ', b'P', b'r', b'o', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// A [`DeviceState`] as it looks right after connecting to the sample earbuds.
pub fn state() -> DeviceState {
    let mut state = DeviceState::default();
    let reads: [(Characteristic, &[u8]); 5] = [
        (Characteristic::VERSION, &VERSION),
        (Characteristic::BATTERY, &BATTERY),
        (Characteristic::SETTINGS, &SETTINGS),
        (Characteristic::KEYS, &KEYS),
        (Characteristic::SETTINGS, &NAME_REPLY),
    ];
    for (c, data) in reads {
        for u in update::decode(c, data).expect("samples are valid") {
            state.apply(u);
        }
    }
    // The default preset, as the earbuds report it.
    let panel = crate::catalog::panel(19786);
    if let Some(p) = panel.eq.as_ref().and_then(|eq| eq.preset(1)) {
        state.apply(crate::Update::ParametricEq(crate::eq::ParametricEq {
            index: p.id,
            master_gain_db: p.master_gain_db,
            bands: p.bands.clone(),
        }));
    }
    state
}

#[cfg(test)]
mod tests {
    #[test]
    fn sample_state_is_complete() {
        let s = super::state();
        assert_eq!(s.device_name.as_deref(), Some("QCY MeloBuds Pro"));
        assert!(s.battery.is_some() && s.anc.is_some() && s.key_bindings.len() == 8);
        assert!(s.unknown.is_empty());
    }
}
