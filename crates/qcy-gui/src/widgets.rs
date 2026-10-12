//! App-specific widgets built on the Material scheme.

use eframe::egui::{
    Align2, Color32, CornerRadius, Pos2, Sense, Shape, Stroke, Ui, epaint::Mesh, pos2, vec2,
};
use qcy_protocol::eq::ParametricEq;
use qcy_protocol::types::BatteryLevel;

use crate::material::components::paint_icon;
use crate::material::{MaterialIcon, Scheme, icons, medium, regular};

/// A circular battery gauge with the percentage in the middle and a label
/// with an icon underneath.
pub fn battery_ring(ui: &mut Ui, icon: MaterialIcon, label: &str, level: Option<BatteryLevel>) {
    let s = Scheme::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(96.0, 112.0), Sense::hover());
    let center = pos2(rect.center().x, rect.top() + 40.0);
    let radius = 34.0;
    let width = 7.0;

    // Track.
    ui.painter().circle_stroke(
        center,
        radius,
        Stroke::new(width, s.surface_container_highest),
    );

    let (text, color) = match level {
        Some(b) => {
            let color = match b.percent {
                _ if b.charging => s.tertiary,
                0..=15 => s.error,
                16..=30 => s.warning,
                _ => s.primary,
            };
            (format!("{}%", b.percent), color)
        }
        None => ("–".to_string(), s.on_surface_variant),
    };

    if let Some(b) = level {
        // Progress arc, clockwise from 12 o'clock, with round caps.
        let fraction = f32::from(b.percent) / 100.0;
        let steps = (64.0 * fraction).ceil().max(2.0) as usize;
        let start = -std::f32::consts::FRAC_PI_2;
        let points: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let a = start + std::f32::consts::TAU * fraction * i as f32 / steps as f32;
                center + vec2(a.cos(), a.sin()) * radius
            })
            .collect();
        for cap in [points[0], points[points.len() - 1]] {
            ui.painter().circle_filled(cap, width / 2.0, color);
        }
        ui.painter()
            .add(Shape::line(points, Stroke::new(width, color)));
        if b.charging {
            paint_icon(ui, icons::ICON_BOLT, center + vec2(0.0, -16.0), 14.0, color);
        }
    }
    ui.painter().text(
        center + vec2(0.0, 2.0),
        Align2::CENTER_CENTER,
        text,
        medium(18.0),
        s.on_surface,
    );

    // Label row: icon + text, centred under the ring.
    let label_y = rect.top() + 96.0;
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), regular(13.0), s.on_surface_variant);
    let total = 18.0 + 4.0 + galley.size().x;
    let x = rect.center().x - total / 2.0;
    paint_icon(
        ui,
        icon.outlined(),
        pos2(x + 9.0, label_y),
        18.0,
        s.on_surface_variant,
    );
    ui.painter().galley(
        pos2(x + 22.0, label_y - galley.size().y / 2.0),
        galley,
        s.on_surface_variant,
    );
}

/// Plots an EQ's magnitude response (20 Hz – 20 kHz, ±12 dB) as a filled curve.
pub fn eq_curve(ui: &mut Ui, eq: &ParametricEq) {
    const DB: f64 = 12.0;
    let s = Scheme::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 140.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(12), s.surface_container_low);

    let x_of = |f: f64| {
        let t = (f.log10() - 20f64.log10()) / (20_000f64.log10() - 20f64.log10());
        rect.left() + rect.width() * t as f32
    };
    let y_of = |db: f64| {
        let t = (db.clamp(-DB, DB) + DB) / (2.0 * DB);
        rect.bottom() - rect.height() * t as f32
    };

    let grid = Stroke::new(1.0, s.outline_variant.gamma_multiply(0.6));
    for f in [100.0, 1_000.0, 10_000.0] {
        painter.vline(x_of(f), rect.y_range(), grid);
    }
    painter.hline(
        rect.x_range(),
        y_of(0.0),
        Stroke::new(1.0, s.outline_variant),
    );
    for (f, l) in [(100.0, "100 Hz"), (1_000.0, "1 kHz"), (10_000.0, "10 kHz")] {
        painter.text(
            pos2(x_of(f) + 4.0, rect.bottom() - 4.0),
            Align2::LEFT_BOTTOM,
            l,
            regular(10.0),
            s.on_surface_variant,
        );
    }
    for db in [6.0, -6.0] {
        painter.text(
            pos2(rect.left() + 6.0, y_of(db)),
            Align2::LEFT_CENTER,
            format!("{db:+} dB"),
            regular(10.0),
            s.on_surface_variant,
        );
    }

    let points: Vec<Pos2> = (0..=240)
        .map(|i| {
            let f = 20.0 * 1000f64.powf(f64::from(i) / 240.0);
            pos2(x_of(f), y_of(eq.response_db(f)))
        })
        .collect();

    // Tinted area between the curve and the 0 dB line.
    let zero = y_of(0.0);
    let fill = s.primary.gamma_multiply(0.18);
    let mut mesh = Mesh::default();
    for p in &points {
        mesh.colored_vertex(*p, fill);
        mesh.colored_vertex(pos2(p.x, zero), fill);
    }
    for i in 0..(points.len() as u32 - 1) {
        let (a, b, c, d) = (i * 2, i * 2 + 1, i * 2 + 2, i * 2 + 3);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, c, d);
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::line(points, Stroke::new(2.5, s.primary)));
}

/// Colour of the connection status dot.
pub fn status_color(s: &Scheme, connected: bool, busy: bool) -> Color32 {
    match (connected, busy) {
        (true, _) => s.success,
        (false, true) => s.warning,
        _ => s.outline,
    }
}

/// Empty-state illustration: a large icon in a tonal circle.
pub fn hero_icon(ui: &mut Ui, icon: MaterialIcon) {
    let s = Scheme::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(112.0, 112.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 56.0, s.primary_container);
    paint_icon(ui, icon, rect.center(), 56.0, s.on_primary_container);
}
