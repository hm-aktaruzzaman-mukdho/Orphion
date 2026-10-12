//! Windows backend: the WinRT Bluetooth LE API (`Windows.Devices.Bluetooth`),
//! available on Windows 10 2004+ and Windows 11 without extra drivers or
//! libraries.
//!
//! * [`discovery`]: advertisement scanning ([`WinRt`] implements
//!   [`Backend`](crate::transport::Backend)).
//! * [`gatt`]: the GATT connection ([`WinGattLink`] implements
//!   [`Link`](crate::transport::Link)).
//!
//! Differences from BlueZ worth knowing:
//!
//! * Windows has no system-wide cache of advertisements, so endpoints come
//!   from active scans, plus a small file remembering the last ones seen (a
//!   connected peripheral stops advertising; the file lets a second program,
//!   e.g. the GUI next to the tray, find an already connected set).
//! * There is no "disconnect" call: releasing every reference to the device
//!   lets Windows close the link.

pub mod discovery;
pub mod gatt;

pub use discovery::WinRt;
pub use gatt::WinGattLink;

use qcy_protocol::advert::MacAddress;
use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};
use windows::core::GUID;

use crate::Error;

impl From<windows::core::Error> for Error {
    fn from(e: windows::core::Error) -> Self {
        Error::Backend(format!("{} ({:#010x})", e.message(), e.code().0))
    }
}

/// A UUID string such as `0000a001-0000-1000-8000-00805f9b34fb` as a GUID.
pub(crate) fn guid(uuid: &str) -> GUID {
    let hex: String = uuid.chars().filter(|c| *c != '-').collect();
    GUID::from_u128(u128::from_str_radix(&hex, 16).expect("valid UUID constant"))
}

/// Copies the contents of a WinRT buffer.
pub(crate) fn buffer_bytes(buffer: &IBuffer) -> windows::core::Result<Vec<u8>> {
    let reader = DataReader::FromBuffer(buffer)?;
    let mut bytes = vec![0u8; reader.UnconsumedBufferLength()? as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}

/// Wraps bytes into a WinRT buffer.
pub(crate) fn to_buffer(bytes: &[u8]) -> windows::core::Result<IBuffer> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(bytes)?;
    writer.DetachBuffer()
}

/// The 48-bit address WinRT uses, as a [`MacAddress`].
pub(crate) fn mac_from_u64(address: u64) -> MacAddress {
    let b = address.to_be_bytes();
    MacAddress([b[2], b[3], b[4], b[5], b[6], b[7]])
}

#[cfg(test)]
fn mac_to_u64(mac: MacAddress) -> u64 {
    let m = mac.0;
    u64::from_be_bytes([0, 0, m[0], m[1], m[2], m[3], m[4], m[5]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_roundtrip() {
        let mac: MacAddress = "C4:AC:60:89:A4:B1".parse().unwrap();
        assert_eq!(mac_to_u64(mac), 0xC4AC_6089_A4B1);
        assert_eq!(mac_from_u64(0xC4AC_6089_A4B1), mac);
    }

    #[test]
    fn service_guid() {
        assert_eq!(
            guid(qcy_protocol::uuids::SERVICE).to_u128(),
            0x0000a001_0000_1000_8000_00805f9b34fb
        );
    }
}
