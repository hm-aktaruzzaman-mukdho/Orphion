//! Linux backend: BlueZ over the system D-Bus (pure Rust via `zbus`; no
//! `libdbus` or root required).
//!
//! * [`discovery`]: finds the BLE endpoint of a headset ([`Bluez`] implements
//!   [`Backend`](crate::transport::Backend)).
//! * [`gatt`]: the GATT session ([`GattSession`] implements
//!   [`Link`](crate::transport::Link)).

pub mod discovery;
pub mod gatt;
mod proxies;

pub use discovery::Bluez;
pub use gatt::GattSession;

use crate::Error;

impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self {
        Error::Backend(e.to_string())
    }
}

impl From<zbus::fdo::Error> for Error {
    fn from(e: zbus::fdo::Error) -> Self {
        Error::Backend(e.to_string())
    }
}
