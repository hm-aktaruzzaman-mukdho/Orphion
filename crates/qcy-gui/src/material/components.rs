//! Material 3 components drawn with egui's painter.
//!
//! All components read colours from [`Scheme::of`] and honour the enclosing
//! `Ui`'s enabled state, so `ui.add_enabled_ui(false, …)` greys them out.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Layout, Rect, Response,
    Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use egui_material_icons::MaterialIcon;

use super::scheme::{Scheme, lerp};
use super::{icons, medium, regular, text};

const HOVER: f32 = 0.08;
const PRESS: f32 = 0.12;

/// Paints `icon` centred on `center`.
pub fn paint_icon(ui: &Ui, icon: MaterialIcon, center: egui::Pos2, size: f32, color: Color32) {
    ui.painter().text(
        center,
        Align2::CENTER_CENTER,
        icon.codepoint,
        FontId::new(size, icon.font_family()),
        color,
    );
}

fn state_layer(ui: &Ui, rect: Rect, radius: CornerRadius, response: &Response, on: Color32) {
    let opacity = if response.is_pointer_button_down_on() {
        PRESS
    } else if response.hovered() {
        HOVER
    } else {
        return;
    };
    ui.painter()
        .rect_filled(rect, radius, on.gamma_multiply(opacity));
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

/// A filled card (`surface_container`, 16 dp corners).
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let s = Scheme::of(ui);
    egui::Frame::new()
        .fill(s.surface_container)
        .corner_radius(16)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A card with a leading icon and a title.
pub fn titled_card<R>(
    ui: &mut Ui,
    icon: MaterialIcon,
    title: &str,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    card(ui, |ui| {
        let s = Scheme::of(ui);
        ui.horizontal(|ui| {
            ui.label(super::icon(icon, 22.0, s.primary));
            ui.label(text::title_medium(title).color(s.on_surface));
        });
        ui.add_space(4.0);
        add(ui)
    })
}

/// A thin horizontal divider (`outline_variant`).
pub fn divider(ui: &mut Ui) {
    let s = Scheme::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, s.outline_variant),
    );
}

/// Supporting text in `on_surface_variant`.
pub fn supporting(ui: &mut Ui, text: &str) {
    let s = Scheme::of(ui);
    ui.add(egui::Label::new(text::body_medium(text).color(s.on_surface_variant)).wrap());
}

/// A short notice with an icon, e.g. side effects of a setting.
pub fn notice(ui: &mut Ui, icon: MaterialIcon, text: &str, color: Color32) {
    ui.horizontal_top(|ui| {
        ui.label(super::icon(icon, 16.0, color));
        ui.add(egui::Label::new(text::body_small(text).color(color)).wrap());
    });
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Filled,
    Tonal,
    Outlined,
    Text,
    /// Outlined, in the error colour: for destructive actions.
    Danger,
}

/// A Material 3 common button (40 dp, fully rounded).
pub fn button(ui: &mut Ui, kind: ButtonKind, icon: Option<MaterialIcon>, label: &str) -> Response {
    let s = Scheme::of(ui);
    let enabled = ui.is_enabled();
    let (container, content, border) = match kind {
        ButtonKind::Filled => (s.primary, s.on_primary, None),
        ButtonKind::Tonal => (s.secondary_container, s.on_secondary_container, None),
        ButtonKind::Outlined => (Color32::TRANSPARENT, s.primary, Some(s.outline)),
        ButtonKind::Text => (Color32::TRANSPARENT, s.primary, None),
        ButtonKind::Danger => (Color32::TRANSPARENT, s.error, Some(s.error)),
    };
    let (container, content) = if enabled {
        (container, content)
    } else {
        let bg = if container == Color32::TRANSPARENT {
            container
        } else {
            s.on_surface.gamma_multiply(0.12)
        };
        (bg, s.disabled(s.on_surface))
    };

    let font = medium(14.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, content);
    let icon_w = if icon.is_some() { 18.0 + 8.0 } else { 0.0 };
    let pad = if icon.is_some() { 16.0 } else { 24.0 };
    let pad = if kind == ButtonKind::Text { 12.0 } else { pad };
    let size = vec2(pad * 2.0 + icon_w + galley.size().x, 40.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let radius = CornerRadius::same(20);
        ui.painter().rect_filled(rect, radius, container);
        if let Some(border) = border {
            let border = if enabled {
                border
            } else {
                s.on_surface.gamma_multiply(0.12)
            };
            ui.painter()
                .rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
        }
        state_layer(ui, rect, radius, &response, content);
        let mut x = rect.left() + pad;
        if let Some(icon) = icon {
            paint_icon(ui, icon, pos2(x + 9.0, rect.center().y), 18.0, content);
            x += icon_w;
        }
        let y = rect.center().y - galley.size().y / 2.0;
        ui.painter().galley(pos2(x, y), galley, content);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Selection controls
// ---------------------------------------------------------------------------

/// Paints an M3 switch into `rect` (52 × 32 dp). `t` is the animated on-ness.
fn paint_switch(ui: &Ui, rect: Rect, t: f32, hovered: bool) {
    let s = Scheme::of(ui);
    let enabled = ui.is_enabled();
    let radius = CornerRadius::same(16);
    let track = lerp(s.surface_container_highest, s.primary, t);
    let track = if enabled {
        track
    } else {
        lerp(s.surface, track, 0.12 / 0.38)
    };
    ui.painter().rect_filled(rect, radius, track);
    if t < 0.5 {
        let outline = if enabled {
            s.outline
        } else {
            s.disabled(s.on_surface)
        };
        ui.painter()
            .rect_stroke(rect, radius, Stroke::new(2.0, outline), StrokeKind::Inside);
    }
    let thumb_r = egui::lerp(8.0..=12.0, t);
    let x = egui::lerp((rect.left() + 16.0)..=(rect.right() - 16.0), t);
    let center = pos2(x, rect.center().y);
    if hovered && enabled {
        let layer = if t > 0.5 { s.primary } else { s.on_surface };
        ui.painter()
            .circle_filled(center, 20.0, layer.gamma_multiply(HOVER));
    }
    let thumb = lerp(s.outline, s.on_primary, t);
    let thumb = if enabled {
        thumb
    } else {
        s.disabled(s.on_surface)
    };
    ui.painter().circle_filled(center, thumb_r, thumb);
    if t > 0.7 {
        paint_icon(ui, icons::ICON_CHECK, center, 16.0, s.on_primary_container);
    }
}

/// A list row: icon, title and supporting text on the left, a switch on the
/// right. The whole row is clickable. Returns the new value when toggled.
pub fn switch_row(
    ui: &mut Ui,
    icon: MaterialIcon,
    title: &str,
    supporting: Option<&str>,
    on: bool,
) -> Option<bool> {
    let id = ui.id().with(("switch_row", title));
    let response = row(
        ui,
        id,
        Some(icon),
        title,
        supporting,
        60.0,
        |ui, hovered| {
            let (rect, _) = ui.allocate_exact_size(vec2(52.0, 32.0), Sense::hover());
            let t = ui.ctx().animate_bool_responsive(id.with("anim"), on);
            paint_switch(ui, rect, t, hovered);
        },
    );
    response.clicked().then_some(!on)
}

/// A list row with a trailing control (dropdown, button, …).
pub fn setting_row(
    ui: &mut Ui,
    icon: MaterialIcon,
    title: &str,
    supporting: Option<&str>,
    trailing_width: f32,
    trailing: impl FnOnce(&mut Ui),
) {
    let id = ui.id().with(("setting_row", title));
    // The trailing control handles its own clicks; the row only lays out.
    let mut trailing = Some(trailing);
    row_layout(
        ui,
        id,
        Some(icon),
        title,
        supporting,
        trailing_width,
        false,
        |ui, _| {
            if let Some(f) = trailing.take() {
                f(ui);
            }
        },
    );
}

/// Generic clickable list row.
fn row(
    ui: &mut Ui,
    id: Id,
    icon: Option<MaterialIcon>,
    title: &str,
    supporting: Option<&str>,
    trailing_width: f32,
    trailing: impl FnOnce(&mut Ui, bool),
) -> Response {
    row_layout(
        ui,
        id,
        icon,
        title,
        supporting,
        trailing_width,
        true,
        trailing,
    )
}

#[allow(clippy::too_many_arguments)]
fn row_layout(
    ui: &mut Ui,
    id: Id,
    icon: Option<MaterialIcon>,
    title: &str,
    supporting: Option<&str>,
    trailing_width: f32,
    clickable: bool,
    trailing: impl FnOnce(&mut Ui, bool),
) -> Response {
    let s = Scheme::of(ui);
    let enabled = ui.is_enabled();
    let (title_color, support_color, icon_color) = if enabled {
        (s.on_surface, s.on_surface_variant, s.on_surface_variant)
    } else {
        let d = s.disabled(s.on_surface);
        (d, d, d)
    };
    // Hover state from the previous frame (the rect is only known afterwards).
    let hovered = clickable && ui.ctx().read_response(id).is_some_and(|r| r.hovered());

    // Leading part first, so the row height (wrapped text!) is known…
    let full = ui.available_width();
    let top_left = ui.cursor().min;
    let leading_width = (full - trailing_width - 12.0).max(80.0);
    let leading = ui
        .allocate_ui_with_layout(
            vec2(leading_width, 0.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                // Padding instead of a fixed height keeps the text centred
                // whether or not it wraps.
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(0, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(icon) = icon {
                                ui.add_space(4.0);
                                ui.label(super::icon(icon, 24.0, icon_color));
                                ui.add_space(8.0);
                            }
                            let title =
                                egui::Label::new(text::body_large(title).color(title_color)).wrap();
                            match supporting {
                                // A lone label is centred by the row layout;
                                // a `vertical` block would stick to the top.
                                None => {
                                    ui.add(title);
                                }
                                Some(sup) => {
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        ui.add(title);
                                        ui.add(
                                            egui::Label::new(
                                                text::body_medium(sup).color(support_color),
                                            )
                                            .wrap(),
                                        );
                                    });
                                }
                            }
                        });
                    });
            },
        )
        .response;
    // …then the trailing control, vertically centred on the whole row.
    let row_rect = Rect::from_min_size(top_left, vec2(full, leading.rect.height()));
    let trailing_rect = Rect::from_min_max(
        pos2(row_rect.right() - trailing_width, row_rect.top()),
        row_rect.max,
    );
    // A detached child: it must not move the parent's cursor, which already
    // sits below the row.
    let mut trailing_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(trailing_rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    trailing(&mut trailing_ui, hovered);
    let row_response = leading.with_new_rect(row_rect);

    let rect = row_rect.expand2(vec2(8.0, 4.0));
    if !clickable {
        return row_response;
    }
    let response = ui.interact(rect, id, Sense::click());
    if response.hovered() && enabled {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(12),
            s.on_surface.gamma_multiply(HOVER),
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A selectable list row (single choice), highlighted when selected.
pub fn choice_row(
    ui: &mut Ui,
    icon: MaterialIcon,
    title: &str,
    supporting: &str,
    selected: bool,
) -> Response {
    let s = Scheme::of(ui);
    let id = ui.id().with(("choice", title));
    let where_to = ui.painter().add(egui::Shape::Noop);
    let response = row(
        ui,
        id,
        Some(icon),
        title,
        Some(supporting),
        32.0,
        |ui, _| {
            if selected {
                ui.label(super::icon(
                    icons::ICON_CHECK,
                    22.0,
                    s.on_secondary_container,
                ));
            }
        },
    );
    if selected {
        ui.painter().set(
            where_to,
            egui::Shape::rect_filled(response.rect, CornerRadius::same(12), s.secondary_container),
        );
    }
    response
}

/// A row of connected toggle buttons; returns the index clicked.
pub fn segmented(ui: &mut Ui, options: &[&str], selected: Option<usize>) -> Option<usize> {
    let s = Scheme::of(ui);
    let enabled = ui.is_enabled();
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
    let n = options.len().max(1) as f32;
    let seg_w = rect.width() / n;
    let outline = if enabled {
        s.outline
    } else {
        s.on_surface.gamma_multiply(0.12)
    };
    let mut clicked = None;

    for (i, label) in options.iter().enumerate() {
        let seg = Rect::from_min_size(
            pos2(rect.left() + seg_w * i as f32, rect.top()),
            vec2(seg_w, 40.0),
        );
        let radius = CornerRadius {
            nw: if i == 0 { 20 } else { 0 },
            sw: if i == 0 { 20 } else { 0 },
            ne: if i + 1 == options.len() { 20 } else { 0 },
            se: if i + 1 == options.len() { 20 } else { 0 },
        };
        let response = ui
            .interact(seg, ui.id().with(("segment", i, *label)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let is_selected = selected == Some(i);
        let content = match (enabled, is_selected) {
            (false, _) => s.disabled(s.on_surface),
            (true, true) => s.on_secondary_container,
            (true, false) => s.on_surface,
        };
        if is_selected {
            ui.painter().rect_filled(seg, radius, s.secondary_container);
        }
        state_layer(ui, seg, radius, &response, s.on_surface);
        ui.painter()
            .rect_stroke(seg, radius, Stroke::new(1.0, outline), StrokeKind::Inside);

        let galley = ui
            .painter()
            .layout_no_wrap(label.to_string(), medium(14.0), content);
        let icon_w = if is_selected { 26.0 } else { 0.0 };
        let total = icon_w + galley.size().x;
        let x = seg.center().x - total / 2.0;
        if is_selected {
            paint_icon(
                ui,
                icons::ICON_CHECK,
                pos2(x + 9.0, seg.center().y),
                18.0,
                content,
            );
        }
        ui.painter().galley(
            pos2(x + icon_w, seg.center().y - galley.size().y / 2.0),
            galley,
            content,
        );
        if response.clicked() {
            clicked = Some(i);
        }
    }
    clicked.filter(|&i| selected != Some(i))
}

/// A filter chip (32 dp, 8 dp corners), with a check mark when selected.
pub fn chip(ui: &mut Ui, icon: Option<MaterialIcon>, label: &str, selected: bool) -> Response {
    let s = Scheme::of(ui);
    let enabled = ui.is_enabled();
    let content = match (enabled, selected) {
        (false, _) => s.disabled(s.on_surface),
        (true, true) => s.on_secondary_container,
        (true, false) => s.on_surface_variant,
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), medium(14.0), content);
    let lead = if selected || icon.is_some() {
        26.0
    } else {
        0.0
    };
    let size = vec2(16.0 + lead + galley.size().x + 16.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let radius = CornerRadius::same(8);
    if selected {
        ui.painter()
            .rect_filled(rect, radius, s.secondary_container);
    } else {
        let border = if enabled {
            s.outline_variant
        } else {
            s.on_surface.gamma_multiply(0.12)
        };
        ui.painter()
            .rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    }
    state_layer(ui, rect, radius, &response, s.on_surface);
    let mut x = rect.left() + 12.0;
    let lead_icon = if selected {
        Some(icons::ICON_CHECK)
    } else {
        icon
    };
    if let Some(i) = lead_icon {
        paint_icon(ui, i, pos2(x + 9.0, rect.center().y), 18.0, content);
        x += lead;
    } else {
        x += 4.0;
    }
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        content,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A large selectable tile with an icon, title and subtitle.
pub fn tile(
    ui: &mut Ui,
    icon: MaterialIcon,
    title: &str,
    subtitle: &str,
    selected: bool,
    size: Vec2,
) -> Response {
    let s = Scheme::of(ui);
    let enabled = ui.is_enabled();
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (bg, fg, sub) = match (enabled, selected) {
        (true, true) => (
            s.primary_container,
            s.on_primary_container,
            s.on_primary_container,
        ),
        (true, false) => (
            s.surface_container_highest,
            s.on_surface,
            s.on_surface_variant,
        ),
        (false, sel) => {
            let bg = if sel {
                s.on_surface.gamma_multiply(0.12)
            } else {
                s.surface_container_high
            };
            (bg, s.disabled(s.on_surface), s.disabled(s.on_surface))
        }
    };
    let radius = CornerRadius::same(if selected { 24 } else { 16 });
    ui.painter().rect_filled(rect, radius, bg);
    state_layer(ui, rect, radius, &response, fg);
    let icon_icon = if selected { icon } else { icon.outlined() };
    paint_icon(
        ui,
        icon_icon,
        pos2(rect.center().x, rect.top() + 28.0),
        28.0,
        fg,
    );
    ui.painter().text(
        pos2(rect.center().x, rect.top() + 58.0),
        Align2::CENTER_CENTER,
        title,
        medium(14.0),
        fg,
    );
    ui.painter().text(
        pos2(rect.center().x, rect.top() + 78.0),
        Align2::CENTER_CENTER,
        subtitle,
        regular(12.0),
        sub,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

/// A navigation rail destination (active indicator pill + label).
pub fn nav_item(ui: &mut Ui, icon: MaterialIcon, label: &str, selected: bool) -> Response {
    let s = Scheme::of(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(80.0, 60.0), Sense::click());
    let pill = Rect::from_center_size(pos2(rect.center().x, rect.top() + 18.0), vec2(56.0, 32.0));
    let radius = CornerRadius::same(16);
    if selected {
        ui.painter()
            .rect_filled(pill, radius, s.secondary_container);
    }
    state_layer(ui, pill, radius, &response, s.on_surface);
    let (icon, icon_color, label_color) = if selected {
        (icon, s.on_secondary_container, s.on_surface)
    } else {
        (icon.outlined(), s.on_surface_variant, s.on_surface_variant)
    };
    paint_icon(ui, icon, pill.center(), 24.0, icon_color);
    ui.painter().text(
        pos2(rect.center().x, rect.top() + 46.0),
        Align2::CENTER_CENTER,
        label,
        medium(12.0),
        label_color,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------

/// Snackbars stacked at the bottom centre of the window.
pub fn snackbars(ctx: &egui::Context, messages: &[(String, bool)]) {
    if messages.is_empty() {
        return;
    }
    let s = Scheme::for_dark(ctx.theme() == egui::Theme::Dark);
    egui::Area::new(Id::new("snackbars"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -24.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            for (msg, is_error) in messages {
                egui::Frame::new()
                    .fill(s.inverse_surface)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .shadow(ui.visuals().popup_shadow)
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            if *is_error {
                                ui.label(super::icon(
                                    icons::ICON_ERROR,
                                    20.0,
                                    s.inverse_on_surface,
                                ));
                            }
                            ui.add(
                                egui::Label::new(
                                    text::body_medium(msg).color(s.inverse_on_surface),
                                )
                                .wrap(),
                            );
                        });
                    });
                ui.add_space(8.0);
            }
        });
}

/// Outcome of a [`dialog`] frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogResult {
    Open,
    Confirmed,
    Dismissed,
}

/// A basic M3 dialog with an icon, headline, body and two actions.
pub fn dialog(
    ctx: &egui::Context,
    icon: MaterialIcon,
    headline: &str,
    body: &str,
    confirm: &str,
    destructive: bool,
) -> DialogResult {
    let s = Scheme::for_dark(ctx.theme() == egui::Theme::Dark);
    let mut result = DialogResult::Open;
    let modal = egui::Modal::new(Id::new("m3-dialog"))
        .frame(
            egui::Frame::new()
                .fill(s.surface_container_high)
                .corner_radius(28)
                .inner_margin(24),
        )
        .show(ctx, |ui| {
            ui.set_width(340.0);
            let accent = if destructive { s.error } else { s.primary };
            ui.vertical_centered(|ui| {
                ui.label(super::icon(icon, 24.0, accent));
                ui.add_space(8.0);
                ui.label(text::headline_small(headline).color(s.on_surface));
            });
            ui.add_space(8.0);
            ui.add(egui::Label::new(text::body_medium(body).color(s.on_surface_variant)).wrap());
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let kind = if destructive {
                    ButtonKind::Danger
                } else {
                    ButtonKind::Filled
                };
                if button(ui, kind, None, confirm).clicked() {
                    result = DialogResult::Confirmed;
                }
                if button(ui, ButtonKind::Text, None, "Cancel").clicked() {
                    result = DialogResult::Dismissed;
                }
            });
        });
    if result == DialogResult::Open && modal.should_close() {
        result = DialogResult::Dismissed;
    }
    result
}

/// An assist-chip style status pill with a coloured dot.
pub fn status_pill(ui: &mut Ui, label: &str, color: Color32) {
    let s = Scheme::of(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), medium(12.0), s.on_surface);
    let size = vec2(12.0 + 8.0 + 6.0 + galley.size().x + 12.0, 28.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(14), s.surface_container_highest);
    ui.painter()
        .circle_filled(pos2(rect.left() + 16.0, rect.center().y), 4.0, color);
    ui.painter().galley(
        pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0),
        galley,
        s.on_surface,
    );
}
