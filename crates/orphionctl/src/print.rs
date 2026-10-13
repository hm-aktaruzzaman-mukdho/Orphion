//! Human readable rendering of the device state, with the model's own names.

use qcy_client::Endpoint;
use qcy_protocol::panel::{EqControl, EqKind, ItemKind, Panel};
use qcy_protocol::state::DeviceState;
use qcy_protocol::types::BatteryLevel;
use qcy_protocol::{Characteristic, catalog};

use crate::controls::{self, slug};

pub fn endpoint(e: &Endpoint) {
    let id = e.advertisement.vendor_id;
    println!(
        "{}  ble={}  audio={}  model={id} ({})  battery L{} R{} C{}{}",
        e.display_name(),
        e.ble_address,
        e.advertisement.classic_address,
        catalog::name(id).unwrap_or("unknown model"),
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

/// Everything known about the connected earbuds.
pub fn status(
    name: &str,
    model: u16,
    panel: &Panel,
    s: &DeviceState,
    has: impl Fn(Characteristic) -> bool,
) {
    println!(
        "{name} ({}, model {model})",
        catalog::name(model).unwrap_or("unknown model")
    );
    if let Some(n) = &s.device_name {
        println!("  {:<24} {n}", "Bluetooth name");
    }
    if let Some(fw) = s.firmware {
        let v = |v: Option<qcy_protocol::types::Version>| v.map_or("–".into(), |v| v.to_string());
        println!("  {:<24} L {}  R {}", "Firmware", v(fw.left), v(fw.right));
    }
    if let Some(b) = s.battery {
        println!(
            "  {:<24} L {}  R {}  case {}",
            "Battery",
            battery(b.left),
            battery(b.right),
            battery(b.case)
        );
    }
    if let Some(anc) = controls::describe_anc(panel, s) {
        println!("  {:<24} {anc}", "Noise control");
    }
    if let Some(eq) = &panel.eq {
        self::eq(eq, s);
    }
    if panel.balance.is_some()
        && let Some(b) = s.balance
    {
        println!("  {:<24} {b} (50 = centre)", "Balance");
    }
    let settings = controls::settings(panel);
    if !settings.is_empty() {
        println!("  Settings (orphionctl set NAME VALUE)");
        for (slug, item) in settings {
            println!("    {slug:<28} {}", controls::value(item, s));
        }
    }
    keys(panel, s, has);
}

/// The active EQ preset and, for parametric EQs, its bands.
pub fn eq(eq: &EqControl, s: &DeviceState) {
    let active = match eq.kind {
        EqKind::Graphic => s.graphic_eq.as_ref().map(|g| g.preset),
        _ => s.active_eq_preset(),
    };
    let Some(id) = active else {
        return;
    };
    let name = eq.preset(id).map_or_else(
        || {
            if id >= 128 || id == 0 {
                "Custom".into()
            } else {
                format!("#{id}")
            }
        },
        |p| p.name.clone(),
    );
    println!("  {:<24} {name}", "Equalizer");
    match eq.kind {
        EqKind::Graphic => {
            if let Some(g) = &s.graphic_eq {
                let bands: Vec<String> = eq
                    .bands
                    .iter()
                    .zip(&g.gains)
                    .map(|(b, g)| format!("{} {g:+}", b.label))
                    .collect();
                println!("    {}", bands.join("  "));
            }
        }
        _ => {
            if let Some(p) = &s.parametric_eq {
                for b in p.active_bands() {
                    println!(
                        "    {:>5} Hz  {:+5.1} dB  Q {:.2}  type {}",
                        b.freq_hz, b.gain_db, b.q, b.filter
                    );
                }
                if p.master_gain_db != 0.0 {
                    println!("    master gain {:+.2} dB", p.master_gain_db);
                }
            }
        }
    }
}

/// Gestures with their current actions.
pub fn keys(panel: &Panel, s: &DeviceState, has: impl Fn(Characteristic) -> bool) {
    let gestures = controls::gestures(panel, has);
    if gestures.is_empty() {
        return;
    }
    println!("  Gestures (orphionctl keys set GESTURE ACTION)");
    for (name, slot) in gestures {
        let action = controls::key_action(slot, s).map_or("unknown".into(), |a| {
            slot.label_of(a)
                .map_or(format!("action {a}"), str::to_string)
        });
        println!("    {name:<28} {action}");
    }
}

/// What a model offers, without connecting.
pub fn panel(model: u16, panel: &Panel) {
    println!(
        "{} (model {model}, {:?} chip)",
        catalog::name(model).unwrap_or("unknown model"),
        qcy_protocol::advert::ChipFamily::from_vendor_id(model)
    );
    if panel.anc.is_some() {
        println!("\nNoise control (orphionctl anc MODE [SCENE] [--level N])");
        controls::list_anc(panel);
    }
    if let Some(eq) = &panel.eq {
        println!(
            "\nEqualizer: {:?}, {} bands, ±{} dB",
            eq.kind,
            eq.bands.len(),
            eq.max_db
        );
        for p in &eq.presets {
            println!("  {:>3}  {}", p.id, p.name);
        }
    }
    if let Some(eq) = &panel.game_eq {
        println!("\nGame EQ");
        for p in &eq.presets {
            println!("  {:>3}  {}", p.id, p.name);
        }
    }
    if panel.balance.is_some() {
        println!("\nBalance (orphionctl balance 0..100)");
    }
    let gestures = controls::gestures(panel, |_| true);
    if !gestures.is_empty() {
        println!("\nGestures (orphionctl keys set GESTURE ACTION)");
        for (name, slot) in gestures {
            let actions: Vec<String> = slot.choices.iter().map(|(_, l)| slug(l)).collect();
            println!("  {name:<28} {}", actions.join(" | "));
        }
    }
    let settings = controls::settings(panel);
    if !settings.is_empty() {
        println!("\nSettings (orphionctl set NAME VALUE)");
        for (name, item) in settings {
            println!("  {name:<28} {}", controls::accepted(item));
        }
    }
    let actions: Vec<&str> = panel
        .items
        .iter()
        .filter(|i| matches!(i.kind, ItemKind::Action(_)))
        .map(|i| i.title.as_str())
        .collect();
    if !actions.is_empty() {
        println!("\nReset (orphionctl reset … --yes): {}", actions.join(", "));
    }
}
