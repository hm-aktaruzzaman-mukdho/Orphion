//! Human readable rendering of the device state.

use qcy_bluez::Endpoint;
use qcy_protocol::profile::DeviceProfile;
use qcy_protocol::state::DeviceState;
use qcy_protocol::types::{BatteryLevel, Gesture, Toggle};

pub fn endpoint(e: &Endpoint) {
    println!(
        "{}  ble={}  audio={}  model={:#06x} ({})  battery L{} R{} C{}{}",
        e.display_name(),
        e.ble_address,
        e.advertisement.classic_address,
        e.advertisement.vendor_id,
        e.profile().name,
        battery(e.advertisement.battery.left),
        battery(e.advertisement.battery.right),
        battery(e.advertisement.battery.case),
        e.rssi.map(|r| format!("  rssi={r}")).unwrap_or_default(),
    );
}

pub fn battery(b: Option<BatteryLevel>) -> String {
    match b {
        Some(b) if b.charging => format!("{}%⚡", b.percent),
        Some(b) => format!("{}%", b.percent),
        None => "–".into(),
    }
}

pub fn status(name: &str, profile: &DeviceProfile, s: &DeviceState) {
    println!("{name} ({})", profile.name);
    if let Some(n) = &s.device_name {
        println!("  Bluetooth name   {n}");
    }
    if let Some(fw) = s.firmware {
        let v = |v: Option<qcy_protocol::types::Version>| v.map_or("–".into(), |v| v.to_string());
        println!("  Firmware         L {}  R {}", v(fw.left), v(fw.right));
    }
    if let Some(b) = s.battery {
        println!(
            "  Battery          L {}  R {}  case {}",
            battery(b.left),
            battery(b.right),
            battery(b.case)
        );
    }
    if let Some(a) = s.anc {
        println!("  Noise control    {}", a.describe());
    }
    eq(profile, s);
    for t in Toggle::ALL {
        if let Some(on) = s.toggle(t) {
            println!("  {:<16} {}", t.label(), if on { "on" } else { "off" });
        }
    }
    if let Some(b) = s.balance {
        println!("  Balance          {b} (50 = centre)");
    }
    if let Some(v) = s.volume {
        println!(
            "  Media volume     L {}  R {}  (max {})",
            v.left, v.right, v.max
        );
    }
    if let Some(p) = s.prompt_volume {
        println!("  Prompt volume    {} / {}", p.level, p.max);
    }
    if let Some(l) = &s.prompt_language {
        let label = profile
            .prompt_languages
            .iter()
            .find(|c| c.value == l)
            .map_or(l.as_str(), |c| c.label);
        println!("  Prompt language  {label}");
    }
    if let Some(w) = s.wear_detection {
        println!(
            "  Wear detection   {}; music: {}; ANC follows wear: {}",
            if w.enabled { "on" } else { "off" },
            w.music.label(),
            if w.anc_follows_wear { "yes" } else { "no" }
        );
    }
    if let Some(a) = s.auto_power_off {
        println!("  Auto power off   {} after disconnect", a.label());
    }
    if let Some(t) = s.power_off_timer {
        if t.minutes == 0 || t.minutes == u16::MAX {
            println!("  Sleep timer      off");
        } else {
            println!("  Sleep timer      {} min", t.minutes);
        }
    }
    keys(s);
    if !s.unknown.is_empty() {
        println!("  Undecoded        {}", s.unknown.len());
        for (id, payload) in &s.unknown {
            let hex: Vec<String> = payload.iter().map(|b| format!("{b:02x}")).collect();
            println!("    {:#04x}: {}", id, hex.join(" "));
        }
    }
}

pub fn eq(profile: &DeviceProfile, s: &DeviceState) {
    let Some(id) = s.active_eq_preset() else {
        return;
    };
    let name = profile
        .eq_preset(id)
        .map(|p| p.name.to_string())
        .unwrap_or_else(|| {
            if id >= 128 {
                "Custom".into()
            } else {
                format!("#{id}")
            }
        });
    println!("  Equalizer        {name}");
    if let Some(eq) = &s.parametric_eq {
        for b in eq.active_bands() {
            println!(
                "    {:>5} Hz  {:+5.1} dB  Q {:.2}  type {}",
                b.freq_hz, b.gain_db, b.q, b.filter
            );
        }
        if eq.master_gain_db != 0.0 {
            println!("    master gain {:+.2} dB", eq.master_gain_db);
        }
    }
}

pub fn keys(s: &DeviceState) {
    if s.key_bindings.is_empty() {
        return;
    }
    println!("  Touch controls");
    for b in &s.key_bindings {
        let gesture = b
            .gesture()
            .map(Gesture::label)
            .unwrap_or_else(|| format!("gesture {}", b.gesture_id));
        println!("    {:<24} {}", gesture, b.action.label());
    }
}
