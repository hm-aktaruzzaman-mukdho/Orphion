//! GATT services and characteristics used by QCY earbuds.
//!
//! All UUIDs are 16-bit values expanded with the Bluetooth base UUID
//! (`0000xxxx-0000-1000-8000-00805f9b34fb`). They are produced lower-case
//! because that is how BlueZ reports them over D-Bus.
//!
//! Two vendor services matter:
//!
//! * `0xA001`, the QCY control service, present on every model. WuQi,
//!   Realtek and the other "QCY protocol" chips do everything through it; ZR
//!   chips use its older single-purpose characteristics.
//! * `0xA002`, JieLi's RCSP channel as QCY configures it (write `0x0001`,
//!   notify `0x0002`). Only JieLi-based models have it.
//!
//! Some earbuds also expose a firmware-update service (WuQi `0x7033`) whose
//! characteristics re-use the UUIDs `0x1001`/`0x1002`. That is why a
//! [`Characteristic`] is always qualified by its [`Service`], and why the
//! update service has no representation here at all: it must never be used.

use std::fmt;

/// Client Characteristic Configuration Descriptor (used to enable notifications).
pub const CCCD: &str = "00002902-0000-1000-8000-00805f9b34fb";

/// Bluetooth SIG company identifiers found in QCY advertisements
/// (`BuildConfig.CompanyID = 21020`, fallback `21036` in the app).
pub const COMPANY_IDS: [u16; 2] = [0x521C, 0x522C];

/// Expands a 16-bit UUID with the Bluetooth base UUID.
pub fn expand(short: u16) -> String {
    format!("0000{short:04x}-0000-1000-8000-00805f9b34fb")
}

/// Extracts the 16-bit part of a UUID built on the Bluetooth base UUID.
pub fn shorten(uuid: &str) -> Option<u16> {
    let uuid = uuid.to_ascii_lowercase();
    let rest = uuid.strip_prefix("0000")?;
    let (short, base) = rest.split_at_checked(4)?;
    (base == "-0000-1000-8000-00805f9b34fb")
        .then(|| u16::from_str_radix(short, 16).ok())
        .flatten()
}

/// A vendor GATT service this crate talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Service {
    /// `0xA001`: QCY control service.
    Control,
    /// `0xA002`: JieLi RCSP channel.
    Rcsp,
}

impl Service {
    pub const ALL: [Service; 2] = [Service::Control, Service::Rcsp];

    pub const fn short(self) -> u16 {
        match self {
            Service::Control => 0xA001,
            Service::Rcsp => 0xA002,
        }
    }

    /// Full 128-bit UUID string (lower-case).
    pub fn uuid(self) -> String {
        expand(self.short())
    }

    /// Reverse lookup from a UUID string (case-insensitive).
    pub fn from_uuid(uuid: &str) -> Option<Self> {
        let short = shorten(uuid)?;
        Self::ALL.into_iter().find(|s| s.short() == short)
    }
}

/// A characteristic of one of the vendor [`Service`]s.
///
/// The well-known ones are associated constants; any other characteristic of
/// the two services can be named too (control panels reference them by UUID).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Characteristic {
    pub service: Service,
    /// 16-bit UUID of the characteristic.
    pub id: u16,
}

impl Characteristic {
    /// `0x1001`: framed commands (TLV protocol).
    pub const COMMAND: Self = Self::control(0x1001);
    /// `0x1002`: framed settings / replies, read + notify.
    pub const SETTINGS: Self = Self::control(0x1002);
    /// `0x0008`: battery levels, read + notify.
    pub const BATTERY: Self = Self::control(0x0008);
    /// `0x0007`: firmware versions, read.
    pub const VERSION: Self = Self::control(0x0007);
    /// `0x0009`: voice prompt language (older models), read + write.
    pub const LANGUAGE: Self = Self::control(0x0009);
    /// `0x000A`: reset / clear pairing / factory reset (older models), write.
    pub const RESET: Self = Self::control(0x000A);
    /// `0x000B`: graphic EQ (ZR: biquad coefficient blob), read + write.
    pub const EQ: Self = Self::control(0x000B);
    /// `0x000C`: clock synchronisation (older models), write.
    pub const TIME: Self = Self::control(0x000C);
    /// `0x000D`: touch key function table, read + write.
    pub const KEYS: Self = Self::control(0x000D);
    /// `0x000E`: ZR status / reset, read + write.
    pub const ZR_SETTING: Self = Self::control(0x000E);
    /// `0x000F`: in-ear sensor and monitor switches (older models).
    pub const FUNCTION: Self = Self::control(0x000F);
    /// JieLi RCSP: phone → earbuds.
    pub const RCSP_WRITE: Self = Self::new(Service::Rcsp, 0x0001);
    /// JieLi RCSP: earbuds → phone (notify).
    pub const RCSP_NOTIFY: Self = Self::new(Service::Rcsp, 0x0002);

    pub const fn new(service: Service, id: u16) -> Self {
        Self { service, id }
    }

    /// A characteristic of the control service `0xA001`.
    pub const fn control(id: u16) -> Self {
        Self::new(Service::Control, id)
    }

    /// Legacy per-gesture key characteristics `0x0001..=0x0006` of the
    /// control service (left single, right single, left double, …).
    pub const fn legacy_key(slot: u8) -> Self {
        Self::control(slot as u16)
    }

    /// Full 128-bit UUID string (lower-case).
    pub fn uuid(self) -> String {
        expand(self.id)
    }

    /// Short readable role, for logs and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Self::COMMAND => "command",
            Self::SETTINGS => "settings",
            Self::BATTERY => "battery",
            Self::VERSION => "version",
            Self::LANGUAGE => "language",
            Self::RESET => "reset",
            Self::EQ => "eq",
            Self::TIME => "time",
            Self::KEYS => "keys",
            Self::ZR_SETTING => "zr-setting",
            Self::FUNCTION => "function",
            Self::RCSP_WRITE => "rcsp-write",
            Self::RCSP_NOTIFY => "rcsp-notify",
            Self {
                service: Service::Control,
                id: 1..=6,
            } => "legacy-key",
            _ => "other",
        }
    }
}

impl fmt::Debug for Characteristic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04X}/{:04X}({})",
            self.service.short(),
            self.id,
            self.name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids() {
        assert_eq!(
            Characteristic::COMMAND.uuid(),
            "00001001-0000-1000-8000-00805f9b34fb"
        );
        assert_eq!(Service::Rcsp.uuid(), "0000a002-0000-1000-8000-00805f9b34fb");
        assert_eq!(
            shorten("0000000F-0000-1000-8000-00805F9B34FB"),
            Some(0x000F)
        );
        assert_eq!(shorten("6e400001-b5a3-f393-e0a9-e50e24dcca9e"), None);
        assert_eq!(
            Service::from_uuid("0000A001-0000-1000-8000-00805f9b34fb"),
            Some(Service::Control)
        );
        // The firmware-update service is deliberately unknown.
        assert_eq!(Service::from_uuid(&expand(0x7033)), None);
    }

    #[test]
    fn names() {
        assert_eq!(format!("{:?}", Characteristic::KEYS), "A001/000D(keys)");
        assert_eq!(Characteristic::legacy_key(3).name(), "legacy-key");
    }
}
