use qcy_protocol::ProtocolError;
use thiserror::Error;

/// Errors raised by the BlueZ transport.
#[derive(Debug, Error)]
pub enum Error {
    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    #[error("BlueZ error: {0}")]
    Bluez(#[from] zbus::fdo::Error),

    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("no Bluetooth adapter found (is bluetoothd running?)")]
    NoAdapter,

    #[error("the Bluetooth adapter is powered off")]
    AdapterOff,

    #[error("{0}")]
    NotFound(String),

    #[error("timed out while {0}")]
    Timeout(&'static str),

    #[error("the connection attempt was aborted; the earbuds need to be found again")]
    DeviceGone,

    #[error("the earbuds are not connected")]
    NotConnected,

    #[error("command of {len} bytes does not fit the link MTU ({max} bytes)")]
    TooLarge { len: usize, max: usize },
}

impl Error {
    /// Whether retrying (e.g. reconnecting) may help.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Error::DBus(_)
                | Error::Bluez(_)
                | Error::Timeout(_)
                | Error::NotConnected
                | Error::DeviceGone
        )
    }
}
