use thiserror::Error;

/// Everything that can go wrong while encoding or decoding protocol data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtocolError {
    /// A frame did not start with the expected start-of-frame byte (`0xFF`).
    #[error("frame does not start with 0xFF (got {0:#04x})")]
    BadStartOfFrame(u8),

    /// The length byte of a frame disagrees with the number of bytes received.
    #[error("frame length mismatch: header says {declared} bytes, got {actual}")]
    LengthMismatch { declared: usize, actual: usize },

    /// A TLV entry claims more payload than is left in the frame.
    #[error("truncated TLV entry for command {id:#04x}")]
    TruncatedEntry { id: u8 },

    /// A buffer was too short to be decoded.
    #[error("payload too short: expected at least {expected} bytes, got {actual}")]
    TooShort { expected: usize, actual: usize },

    /// A value does not fit into the single length byte used by the protocol.
    #[error("payload of {0} bytes exceeds the protocol limit of 255")]
    PayloadTooLong(usize),

    /// A user supplied value is outside of what the device accepts.
    #[error("invalid value: {0}")]
    InvalidValue(String),
}
