//! Touch gesture bindings.

use eframe::egui::Ui;
use qcy_protocol::Command;
use qcy_protocol::types::{Gesture, KeyAction, Side, Tap};

use crate::app::ViewCtx;
use crate::material::components as m3;
use crate::material::{MaterialIcon, Scheme, icons, text};
use crate::views::dropdown;

fn tap_icon(tap: Tap) -> MaterialIcon {
    match tap {
        Tap::Single => icons::ICON_LOOKS_ONE,
        Tap::Double => icons::ICON_LOOKS_TWO,
        Tap::Triple => icons::ICON_LOOKS_3,
        Tap::Quadruple => icons::ICON_LOOKS_4,
        Tap::Long => icons::ICON_TOUCH_APP,
    }
}

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let s = Scheme::of(ui);
    let profile = view.profile;
    let bindings = view.snap.state.key_bindings.clone();
    let action_of = |g: Gesture| {
        bindings
            .iter()
            .find(|b| b.gesture_id == u16::from(g.id()))
            .map(|b| b.action)
    };

    // Rows: every tap kind offered for this model, in a stable order.
    let mut taps: Vec<Tap> = Vec::new();
    for g in profile.gestures {
        if !taps.contains(&g.tap) {
            taps.push(g.tap);
        }
    }
    let mut choices: Vec<(KeyAction, String)> = profile
        .key_actions
        .iter()
        .map(|a| (*a, a.label()))
        .collect();
    // Keep actions the device reports selectable even if unknown to us.
    for b in &bindings {
        if !choices.iter().any(|(a, _)| *a == b.action) {
            choices.push((b.action, b.action.label()));
        }
    }

    m3::titled_card(ui, icons::ICON_TOUCH_APP, "Touch controls", |ui| {
        if bindings.is_empty() {
            m3::supporting(
                ui,
                "Waiting for the earbuds to report their touch settings…",
            );
        }
        for (i, tap) in taps.iter().enumerate() {
            if i > 0 {
                m3::divider(ui);
            }
            ui.horizontal(|ui| {
                ui.label(crate::material::icon(
                    tap_icon(*tap).outlined(),
                    22.0,
                    s.on_surface_variant,
                ));
                ui.label(text::title_small(tap.label()).color(s.on_surface));
            });
            let width = (ui.available_width() - 12.0) / 2.0;
            ui.columns(2, |cols| {
                for (col, side) in cols.iter_mut().zip([Side::Left, Side::Right]) {
                    let gesture = Gesture::new(side, *tap);
                    col.label(
                        text::body_small(format!("{} earbud", side.label()))
                            .color(s.on_surface_variant),
                    );
                    if !profile.gestures.contains(&gesture) {
                        col.label(text::body_medium("Not available").color(s.on_surface_variant));
                        continue;
                    }
                    let id = format!("key-{}", gesture.id());
                    if let Some(action) =
                        dropdown(col, &id, action_of(gesture), &choices, width - 8.0)
                    {
                        view.send(Command::SetKeyFunctions(vec![(gesture.id(), action)]));
                    }
                }
            });
        }
    });

    if !profile.call_controls.is_empty() {
        m3::titled_card(ui, icons::ICON_CALL, "Calls", |ui| {
            m3::supporting(ui, "Fixed by the firmware:");
            for (gesture, action) in profile.call_controls {
                m3::setting_row(ui, icons::ICON_CALL, action, Some(gesture), 0.0, |_| {});
            }
        });
    }
}
