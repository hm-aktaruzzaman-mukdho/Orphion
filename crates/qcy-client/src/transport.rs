//! The seam between the generic client and an operating system's Bluetooth
//! stack.
//!
//! A [`Backend`] finds earbuds and opens a [`Link`]; a `Link` moves bytes to
//! and from the characteristics of the QCY control service. Everything above
//! this module ([`Earbuds`](crate::Earbuds), [`Manager`](crate::manager::Manager))
//! is written against these traits only, so adding a platform means
//! implementing them once (see `backend/`).

use std::future::Future;
use std::time::Duration;

use futures_util::stream::BoxStream;
use qcy_protocol::advert::{Advertisement, MacAddress};
use qcy_protocol::profile::{self, DeviceProfile};
use qcy_protocol::{Characteristic, Write};

use crate::Error;

/// Which earbuds to look for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Selector {
    /// Prefer earbuds that are connected for audio, then any paired ones,
    /// then anything QCY that is advertising.
    #[default]
    Auto,
    /// A specific set, by its classic **or** BLE address.
    Address(MacAddress),
}

/// A QCY BLE control endpoint found by a backend.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// Backend-specific handle: a D-Bus object path on Linux, the 48-bit
    /// address on Windows.
    pub id: String,
    pub ble_address: MacAddress,
    pub advertisement: Advertisement,
    /// Name of the matching classic (audio) device, if it is paired.
    pub name: Option<String>,
    /// Whether the matching classic device is connected for audio.
    pub audio_connected: bool,
    /// Whether the BLE control link is currently up.
    pub ble_connected: bool,
    /// Signal strength, present only while seen by an active scan.
    pub rssi: Option<i16>,
}

impl Endpoint {
    pub fn profile(&self) -> &'static DeviceProfile {
        profile::for_vendor(self.advertisement.vendor_id)
    }

    /// Best human readable name.
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.profile().name.to_string())
    }

    pub fn matches(&self, selector: &Selector) -> bool {
        match selector {
            Selector::Auto => true,
            Selector::Address(a) => {
                *a == self.ble_address || self.advertisement.matches_classic(*a)
            }
        }
    }

    /// Ranking used by [`Selector::Auto`]: higher is better.
    pub fn rank(&self) -> u8 {
        u8::from(self.ble_connected) * 8
            + u8::from(self.audio_connected) * 4
            + u8::from(self.name.is_some()) * 2
            + u8::from(self.rssi.is_some())
    }

    /// The best endpoint for `selector`. With `fresh_only`, endpoints that
    /// are neither connected nor currently advertising are ignored.
    pub fn pick(
        endpoints: Vec<Endpoint>,
        selector: &Selector,
        fresh_only: bool,
    ) -> Option<Endpoint> {
        endpoints
            .into_iter()
            .filter(|e| e.matches(selector))
            .filter(|e| e.ble_connected || !fresh_only || e.rssi.is_some())
            .max_by_key(Endpoint::rank)
    }
}

/// Data received from a characteristic notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub characteristic: Characteristic,
    pub data: Vec<u8>,
}

/// An operating system's Bluetooth stack.
pub trait Backend: Clone + Send + Sync + Sized + 'static {
    type Link: Link;

    /// Connects to the Bluetooth stack and checks that an adapter is usable.
    fn init() -> impl Future<Output = Result<Self, Error>> + Send;

    /// Finds the endpoint matching `selector`, scanning for up to `timeout`.
    /// Already connected endpoints are returned without scanning (connected
    /// BLE peripherals stop advertising).
    fn find(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> impl Future<Output = Result<Endpoint, Error>> + Send;

    /// Scans for `duration` and returns every QCY endpoint seen.
    fn scan(&self, duration: Duration)
    -> impl Future<Output = Result<Vec<Endpoint>, Error>> + Send;

    /// Connects to `endpoint` and resolves the control characteristics.
    fn open(&self, endpoint: &Endpoint) -> impl Future<Output = Result<Self::Link, Error>> + Send;
}

/// An open connection to the QCY control service (`0xA001`).
///
/// Implementations must only expose characteristics of that service: the
/// earbuds also have a firmware-update service whose characteristics reuse
/// the same UUIDs, and it must never be written to.
pub trait Link: Clone + Send + Sync + 'static {
    /// Largest value that fits into a single write.
    fn max_write_len(&self) -> usize;

    /// Whether the characteristic exists on this device.
    fn has(&self, c: Characteristic) -> bool;

    fn read(&self, c: Characteristic) -> impl Future<Output = Result<Vec<u8>, Error>> + Send;

    /// Writes without response, like the official app.
    fn write(&self, w: &Write) -> impl Future<Output = Result<(), Error>> + Send;

    /// Enables notifications and returns everything received.
    fn notifications(
        &self,
    ) -> impl Future<Output = Result<BoxStream<'static, Notification>, Error>> + Send;

    /// A stream that yields once the link goes down.
    fn disconnected(&self) -> impl Future<Output = Result<BoxStream<'static, ()>, Error>> + Send;

    /// Drops the control link (audio over classic Bluetooth is unaffected).
    fn disconnect(&self) -> impl Future<Output = Result<(), Error>> + Send;
}

/// Bytes as space separated hex, for logs.
pub(crate) fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
