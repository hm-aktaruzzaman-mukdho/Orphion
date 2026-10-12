//! Platform implementations of [`Backend`](crate::transport::Backend).
//!
//! Exactly one is compiled in, chosen by the build target, and exported as
//! [`Platform`]. To support another OS, add a module implementing the
//! `Backend` and `Link` traits and select it here.

#[cfg(target_os = "linux")]
pub mod bluez;
#[cfg(windows)]
pub mod winrt;

/// The Bluetooth backend for the target platform.
#[cfg(target_os = "linux")]
pub type Platform = bluez::Bluez;

/// The Bluetooth backend for the target platform.
#[cfg(windows)]
pub type Platform = winrt::WinRt;

#[cfg(not(any(target_os = "linux", windows)))]
compile_error!(
    "no Bluetooth backend for this platform yet: implement `Backend` and `Link` \
     in crates/qcy-client/src/backend/ (see the bluez and winrt modules)"
);
