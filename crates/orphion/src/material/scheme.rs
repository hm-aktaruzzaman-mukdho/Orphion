//! Material 3 colour roles, and their mapping onto egui's `Visuals`.
//!
//! The roles are generated from a single *seed* (accent) colour with Google's
//! dynamic colour algorithm (the "tonal spot" variant Android uses for
//! wallpaper-based themes), via the `material-colors` port of
//! material-color-utilities. Changing the accent regenerates both the light
//! and the dark scheme; see [`Scheme::set_seed`].

use std::sync::{LazyLock, RwLock};

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke, Visuals, style::WidgetVisuals};
use material_colors::color::Rgb;
use material_colors::theme::ThemeBuilder;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The accent the app starts with (Google blue).
pub const DEFAULT_SEED: Color32 = rgb(0x0B57D0);

/// Accent colours offered in Settings → Appearance, as `(name, seed)`.
pub const ACCENT_PRESETS: [(&str, Color32); 9] = [
    ("Blue", DEFAULT_SEED),
    ("Indigo", rgb(0x3F51B5)),
    ("Purple", rgb(0x6750A4)),
    ("Pink", rgb(0xC2185B)),
    ("Red", rgb(0xD32F2F)),
    ("Orange", rgb(0xE8710A)),
    ("Amber", rgb(0xF9A825)),
    ("Green", rgb(0x2E7D32)),
    ("Teal", rgb(0x00897B)),
];

/// The Material 3 colour roles used by this app.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scheme {
    pub dark: bool,
    pub primary: Color32,
    pub on_primary: Color32,
    pub primary_container: Color32,
    pub on_primary_container: Color32,
    pub secondary_container: Color32,
    pub on_secondary_container: Color32,
    pub tertiary: Color32,
    pub error: Color32,
    pub on_error: Color32,
    pub surface: Color32,
    pub surface_container_low: Color32,
    pub surface_container: Color32,
    pub surface_container_high: Color32,
    pub surface_container_highest: Color32,
    pub on_surface: Color32,
    pub on_surface_variant: Color32,
    pub outline: Color32,
    pub outline_variant: Color32,
    pub inverse_surface: Color32,
    pub inverse_on_surface: Color32,
    /// Not a Material role: status colours that must not follow the accent.
    pub success: Color32,
    pub warning: Color32,
}

/// The schemes generated from the current seed: `[light, dark]`.
static CURRENT: LazyLock<RwLock<[Scheme; 2]>> =
    LazyLock::new(|| RwLock::new(Scheme::pair(DEFAULT_SEED)));

impl Scheme {
    /// The scheme matching the current light/dark mode of `ui`.
    pub fn of(ui: &egui::Ui) -> Scheme {
        Self::for_dark(ui.visuals().dark_mode)
    }

    /// The current light or dark scheme.
    pub fn for_dark(dark: bool) -> Scheme {
        let schemes = CURRENT.read().unwrap_or_else(|e| e.into_inner());
        schemes[usize::from(dark)]
    }

    /// Regenerates both schemes from a new accent colour.
    pub fn set_seed(seed: Color32) {
        let pair = Self::pair(seed);
        *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = pair;
    }

    /// Light and dark scheme for `seed`.
    fn pair(seed: Color32) -> [Scheme; 2] {
        let theme = ThemeBuilder::with_source(Rgb::new(seed.r(), seed.g(), seed.b())).build();
        [
            Self::from_material(&theme.schemes.light, false),
            Self::from_material(&theme.schemes.dark, true),
        ]
    }

    fn from_material(m: &material_colors::scheme::Scheme, dark: bool) -> Scheme {
        let c = |c: Rgb| Color32::from_rgb(c.red, c.green, c.blue);
        Scheme {
            dark,
            primary: c(m.primary),
            on_primary: c(m.on_primary),
            primary_container: c(m.primary_container),
            on_primary_container: c(m.on_primary_container),
            secondary_container: c(m.secondary_container),
            on_secondary_container: c(m.on_secondary_container),
            tertiary: c(m.tertiary),
            error: c(m.error),
            on_error: c(m.on_error),
            surface: c(m.surface),
            surface_container_low: c(m.surface_container_low),
            surface_container: c(m.surface_container),
            surface_container_high: c(m.surface_container_high),
            surface_container_highest: c(m.surface_container_highest),
            on_surface: c(m.on_surface),
            on_surface_variant: c(m.on_surface_variant),
            outline: c(m.outline),
            outline_variant: c(m.outline_variant),
            inverse_surface: c(m.inverse_surface),
            inverse_on_surface: c(m.inverse_on_surface),
            success: if dark { rgb(0x6DD58C) } else { rgb(0x146C2E) },
            warning: if dark { rgb(0xFFBA42) } else { rgb(0x8A5100) },
        }
    }

    /// `color` with a Material "state layer" of `layer` blended on top
    /// (hover = 0.08, pressed = 0.10, dragged = 0.16).
    pub fn state_layer(color: Color32, layer: Color32, opacity: f32) -> Color32 {
        lerp(color, layer, opacity)
    }

    /// Content colour at the 38 % opacity Material uses for disabled items.
    pub fn disabled(&self, color: Color32) -> Color32 {
        lerp(self.surface, color, 0.38)
    }

    /// Maps the roles onto egui's built-in widgets (combo boxes, sliders,
    /// text fields, menus, scroll bars) so they blend in with our own.
    pub fn visuals(&self) -> Visuals {
        let mut v = if self.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        let radius = CornerRadius::same(8);
        let widget = |bg: Color32, fg: Color32, stroke: Color32| WidgetVisuals {
            bg_fill: bg,
            weak_bg_fill: bg,
            bg_stroke: Stroke::new(1.0, stroke),
            corner_radius: radius,
            fg_stroke: Stroke::new(1.5, fg),
            expansion: 0.0,
        };

        v.panel_fill = self.surface;
        v.window_fill = self.surface_container_high;
        v.window_stroke = Stroke::NONE;
        v.window_corner_radius = CornerRadius::same(28);
        v.menu_corner_radius = CornerRadius::same(8);
        v.extreme_bg_color = self.surface_container_highest;
        v.faint_bg_color = self.surface_container_low;
        v.code_bg_color = self.surface_container_high;
        v.hyperlink_color = self.primary;
        v.warn_fg_color = self.warning;
        v.error_fg_color = self.error;
        v.selection.bg_fill = self.primary;
        v.selection.stroke = Stroke::new(1.0, self.on_primary);
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.35 };
        v.popup_shadow = Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(if self.dark { 120 } else { 40 }),
        };
        v.window_shadow = v.popup_shadow;

        let hovered = Self::state_layer(self.surface_container_highest, self.on_surface, 0.08);
        let pressed = Self::state_layer(self.surface_container_highest, self.on_surface, 0.12);
        v.widgets.noninteractive = widget(
            self.surface_container,
            self.on_surface_variant,
            self.outline_variant,
        );
        v.widgets.inactive = widget(
            self.surface_container_highest,
            self.on_surface,
            Color32::TRANSPARENT,
        );
        v.widgets.hovered = widget(hovered, self.on_surface, self.outline);
        v.widgets.active = widget(pressed, self.on_surface, self.primary);
        v.widgets.open = widget(pressed, self.on_surface, self.primary);
        v.text_cursor.stroke = Stroke::new(2.0, self.primary);
        v
    }
}

/// Linear interpolation between two colours (`t` = 0 → `a`, 1 → `b`).
pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// `#RRGGBB` for a colour.
pub fn to_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// Parses `#RRGGBB` (the `#` is optional).
pub fn from_hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    u32::from_str_radix(s, 16).ok().map(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        assert_eq!(from_hex(&to_hex(DEFAULT_SEED)), Some(DEFAULT_SEED));
        assert_eq!(from_hex("#00897b"), Some(rgb(0x00897B)));
        assert_eq!(from_hex("nope"), None);
    }

    #[test]
    fn seeds_give_readable_schemes() {
        for (_, seed) in ACCENT_PRESETS {
            let [light, dark] = Scheme::pair(seed);
            // Primary and its "on" colour must contrast strongly.
            let luma = |c: Color32| {
                0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b())
            };
            assert!((luma(light.primary) - luma(light.on_primary)).abs() > 80.0);
            assert!((luma(dark.primary) - luma(dark.on_primary)).abs() > 80.0);
            assert!(dark.surface != light.surface);
        }
    }
}
