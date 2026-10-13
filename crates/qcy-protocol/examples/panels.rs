//! Prints what the bundled control panel of each model offers.
//!
//! ```text
//! cargo run -p qcy-protocol --example panels            # every model
//! cargo run -p qcy-protocol --example panels -- 19786   # one model, in detail
//! ```

use qcy_protocol::catalog;
use qcy_protocol::panel::{AncControl, ItemKind, Panel};

fn summary(p: &Panel) -> String {
    let mut parts = Vec::new();
    match &p.anc {
        Some(AncControl::Scenes(a)) => parts.push(format!("anc:{}", a.modes.len())),
        Some(AncControl::Legacy(a)) => parts.push(format!("anc-legacy:{}", a.modes.len())),
        Some(AncControl::Percent(a)) => parts.push(format!("anc-percent:{}", a.modes.len())),
        None => {}
    }
    if let Some(eq) = &p.eq {
        parts.push(format!("eq:{:?}/{}", eq.kind, eq.presets.len()));
    }
    if p.game_eq.is_some() {
        parts.push("game-eq".into());
    }
    for (name, keys) in [
        ("keys", &p.touch_keys),
        ("keys-v1", &p.legacy_keys),
        ("buttons", &p.buttons),
    ] {
        if let Some(k) = keys {
            parts.push(format!("{name}:{}", k.slots().count()));
        }
    }
    if p.balance.is_some() {
        parts.push("balance".into());
    }
    let kinds: Vec<&str> = p
        .items
        .iter()
        .map(|i| match &i.kind {
            ItemKind::Switch(_) => "switch",
            ItemKind::Action(_) => "action",
            ItemKind::Choice(_) => "choice",
            ItemKind::Rename { .. } => "rename",
            ItemKind::SleepTimer => "sleep",
            ItemKind::PromptVolume { .. } => "tone",
            ItemKind::Language { .. } => "lang",
            ItemKind::LegacyLanguage { .. } => "lang-v1",
            ItemKind::WearDetection(_) => "wear",
            ItemKind::FitTest => "fit",
            ItemKind::GameConfig { .. } => "game",
            ItemKind::Multipoint => "multipoint",
            ItemKind::AiAnc => "ai-anc",
            ItemKind::FocusMode => "focus",
            ItemKind::Volume => "volume",
        })
        .collect();
    parts.push(format!("items[{}]", kinds.join(",")));
    parts.join(" ")
}

fn main() {
    match std::env::args().nth(1).and_then(|a| a.parse::<u16>().ok()) {
        Some(id) => {
            println!("{}", catalog::name(id).unwrap_or("unknown model"));
            println!("{:#?}", catalog::panel(id));
        }
        None => {
            for m in catalog::models() {
                let p = catalog::panel(m.id);
                println!(
                    "{:6} {:8} {:32} {}",
                    m.id,
                    format!("{:?}", m.chip),
                    m.name,
                    summary(&p)
                );
            }
        }
    }
}
