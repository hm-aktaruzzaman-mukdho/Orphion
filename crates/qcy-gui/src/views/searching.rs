//! Empty state shown until earbuds have been found.

use eframe::egui::Ui;
use qcy_bluez::manager::LinkStatus;

use crate::app::{Action, ViewCtx};
use crate::material::components::{self as m3, ButtonKind};
use crate::material::{Scheme, icons, text};
use crate::widgets;

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let s = Scheme::of(ui);
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        let icon = match view.snap.link {
            LinkStatus::Idle => icons::ICON_BLUETOOTH_DISABLED,
            _ => icons::ICON_BLUETOOTH_SEARCHING,
        };
        widgets::hero_icon(ui, icon);
        ui.add_space(16.0);
        match view.snap.link {
            LinkStatus::Idle => {
                ui.label(text::headline_small("Not connected").color(s.on_surface));
                ui.add_space(8.0);
                if m3::button(
                    ui,
                    ButtonKind::Filled,
                    Some(icons::ICON_BLUETOOTH),
                    "Connect",
                )
                .clicked()
                {
                    view.act(Action::Connect);
                }
            }
            _ => {
                ui.label(text::headline_small("Looking for your earbuds").color(s.on_surface));
                ui.label(text::body_medium(view.snap.link.label()).color(s.on_surface_variant));
                ui.add_space(8.0);
                ui.spinner();
            }
        }
    });
    ui.add_space(24.0);
    m3::titled_card(ui, icons::ICON_INFO, "Tips", |ui| {
        for (icon, tip) in [
            (icons::ICON_EARBUDS, "Take the earbuds out of the case."),
            (
                icons::ICON_PHONE_ANDROID,
                "Close the QCY app on your phone. The earbuds accept only one control connection.",
            ),
            (
                icons::ICON_BLUETOOTH_CONNECTED,
                "Pairing for audio is optional, but it lets the app pick the right earbuds.",
            ),
        ] {
            ui.horizontal_top(|ui| {
                ui.label(crate::material::icon(
                    icon.outlined(),
                    20.0,
                    s.on_surface_variant,
                ));
                m3::supporting(ui, tip);
            });
        }
    });
}
