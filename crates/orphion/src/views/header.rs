//! Device name, connection status and battery gauges.

use eframe::egui::{Align, Layout, Ui};
use qcy_client::manager::LinkStatus;

use crate::app::{Action, ViewCtx};
use crate::material::components::{self as m3, ButtonKind};
use crate::material::{Scheme, icons, text};
use crate::widgets;

pub fn show(ui: &mut Ui, view: &mut ViewCtx<'_>) {
    let snap = view.snap;
    let s = Scheme::of(ui);
    let name = snap
        .state
        .device_name
        .clone()
        // In the charging case the earbuds may report an empty name.
        .filter(|n| !n.trim().is_empty())
        .or_else(|| snap.device.as_ref().map(|d| d.name.clone()))
        .unwrap_or_else(|| "QCY earbuds".into());

    m3::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(text::headline_small(&name).color(s.on_surface));
                let color = widgets::status_color(&s, snap.is_connected(), snap.link.is_busy());
                m3::status_pill(ui, &snap.link.label(), color);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match snap.link {
                LinkStatus::Connected => {
                    if m3::button(
                        ui,
                        ButtonKind::Tonal,
                        Some(icons::ICON_LINK_OFF),
                        "Disconnect",
                    )
                    .on_hover_text("Release the control link. Audio keeps playing.")
                    .clicked()
                    {
                        view.act(Action::Disconnect);
                    }
                }
                LinkStatus::Idle => {
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
                    if m3::button(ui, ButtonKind::Text, None, "Stop").clicked() {
                        view.act(Action::Disconnect);
                    }
                    ui.spinner();
                }
            });
        });

        ui.add_space(8.0);
        let battery = snap.state.battery.unwrap_or_default();
        ui.columns(3, |cols| {
            let items = [
                (icons::ICON_EARBUDS, "Left", battery.left),
                (icons::ICON_EARBUDS, "Right", battery.right),
                (icons::ICON_BATTERY_CHARGING_FULL, "Case", battery.case),
            ];
            for (col, (icon, label, level)) in cols.iter_mut().zip(items) {
                col.vertical_centered(|ui| widgets::battery_ring(ui, icon, label, level));
            }
        });
    });
}
