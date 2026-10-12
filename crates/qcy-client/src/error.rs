use qcy_protocol::ProtocolError;
use thiserror::Error;

/// Errors raised while finding, connecting to or talking to earbuds.
#[derive(Debug, Error)]
pub enum Error {
    /// The operating system's Bluetooth stack reported an error.
    #[error("Bluetooth error: {0}")]
    Backend(String),

    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("no Bluetooth adapter found")]
    NoAdapter,

    #[error("Bluetooth is turned off")]
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
            Error::Backend(_) | Error::Timeout(_) | Error::NotConnected | Error::DeviceGone
        )
    }

    /// The message used when no earbuds could be found.
    pub(crate) fn not_found() -> Self {
        Error::NotFound(
            "no QCY earbuds found. Take them out of the case and make sure \
             they are not connected to the QCY app on your phone."
                .into(),
        )
    }
}
