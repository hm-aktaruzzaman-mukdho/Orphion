//! Minimal `zbus` proxies for the BlueZ D-Bus API
//! (see `doc/org.bluez.*.rst` in the BlueZ source tree).

use std::collections::HashMap;

use zbus::proxy;
use zbus::zvariant::{OwnedValue, Value};

pub const SERVICE: &str = "org.bluez";
pub const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
pub const DEVICE_IFACE: &str = "org.bluez.Device1";
pub const GATT_SERVICE_IFACE: &str = "org.bluez.GattService1";
pub const GATT_CHAR_IFACE: &str = "org.bluez.GattCharacteristic1";

#[proxy(interface = "org.bluez.Adapter1", default_service = "org.bluez")]
pub trait Adapter1 {
    fn set_discovery_filter(&self, filter: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    fn start_discovery(&self) -> zbus::Result<()>;
    fn stop_discovery(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;
}

#[proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
pub trait Device1 {
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn services_resolved(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn manufacturer_data(&self) -> zbus::Result<HashMap<u16, OwnedValue>>;
}

#[proxy(
    interface = "org.bluez.GattCharacteristic1",
    default_service = "org.bluez"
)]
pub trait GattCharacteristic1 {
    fn read_value(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<Vec<u8>>;
    fn write_value(&self, value: &[u8], options: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    fn start_notify(&self) -> zbus::Result<()>;
    fn stop_notify(&self) -> zbus::Result<()>;

    #[zbus(property, name = "MTU")]
    fn mtu(&self) -> zbus::Result<u16>;
    #[zbus(property)]
    fn flags(&self) -> zbus::Result<Vec<String>>;
}
