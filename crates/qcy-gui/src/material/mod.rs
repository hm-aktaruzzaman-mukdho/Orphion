//! A small Material 3 design system for egui.
//!
//! * [`scheme`]: colour roles for dark and light mode
//! * [`components`]: cards, switches, buttons, chips, segmented buttons,
//!   list rows, navigation rail items, snackbars, dialogs
//! * this module: fonts (Roboto, or Segoe UI on Windows, + Material Symbols)
//!   and the type scale
//!
//! Nothing here knows about earbuds, so it can be reused or extracted.

pub mod components;
pub mod scheme;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, RichText, Theme};
pub use egui_material_icons::{MaterialIcon, icons};
pub use scheme::Scheme;

/// Font family with medium weight, used for titles and labels.
const MEDIUM: &str = "medium";

/// Installs fonts, icons and the light + dark styles.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    egui_material_icons::initialize(ctx);

    ctx.set_visuals_of(Theme::Dark, scheme::DARK.visuals());
    ctx.set_visuals_of(Theme::Light, scheme::LIGHT.visuals());
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 32.0;
        style.spacing.slider_width = 240.0;
        style.spacing.slider_rail_height = 6.0;
        style.spacing.combo_height = 320.0;
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.scroll.floating = true;
        style.interaction.selectable_labels = false;
        use egui::TextStyle::*;
        style.text_styles = [
            (Heading, regular(22.0)),
            (Body, regular(14.0)),
            (Button, medium(14.0)),
            (Small, regular(12.0)),
            (Monospace, FontId::monospace(13.0)),
        ]
        .into();
    });
}

/// Loads the platform's UI font (see [`system_fonts`]); egui's bundled font
/// remains as a fallback for missing glyphs or when nothing is found.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut medium_family = fonts.families[&FontFamily::Proportional].clone();
    let (regular, medium) = system_fonts();

    if let Some(data) = regular {
        fonts
            .font_data
            .insert("ui".into(), FontData::from_owned(data).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ui".into());
        medium_family.insert(0, "ui".into());
    }
    if let Some(data) = medium {
        fonts
            .font_data
            .insert("ui-medium".into(), FontData::from_owned(data).into());
        medium_family.insert(0, "ui-medium".into());
    }
    fonts
        .families
        .insert(FontFamily::Name(MEDIUM.into()), medium_family);
    ctx.set_fonts(fonts);
}

/// Regular and medium weight of the UI font: Roboto (Material's typeface)
/// found through fontconfig.
#[cfg(not(windows))]
fn system_fonts() -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    (
        fontconfig_font("Roboto", "Regular"),
        fontconfig_font("Roboto", "Medium"),
    )
}

/// Regular and medium weight of the UI font: Roboto if the user installed
/// it, otherwise Segoe UI (regular + semibold), which every Windows has.
#[cfg(windows)]
fn system_fonts() -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    use std::path::PathBuf;
    let dirs: Vec<PathBuf> = [
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Fonts")),
        std::env::var_os("WINDIR").map(|d| PathBuf::from(d).join("Fonts")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let find = |file: &str| dirs.iter().find_map(|d| std::fs::read(d.join(file)).ok());
    match (find("Roboto-Regular.ttf"), find("Roboto-Medium.ttf")) {
        (Some(r), m) => (Some(r), m),
        _ => (find("segoeui.ttf"), find("seguisb.ttf")),
    }
}

/// Asks fontconfig for `family`/`style`; returns `None` if it is not installed.
#[cfg(not(windows))]
fn fontconfig_font(family: &str, style: &str) -> Option<Vec<u8>> {
    let out = std::process::Command::new("fc-match")
        .arg(format!("{family}:style={style}"))
        .arg("--format=%{family}|%{style}|%{file}")
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let mut parts = text.splitn(3, '|');
    let (found_family, found_style, file) = (parts.next()?, parts.next()?, parts.next()?);
    // fc-match always returns *something*; make sure it is what we asked for.
    if !found_family.contains(family) || !found_style.contains(style) {
        return None;
    }
    std::fs::read(file).ok()
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// The Material 3 type scale (subset).
pub mod text {
    use super::*;

    pub fn headline_small(t: impl Into<String>) -> RichText {
        RichText::new(t).font(regular(24.0))
    }
    pub fn title_medium(t: impl Into<String>) -> RichText {
        RichText::new(t).font(medium(16.0))
    }
    pub fn title_small(t: impl Into<String>) -> RichText {
        RichText::new(t).font(medium(14.0))
    }
    pub fn body_large(t: impl Into<String>) -> RichText {
        RichText::new(t).font(regular(16.0))
    }
    pub fn body_medium(t: impl Into<String>) -> RichText {
        RichText::new(t).font(regular(14.0))
    }
    pub fn body_small(t: impl Into<String>) -> RichText {
        RichText::new(t).font(regular(12.0))
    }
    pub fn label_large(t: impl Into<String>) -> RichText {
        RichText::new(t).font(medium(14.0))
    }
    pub fn label_medium(t: impl Into<String>) -> RichText {
        RichText::new(t).font(medium(12.0))
    }
}

/// An icon as text of the given size and colour.
pub fn icon(icon: MaterialIcon, size: f32, color: egui::Color32) -> RichText {
    icon.rich_text().size(size).color(color)
}
