//! # qcy-bluez
//!
//! Talks to QCY earbuds through BlueZ, the Linux Bluetooth daemon, over the
//! system D-Bus (pure Rust via `zbus`; no `libdbus` or root required).
//!
//! * [`discovery`] finds the BLE control endpoint that belongs to a headset.
//! * [`gatt`] is a thin GATT session: read / write / notifications.
//! * [`Earbuds`] is the high level client: it keeps a live
//!   [`DeviceState`](qcy_protocol::state::DeviceState) and executes
//!   [`Command`](qcy_protocol::Command)s.
//! * [`manager::Manager`] wraps `Earbuds` for long-running front-ends:
//!   background (re)connection with backoff and a `watch` channel of snapshots.
//!
//! ```no_run
//! # async fn demo() -> Result<(), qcy_bluez::Error> {
//! use qcy_bluez::{Bluez, Earbuds, discovery::Selector};
//! use qcy_protocol::{Command, types::AncMode};
//!
//! let bluez = Bluez::connect().await?;
//! let endpoint = bluez.find(&Selector::Auto, std::time::Duration::from_secs(10)).await?;
//! let buds = Earbuds::connect(&bluez, &endpoint).await?;
//! buds.send(Command::SetAnc(AncMode::Off)).await?;
//! println!("{:?}", buds.state().battery);
//! # Ok(()) }
//! ```

pub mod discovery;
pub mod gatt;
pub mod manager;

mod client;
mod error;
mod proxies;

pub use client::{CONFIRM_DELAY, ConnectionEvent, Earbuds};
pub use discovery::{Bluez, Endpoint};
pub use error::Error;
