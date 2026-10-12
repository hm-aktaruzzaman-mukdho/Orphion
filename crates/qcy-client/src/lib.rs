//! # qcy-client
//!
//! Finds QCY earbuds, connects to their BLE control service and keeps a live
//! view of their settings, on top of the operating system's Bluetooth stack.
//!
//! * [`transport`]: the [`Backend`](transport::Backend) / [`Link`](transport::Link)
//!   traits every platform implements.
//! * [`backend`]: the implementations: BlueZ over D-Bus on Linux, the WinRT
//!   Bluetooth LE API on Windows. [`Platform`] is the one for the build target.
//! * [`Earbuds`]: stateful client; keeps a live
//!   [`DeviceState`](qcy_protocol::state::DeviceState) and executes
//!   [`Command`](qcy_protocol::Command)s.
//! * [`manager::Manager`]: wraps `Earbuds` for long-running front-ends:
//!   background (re)connection with backoff and a `watch` channel of snapshots.
//!
//! ```no_run
//! # async fn demo() -> Result<(), qcy_client::Error> {
//! use qcy_client::transport::{Backend, Selector};
//! use qcy_client::{Earbuds, Platform};
//! use qcy_protocol::{Command, types::AncMode};
//!
//! let platform = Platform::init().await?;
//! let endpoint = platform.find(&Selector::Auto, std::time::Duration::from_secs(10)).await?;
//! let buds = Earbuds::connect(&platform, &endpoint).await?;
//! buds.send(Command::SetAnc(AncMode::Off)).await?;
//! println!("{:?}", buds.state().battery);
//! # Ok(()) }
//! ```

pub mod backend;
pub mod manager;
pub mod transport;

mod client;
mod error;

pub use backend::Platform;
pub use client::{CONFIRM_DELAY, ConnectionEvent, Earbuds};
pub use error::Error;
pub use transport::{Endpoint, Selector};

/// The [`Link`](transport::Link) type of the current platform.
pub type PlatformLink = <Platform as transport::Backend>::Link;
