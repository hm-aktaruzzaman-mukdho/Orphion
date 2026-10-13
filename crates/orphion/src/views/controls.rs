//! Touch gestures and headphone buttons, with the actions each model's
//! panel offers.

use eframe::egui::Ui;
use qcy_protocol::panel::{ItemKind, KeyControl, KeySlot, KeyTarget};
use qcy_protocol::types::Side;
use qcy_protocol::{Command, CommandId};

use crate::app::ViewCtx;
use crate::material::components as m3;
use crate::material::{MaterialIcon, Scheme, icons, text};
use crate::views::dropdown;

/// An icon for a gesture row, from its (English) name.
fn gesture_icon(name: &str) -> MaterialIcon {
    let n = name.to_lowercase();
    if n.contains("double") {
        icons::ICON_LOOKS_TWO
    } else if n.contains("tri") || n.contains("three") {
        icons::ICON_LOOKS_3
    } else if n.contains("four") || n.contains("quad") {
        icons::ICON_LOOKS_4
    } else if n.contains("long") || n.contains("hold") || n.contains("press") {
        icons::ICON_TOUCH_APP
    } else {
        icons::ICON_LOOKS_ONE
    }
}

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let panel = view.panel;
    let device = view.device();
    let keys = panel.keys_for(|c| device.is_none_or(|d| d.has(c)));
    if let Some(keys) = keys {
        key_card(ui, view, keys, icons::ICON_TOUCH_APP);
    }
    if let Some(buttons) = &panel.buttons {
        key_card(ui, view, buttons, icons::ICON_HEADPHONES);
    }
    if let Some(keys) = keys.filter(|k| !k.fixed.is_empty()) {
        m3::titled_card(ui, icons::ICON_CALL, "Calls", |ui| {
            m3::supporting(ui, "Fixed by the firmware:");
            let s = Scheme::of(ui);
            for g in &keys.fixed {
                let action = if g.left == g.right || g.right.is_empty() {
                    g.left.clone()
                } else {
                    format!("L: {}  ·  R: {}", g.left, g.right)
                };
                m3::setting_row(
                    ui,
                    gesture_icon(&g.gesture),
                    &g.gesture,
                    None,
                    220.0,
                    |ui| {
                        ui.label(text::body_medium(action).color(s.on_surface_variant));
                    },
                );
            }
        });
    }
    let head = panel.items.iter().find_map(|i| match &i.kind {
        ItemKind::HeadGestures(k) => Some((i, k)),
        _ => None,
    });
    if let Some((item, head)) = head {
        let on = view
            .snap
            .state
            .setting_on(CommandId::HeadAction)
            .unwrap_or(false);
        m3::card(ui, |ui| {
            if let Some(new) = m3::switch_row(
                ui,
                icons::ICON_ACCESSIBILITY_NEW,
                &item.title,
                item.description.as_deref(),
                on,
            ) {
                view.send(Command::SetValue {
                    id: CommandId::HeadAction,
                    payload: vec![u8::from(new)],
                });
            }
        });
        if on {
            key_card(ui, view, head, icons::ICON_ACCESSIBILITY_NEW);
        }
    }
    if keys.is_none() && panel.buttons.is_none() && head.is_none() {
        m3::titled_card(ui, icons::ICON_TOUCH_APP, "Controls", |ui| {
            m3::supporting(ui, "This model has no configurable controls.");
        });
    }
}

/// The action bound to `slot`, as reported by the earbuds.
fn current(view: &ViewCtx<'_>, slot: &KeySlot) -> Option<u8> {
    if let KeyTarget::Setting(id) = slot.target {
        // `[event, action]` pairs.
        return view.snap.state.setting(id).and_then(|p| {
            p.chunks_exact(2)
                .find(|pair| u16::from(pair[0]) == slot.event)
                .map(|pair| pair[1])
        });
    }
    view.snap
        .state
        .key_bindings
        .iter()
        .find(|b| b.gesture_id == slot.event)
        .map(|b| match b.action.id() {
            // Some firmwares report "no action" as 0xFF.
            0xFF => 0,
            a => a,
        })
}

fn key_card(ui: &mut Ui, view: &mut ViewCtx<'_>, keys: &KeyControl, icon: MaterialIcon) {
    let s = Scheme::of(ui);
    m3::titled_card(ui, icon, &keys.title, |ui| {
        if keys.slots().all(|s| current(view, s).is_none()) {
            m3::supporting(ui, "Waiting for the earbuds to report their settings…");
        }
        let mut first = true;
        for group in &keys.groups {
            if !group.name.is_empty() && keys.groups.len() > 1 {
                ui.label(text::title_small(&group.name).color(s.primary));
            }
            for row in &group.rows {
                if !first {
                    m3::divider(ui);
                }
                first = false;
                ui.horizontal(|ui| {
                    ui.label(crate::material::icon(
                        gesture_icon(&row.name).outlined(),
                        22.0,
                        s.on_surface_variant,
                    ));
                    ui.label(text::title_small(&row.name).color(s.on_surface));
                });
                let columns = row.slots.len().max(1);
                let width = (ui.available_width() - 12.0 * (columns as f32 - 1.0)) / columns as f32;
                ui.columns(columns, |cols| {
                    for (col, slot) in cols.iter_mut().zip(&row.slots) {
                        if let Some(side) = slot.side {
                            let label = match side {
                                Side::Left => "Left earbud",
                                Side::Right => "Right earbud",
                            };
                            col.label(text::body_small(label).color(s.on_surface_variant));
                        }
                        let mut choices: Vec<(u8, String)> = slot.choices.clone();
                        let now = current(view, slot);
                        if let Some(a) = now.filter(|a| !choices.iter().any(|(c, _)| c == a)) {
                            choices.push((a, format!("Other ({a})")));
                        }
                        let id = format!("key-{:?}-{}", slot.target, slot.event);
                        if let Some(action) = dropdown(col, &id, now, &choices, width - 8.0) {
                            view.send(Command::SetKey {
                                target: slot.target,
                                event: slot.event,
                                action,
                            });
                        }
                    }
                });
            }
        }
    });
}
