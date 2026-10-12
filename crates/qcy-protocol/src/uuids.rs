//! GATT UUIDs used by QCY earbuds.
//!
//! All UUIDs are 16-bit values expanded with the Bluetooth base UUID. They are
//! stored lower-case because that is how BlueZ reports them over D-Bus.

/// Primary vendor service that hosts all control characteristics.
pub const SERVICE: &str = "0000a001-0000-1000-8000-00805f9b34fb";

/// Client Characteristic Configuration Descriptor (used to enable notifications).
pub const CCCD: &str = "00002902-0000-1000-8000-00805f9b34fb";

/// Bluetooth SIG company identifiers found in QCY advertisements
/// (`BuildConfig.CompanyID = 21020`, fallback `21036` in the app).
pub const COMPANY_IDS: [u16; 2] = [0x521C, 0x522C];

/// The characteristics of the [`SERVICE`] the protocol uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Characteristic {
    /// `0x1001` – framed commands, write without response.
    Command,
    /// `0x1002` – framed settings / replies, read + notify.
    Settings,
    /// `0x0008` – battery levels, read + notify.
    Battery,
    /// `0x0007` – firmware versions, read.
    Version,
    /// `0x000B` – legacy graphic EQ, read + write.
    Eq,
    /// `0x000D` – touch key function table, read + write.
    Keys,
}

impl Characteristic {
    /// Every characteristic, in the order a client normally reads them.
    pub const ALL: [Characteristic; 6] = [
        Characteristic::Version,
        Characteristic::Battery,
        Characteristic::Settings,
        Characteristic::Keys,
        Characteristic::Eq,
        Characteristic::Command,
    ];

    /// Full 128-bit UUID string (lower-case).
    pub const fn uuid(self) -> &'static str {
        match self {
            Characteristic::Command => "00001001-0000-1000-8000-00805f9b34fb",
            Characteristic::Settings => "00001002-0000-1000-8000-00805f9b34fb",
            Characteristic::Battery => "00000008-0000-1000-8000-00805f9b34fb",
            Characteristic::Version => "00000007-0000-1000-8000-00805f9b34fb",
            Characteristic::Eq => "0000000b-0000-1000-8000-00805f9b34fb",
            Characteristic::Keys => "0000000d-0000-1000-8000-00805f9b34fb",
        }
    }

    /// Reverse lookup from a UUID string (case-insensitive).
    pub fn from_uuid(uuid: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.uuid().eq_ignore_ascii_case(uuid))
    }

    /// Whether the device sends notifications on this characteristic.
    pub const fn notifies(self) -> bool {
        matches!(self, Characteristic::Settings | Characteristic::Battery)
    }
}
