//! The connected model's controls, addressed by name on the command line.
//!
//! Every row of the model's control panel gets a *slug* derived from its
//! title ("LDAC Switch" → `ldac-switch`), and every value a slug of its label,
//! so `qcyctl set ldac-switch on` works for any model without this program
//! knowing about LDAC. Names can be shortened to any unambiguous prefix.

use anyhow::{Context, Result, bail};
use qcy_protocol::panel::{
    AncControl, Item, ItemKind, KeyControl, KeySlot, KeyTarget, ModeKind, Panel, anc_command,
};
use qcy_protocol::state::DeviceState;
use qcy_protocol::types::{PairedPhone, Side};
use qcy_protocol::{Characteristic, Command, CommandId, Write, dialect};

/// `"LDAC Switch"` → `"ldac-switch"`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Picks the entry whose slug equals `wanted`, or the only one it prefixes.
pub fn find<'a, T>(entries: &'a [(String, T)], wanted: &str, what: &str) -> Result<&'a T> {
    let wanted = slug(wanted);
    if let Some((_, v)) = entries.iter().find(|(s, _)| *s == wanted) {
        return Ok(v);
    }
    let matches: Vec<&(String, T)> = entries
        .iter()
        .filter(|(s, _)| s.starts_with(&wanted))
        .collect();
    match matches.as_slice() {
        [(_, v)] => Ok(v),
        [] => bail!(
            "unknown {what} {wanted:?}; choose from: {}",
            entries
                .iter()
                .map(|(s, _)| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        many => bail!(
            "{wanted:?} is ambiguous: {}",
            many.iter()
                .map(|(s, _)| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The settings `set` can change, with their slugs.
pub fn settings(panel: &Panel) -> Vec<(String, &Item)> {
    let mut out: Vec<(String, &Item)> = Vec::new();
    for item in &panel.items {
        if matches!(item.kind, ItemKind::Action(_)) {
            continue;
        }
        let mut s = slug(&item.title);
        if s.is_empty() {
            s = "setting".into();
        }
        let base = s.clone();
        let mut n = 2;
        while out.iter().any(|(existing, _)| *existing == s) {
            s = format!("{base}-{n}");
            n += 1;
        }
        out.push((s, item));
    }
    out
}

fn on_off(v: Option<bool>) -> String {
    match v {
        Some(true) => "on".into(),
        Some(false) => "off".into(),
        None => "unknown".into(),
    }
}

fn parse_on_off(value: &str) -> Result<bool> {
    match slug(value).as_str() {
        "on" | "true" | "1" | "yes" | "enable" | "enabled" => Ok(true),
        "off" | "false" | "0" | "no" | "disable" | "disabled" => Ok(false),
        other => bail!("expected on or off, got {other:?}"),
    }
}

/// A labelled option list as `(slug, value)`.
fn options<T: Clone>(list: &[(T, String)]) -> Vec<(String, T)> {
    list.iter().map(|(v, l)| (slug(l), v.clone())).collect()
}

/// The current value of a setting, for display.
pub fn value(item: &Item, s: &DeviceState) -> String {
    match &item.kind {
        ItemKind::Switch(sw) => on_off(sw.is_on(s)),
        ItemKind::Choice(c) => s
            .setting(c.setting)
            .and_then(|v| c.selected(v))
            .map_or("unknown".into(), |i| c.options[i].1.clone()),
        ItemKind::SleepTimer => match s.power_off_timer {
            Some(t) if t.minutes == 0 || t.minutes == u16::MAX => "off".into(),
            Some(t) => format!("{} min", t.minutes),
            None => "unknown".into(),
        },
        ItemKind::PromptVolume { levels } => s.prompt_volume.map_or("unknown".into(), |p| {
            levels
                .iter()
                .find(|(l, _)| *l == p.level)
                .map_or(format!("{} / {}", p.level, p.max), |(_, n)| n.clone())
        }),
        ItemKind::Language { languages } => {
            let code = s.prompt_language.clone().unwrap_or_default();
            languages
                .iter()
                .find(|(c, _)| c.eq_ignore_ascii_case(code.trim()))
                .map_or(
                    if code.is_empty() {
                        "unknown".into()
                    } else {
                        code
                    },
                    |(_, n)| n.clone(),
                )
        }
        ItemKind::LegacyLanguage { .. } => "–".into(),
        ItemKind::WearDetection(_) => s.wear_detection.map_or("unknown".into(), |w| {
            format!(
                "{}; playback: {}; noise control follows wear: {}",
                if w.enabled { "on" } else { "off" },
                w.music.label(),
                if w.anc_follows_wear { "yes" } else { "no" }
            )
        }),
        ItemKind::FitTest => match s.setting(CommandId::FitTest) {
            Some([2, l, r, ..]) => format!("result L {l} R {r} (3 = good seal)"),
            Some([1, ..]) => "testing".into(),
            _ => "not run".into(),
        },
        ItemKind::GameConfig { .. } => match s.setting(CommandId::GameConfig) {
            Some([e, sync, ..]) => format!(
                "{}{}",
                on_off(Some(*e == 1)),
                if *sync == 1 { ", game EQ synced" } else { "" }
            ),
            _ => "unknown".into(),
        },
        ItemKind::Multipoint => {
            let phones = s
                .setting(CommandId::PhoneInfo)
                .map(PairedPhone::parse_list)
                .unwrap_or_default();
            let mut v = on_off(s.setting_on(CommandId::DualDevice));
            for p in phones {
                v += &format!(
                    "\n      {} {} ({})",
                    if p.is_connected() { "●" } else { "○" },
                    if p.name.is_empty() { "phone" } else { &p.name },
                    p.address
                );
            }
            v
        }
        ItemKind::AiAnc => on_off(
            s.setting(CommandId::AiAnc)
                .and_then(|p| p.first())
                .map(|&v| v != 4),
        ),
        ItemKind::FocusMode => match s.setting_on(CommandId::FocusMode) {
            Some(true) => "focus".into(),
            Some(false) => "bluetooth".into(),
            None => "unknown".into(),
        },
        ItemKind::Volume => s.volume.map_or("unknown".into(), |v| {
            format!("L {} R {} (max {})", v.left, v.right, v.max)
        }),
        ItemKind::Rename { .. } => s.device_name.clone().unwrap_or_else(|| "unknown".into()),
        ItemKind::HeadGestures(_) => on_off(s.setting_on(CommandId::HeadAction)),
        ItemKind::Action(_) => String::new(),
    }
}

/// The accepted values of a setting, for help texts.
pub fn accepted(item: &Item) -> String {
    match &item.kind {
        ItemKind::Switch(_)
        | ItemKind::Multipoint
        | ItemKind::AiAnc
        | ItemKind::WearDetection(_)
        | ItemKind::HeadGestures(_) => "on | off".into(),
        ItemKind::GameConfig { .. } => "on | off | on+sync".into(),
        ItemKind::Choice(c) => c
            .options
            .iter()
            .map(|(_, l)| slug(l))
            .collect::<Vec<_>>()
            .join(" | "),
        ItemKind::SleepTimer => "off | MINUTES".into(),
        ItemKind::PromptVolume { levels } => levels
            .iter()
            .map(|(_, l)| slug(l))
            .collect::<Vec<_>>()
            .join(" | "),
        ItemKind::Language { languages } => languages
            .iter()
            .map(|(_, l)| slug(l))
            .collect::<Vec<_>>()
            .join(" | "),
        ItemKind::LegacyLanguage { languages } => languages
            .iter()
            .map(|(_, l)| slug(l))
            .collect::<Vec<_>>()
            .join(" | "),
        ItemKind::FitTest => "start | stop".into(),
        ItemKind::FocusMode => "bluetooth | focus".into(),
        ItemKind::Volume => "LEVEL".into(),
        ItemKind::Rename { max_len } => format!("NAME (up to {max_len} bytes)"),
        ItemKind::Action(_) => String::new(),
    }
}

/// Whether changing the setting this way restarts the earbuds.
pub fn restarts(item: &Item) -> bool {
    match &item.kind {
        ItemKind::Switch(sw) => matches!(
            sw.setting,
            Some(CommandId::Ldac | CommandId::DualDevice | CommandId::Aptx)
        ),
        ItemKind::GameConfig { .. } | ItemKind::Multipoint => true,
        _ => false,
    }
}

/// The command that sets `item` to `value`.
pub fn command(item: &Item, value: &str, s: &DeviceState) -> Result<Command> {
    Ok(match &item.kind {
        ItemKind::Switch(sw) => sw.command(parse_on_off(value)?),
        ItemKind::Choice(c) => {
            let opts: Vec<(String, Vec<u8>)> = c
                .options
                .iter()
                .map(|(p, l)| (slug(l), p.clone()))
                .collect();
            Command::SetValue {
                id: c.setting,
                payload: find(&opts, value, "value")?.clone(),
            }
        }
        ItemKind::SleepTimer => Command::SetPowerOffTimer(match slug(value).as_str() {
            "off" | "never" => 0,
            v => v.parse().context("expected minutes or `off`")?,
        }),
        ItemKind::PromptVolume { levels } => match value.parse::<u8>() {
            Ok(level) => Command::SetPromptVolume(level),
            Err(_) => Command::SetPromptVolume(*find(&options(levels), value, "volume")?),
        },
        ItemKind::Language { languages } => {
            let by_name: Vec<(String, String)> = languages
                .iter()
                .map(|(code, l)| (slug(l), code.clone()))
                .collect();
            let code = match find(&by_name, value, "language") {
                Ok(code) => code.clone(),
                Err(_) if value.len() == 4 && value.is_ascii() => value.to_uppercase(),
                Err(e) => return Err(e),
            };
            Command::SetPromptLanguage(code)
        }
        ItemKind::LegacyLanguage { languages } => Command::Raw(Write {
            characteristic: Characteristic::LANGUAGE,
            data: vec![*find(&options(languages), value, "language")?],
        }),
        ItemKind::WearDetection(_) => {
            let mut w = s
                .wear_detection
                .context("the wear detection setting has not been reported yet")?;
            w.enabled = parse_on_off(value)?;
            Command::SetWearDetection(w)
        }
        ItemKind::FitTest => Command::SetValue {
            id: CommandId::FitTest,
            payload: match slug(value).as_str() {
                "start" | "on" => vec![1, 1, 1],
                "stop" | "off" => vec![0, 0, 0],
                other => bail!("expected start or stop, got {other:?}"),
            },
        },
        ItemKind::GameConfig { .. } => {
            let v = slug(value);
            let (on, sync) = match v.as_str() {
                "on-sync" | "sync" => (true, true),
                other => (parse_on_off(other)?, false),
            };
            Command::SetValue {
                id: CommandId::GameConfig,
                payload: vec![u8::from(on), u8::from(sync)],
            }
        }
        ItemKind::Multipoint => Command::SetValue {
            id: CommandId::DualDevice,
            payload: vec![u8::from(parse_on_off(value)?)],
        },
        ItemKind::AiAnc => {
            let on = parse_on_off(value)?;
            Command::SetValue {
                id: CommandId::AiAnc,
                payload: vec![if on { 0 } else { 4 }, 0, 0, 0, 0, 1],
            }
        }
        ItemKind::FocusMode => Command::SetValue {
            id: CommandId::FocusMode,
            payload: vec![match slug(value).as_str() {
                "focus" | "sd" => 1,
                "bluetooth" | "bt" => 0,
                other => bail!("expected bluetooth or focus, got {other:?}"),
            }],
        },
        ItemKind::Volume => {
            let level: u8 = value.parse().context("expected a volume level")?;
            Command::SetVolume {
                left: level,
                right: level,
            }
        }
        ItemKind::Rename { .. } => Command::SetDeviceName(value.to_string()),
        ItemKind::HeadGestures(_) => Command::SetValue {
            id: CommandId::HeadAction,
            payload: vec![u8::from(parse_on_off(value)?)],
        },
        ItemKind::Action(_) => bail!("use `qcyctl reset` for this"),
    })
}

// ---------------------------------------------------------------------------
// Noise control
// ---------------------------------------------------------------------------

/// Describes the current noise control with the panel's names.
pub fn describe_anc(panel: &Panel, s: &DeviceState) -> Option<String> {
    Some(match panel.anc.as_ref()? {
        AncControl::Scenes(a) => {
            let v = dialect::current_anc(s)?;
            let Some(mode) = a.modes.iter().find(|m| m.contains(v)) else {
                return Some(format!("unknown ({v:#08x})"));
            };
            match &mode.kind {
                ModeKind::Scenes(subs) => match subs.iter().find(|sub| sub.contains(v)) {
                    Some(sub) if sub.max_level() > 0 => {
                        format!("{} · {} level {}", mode.name, sub.name, v - sub.start + 1)
                    }
                    Some(sub) => format!("{} · {}", mode.name, sub.name),
                    None => mode.name.clone(),
                },
                ModeKind::Level { voice: Some(x) } if *x == v => {
                    format!("{} · voice focus", mode.name)
                }
                ModeKind::Level { .. } => {
                    format!("{} · level {}", mode.name, v.saturating_sub(mode.start))
                }
                ModeKind::Ruler => format!("{} · {}", mode.name, v & 0xFF),
                ModeKind::Button => mode.name.clone(),
            }
        }
        AncControl::Legacy(a) => {
            let m = *s.setting(CommandId::NoiseMode)?.first()?;
            a.modes
                .iter()
                .find(|(_, id)| *id == m)
                .map_or(format!("mode {m}"), |(n, _)| n.clone())
        }
        AncControl::Percent(a) => {
            let st = s.anc?.to_setting();
            let mode = a.modes.iter().find(|m| m.mode == st.mode)?;
            if mode.max_percent > 0 {
                format!("{} · {} %", mode.name, st.level)
            } else {
                mode.name.clone()
            }
        }
    })
}

/// Lists the noise control modes, scenes and levels.
pub fn list_anc(panel: &Panel) {
    let Some(anc) = &panel.anc else {
        println!("This model has no adjustable noise control.");
        return;
    };
    match anc {
        AncControl::Scenes(a) => {
            for m in &a.modes {
                match &m.kind {
                    ModeKind::Scenes(subs) => {
                        println!("{}", slug(&m.name));
                        for sub in subs {
                            let levels = if sub.max_level() > 0 {
                                format!("  --level 1..{}", sub.max_level() + 1)
                            } else {
                                String::new()
                            };
                            println!("    {}{levels}", slug(&sub.name));
                        }
                    }
                    ModeKind::Level { voice } => println!(
                        "{}  --level 1..{}{}",
                        slug(&m.name),
                        m.levels().count(),
                        if voice.is_some() {
                            "  (or scene `voice`)"
                        } else {
                            ""
                        }
                    ),
                    ModeKind::Ruler => {
                        let r = m.levels();
                        println!(
                            "{}  --level {}..{}",
                            slug(&m.name),
                            r.start() & 0xFF,
                            r.end() & 0xFF
                        );
                    }
                    ModeKind::Button => println!("{}", slug(&m.name)),
                }
            }
        }
        AncControl::Legacy(a) => a.modes.iter().for_each(|(n, _)| println!("{}", slug(n))),
        AncControl::Percent(a) => {
            for m in &a.modes {
                if m.max_percent > 0 {
                    println!("{}  --level 0..{} (percent)", slug(&m.name), m.max_percent);
                } else {
                    println!("{}", slug(&m.name));
                }
            }
        }
    }
}

/// The command for `anc MODE [SCENE] [--level N]` (levels count from 1).
pub fn anc(panel: &Panel, mode: &str, scene: Option<&str>, level: Option<u32>) -> Result<Command> {
    let anc = panel
        .anc
        .as_ref()
        .context("this model has no adjustable noise control")?;
    Ok(match anc {
        AncControl::Scenes(a) => {
            let modes: Vec<(String, _)> = a.modes.iter().map(|m| (slug(&m.name), m)).collect();
            let m = *find(&modes, mode, "mode")?;
            match &m.kind {
                ModeKind::Scenes(subs) => {
                    let Some(scene) = scene else {
                        return Ok(anc_command(m.default));
                    };
                    let subs: Vec<(String, _)> = subs.iter().map(|s| (slug(&s.name), s)).collect();
                    let sub = *find(&subs, scene, "scene")?;
                    let value = match level {
                        Some(l) if (1..=sub.max_level() + 1).contains(&l) => sub.start + l - 1,
                        Some(l) => bail!(
                            "{} has levels 1..{}, not {l}",
                            sub.name,
                            sub.max_level() + 1
                        ),
                        None => sub.default.clamp(sub.start, sub.end),
                    };
                    anc_command(value)
                }
                ModeKind::Level { voice } => {
                    if scene.map(slug).as_deref() == Some("voice") {
                        anc_command(voice.context("this mode has no voice focus")?)
                    } else {
                        let levels = m.levels();
                        let value = match level {
                            Some(l) => m.start + l,
                            None => m.default.max(*levels.start()),
                        };
                        if !levels.contains(&value) {
                            bail!("levels are 1..{}", levels.count());
                        }
                        anc_command(value)
                    }
                }
                ModeKind::Ruler => {
                    let levels = m.levels();
                    let value = level.map_or(m.default, |l| (m.default & 0xFF_FF00) + l);
                    if !levels.contains(&value) {
                        bail!(
                            "levels are {}..{}",
                            levels.start() & 0xFF,
                            levels.end() & 0xFF
                        );
                    }
                    anc_command(value)
                }
                ModeKind::Button => anc_command(m.default),
            }
        }
        AncControl::Legacy(a) => {
            let modes: Vec<(String, u8)> = a.modes.iter().map(|(n, id)| (slug(n), *id)).collect();
            Command::SetNoiseMode(*find(&modes, mode, "mode")?)
        }
        AncControl::Percent(a) => {
            let modes: Vec<(String, _)> = a.modes.iter().map(|m| (slug(&m.name), m)).collect();
            let m = *find(&modes, mode, "mode")?;
            let percent = level
                .unwrap_or(u32::from(m.max_percent))
                .min(u32::from(m.max_percent));
            anc_command(m.packed(percent as u8))
        }
    })
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// Every configurable gesture with a slug such as `left-double-touch`.
pub fn gestures<'a>(
    panel: &'a Panel,
    has: impl Fn(Characteristic) -> bool,
) -> Vec<(String, &'a KeySlot)> {
    let mut out = Vec::new();
    let mut add = |control: &'a KeyControl, prefix: &str| {
        for group in &control.groups {
            for row in &group.rows {
                for slot in &row.slots {
                    let side = match slot.side {
                        Some(Side::Left) => "left ",
                        Some(Side::Right) => "right ",
                        None => "",
                    };
                    let group_name = if slot.side.is_none() {
                        group.name.as_str()
                    } else {
                        ""
                    };
                    let name = slug(&format!("{prefix} {group_name} {side}{}", row.name));
                    out.push((name, slot));
                }
            }
        }
    };
    if let Some(k) = panel.keys_for(&has) {
        add(k, "");
    }
    if let Some(k) = &panel.buttons {
        add(k, "");
    }
    for item in &panel.items {
        if let ItemKind::HeadGestures(k) = &item.kind {
            add(k, "head");
        }
    }
    out
}

/// The action bound to a slot.
pub fn key_action(slot: &KeySlot, s: &DeviceState) -> Option<u8> {
    if let KeyTarget::Setting(id) = slot.target {
        return s.setting(id).and_then(|p| {
            p.chunks_exact(2)
                .find(|pair| u16::from(pair[0]) == slot.event)
                .map(|pair| pair[1])
        });
    }
    s.key_bindings
        .iter()
        .find(|b| b.gesture_id == slot.event)
        .map(|b| {
            if b.action.id() == 0xFF {
                0
            } else {
                b.action.id()
            }
        })
}

/// The command binding `action` (a label slug or number) to `slot`.
pub fn key_command(slot: &KeySlot, action: &str) -> Result<Command> {
    let action = match action.parse::<u8>() {
        Ok(a) => a,
        Err(_) => *find(&options(&slot.choices), action, "action")?,
    };
    Ok(Command::SetKey {
        target: slot.target,
        event: slot.event,
        action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcy_protocol::catalog;

    #[test]
    fn slugs() {
        assert_eq!(slug("LDAC Switch"), "ldac-switch");
        assert_eq!(
            slug("  Take off earbud to pause!"),
            "take-off-earbud-to-pause"
        );
    }

    #[test]
    fn melobuds_pro_by_name() {
        let panel = catalog::panel(19786);
        let s = DeviceState::default();
        let settings = settings(&panel);
        let ldac = find(&settings, "ldac", "setting").unwrap();
        assert!(matches!(
            command(ldac, "on", &s).unwrap(),
            Command::Panel(_)
        ));
        let cmd = anc(&panel, "noise", Some("commuting"), Some(2)).unwrap();
        assert_eq!(cmd.encode().unwrap().data, [0xFF, 5, 0x17, 3, 1, 2, 1]);
        let cmd = anc(&panel, "transparency", Some("voice"), None).unwrap();
        assert_eq!(cmd.encode().unwrap().data, [0xFF, 5, 0x17, 3, 3, 2, 0]);
        let g = gestures(&panel, |_| true);
        let slot = *find(&g, "right-double", "gesture").unwrap();
        assert_eq!(slot.event, 4);
        assert!(key_command(slot, "next").is_ok());
    }
}
