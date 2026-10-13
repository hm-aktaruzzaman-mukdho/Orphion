//! # qcy-protocol
//!
//! A pure (no I/O) implementation of the control protocol spoken by QCY true
//! wireless earbuds, reverse-engineered from the official *QCY* Android app
//! (v4.0.7, package `com.qcymall.earphonesetup`, library `com.qcymall.qcylibrary`).
//!
//! The earbuds expose a vendor GATT service (`0000a001-…`) over Bluetooth Low
//! Energy, separate from the classic A2DP/HFP audio link. Everything the phone
//! app can change is done through a handful of characteristics in that service:
//!
//! | Characteristic | Direction        | Content                                   |
//! |----------------|------------------|-------------------------------------------|
//! | `0x1001`       | write (no resp.) | framed *commands* (see [`frame`])         |
//! | `0x1002`       | read + notify    | framed *settings / replies*               |
//! | `0x0008`       | read + notify    | battery (3 raw bytes)                     |
//! | `0x0007`       | read             | firmware versions of both buds            |
//! | `0x000B`       | read + write     | legacy 10‑band EQ (preset id + gains)     |
//! | `0x000D`       | read + write     | touch gesture → action table              |
//!
//! This crate turns typed requests ([`Command`]) into bytes for those
//! characteristics and turns bytes coming back into typed [`Update`]s. It knows
//! nothing about Bluetooth stacks, which keeps it trivially unit-testable and
//! re-usable by any transport (BlueZ, a mock, a future Android/Windows port…).
//!
//! See `docs/PROTOCOL.md` at the repository root for the full write-up.

pub mod advert;
pub mod catalog;
pub mod command;
pub mod dialect;
pub mod eq;
pub mod frame;
pub mod ids;
pub mod panel;
pub mod profile;
pub mod samples;
pub mod state;
pub mod types;
pub mod update;
pub mod uuids;

mod error;

pub use command::{Command, PanelWrite, Write};
pub use error::ProtocolError;
pub use ids::CommandId;
pub use update::Update;
pub use uuids::Characteristic;
