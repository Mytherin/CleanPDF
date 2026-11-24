//! Colors and fonts, in light and dark mode: CleanNotes' (an Apple look),
//! with Preview's gray backdrop behind the pages.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId};

use crate::fonts::{self, Face};

/// Selections (pages in the sidebar), and buttons in dialogs.
pub const BLUE: Color32 = Color32::from_rgb(0x00, 0x7A, 0xFF);
pub const RED: Color32 = Color32::from_rgb(0xFF, 0x3B, 0x30);

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// The toolbar.
    pub bg: Color32,
    /// Behind the pages.
    pub canvas: Color32,
    /// The thumbnails sidebar.
    pub sidebar: Color32,
    /// Round toolbar buttons and button groups ("glass"), and their edge.
    pub glass: Color32,
    pub glass_edge: Color32,
    pub text: Color32,
    /// Page numbers, "Edited".
    pub secondary: Color32,
    pub tertiary: Color32,
    pub separator: Color32,
    /// A selected page's backdrop in the sidebar.
    pub selection: Color32,
    /// The same while the sidebar doesn't have the keyboard.
    pub selection_inactive: Color32,
    /// Toolbar buttons when hovered.
    pub hover: Color32,
    /// Menus and dialogs.
    pub popover: Color32,
    /// A page's outline, and its shadow.
    pub page_edge: Color32,
    pub shadow: Color32,
}

impl Palette {
    pub fn get(ctx: &egui::Context) -> Self {
        if ctx.style().visuals.dark_mode {
            Palette {
                dark: true,
                bg: Color32::from_rgb(0x2A, 0x2A, 0x2C),
                canvas: Color32::from_rgb(0x1E, 0x1E, 0x1E),
                sidebar: Color32::from_rgb(0x24, 0x25, 0x27),
                glass: Color32::from_rgb(0x33, 0x33, 0x35),
                glass_edge: Color32::from_rgb(0x45, 0x45, 0x48),
                text: Color32::from_rgb(0xE4, 0xE4, 0xE4),
                secondary: Color32::from_rgb(0x98, 0x98, 0x9D),
                tertiary: Color32::from_rgb(0x5E, 0x5E, 0x62),
                separator: Color32::from_rgb(0x3A, 0x3D, 0x40),
                selection: Color32::from_rgb(0x1F, 0x5F, 0xC4),
                selection_inactive: Color32::from_rgb(0x46, 0x46, 0x48),
                hover: Color32::from_rgb(0x44, 0x44, 0x47),
                popover: Color32::from_rgb(0x2B, 0x2B, 0x2D),
                page_edge: Color32::from_rgb(0x3C, 0x3C, 0x3E),
                shadow: Color32::from_black_alpha(110),
            }
        } else {
            Palette {
                dark: false,
                bg: Color32::from_rgb(0xF6, 0xF6, 0xF6),
                canvas: Color32::from_rgb(0xE6, 0xE6, 0xE6),
                sidebar: Color32::from_rgb(0xEE, 0xEE, 0xEE),
                glass: Color32::WHITE,
                glass_edge: Color32::from_rgb(0xDD, 0xDD, 0xDF),
                text: Color32::from_rgb(0x1D, 0x1D, 0x1F),
                secondary: Color32::from_rgb(0x86, 0x86, 0x8B),
                tertiary: Color32::from_rgb(0xBF, 0xBF, 0xC4),
                separator: Color32::from_rgb(0xDD, 0xDD, 0xDF),
                selection: Color32::from_rgb(0x3D, 0x8B, 0xF7),
                selection_inactive: Color32::from_rgb(0xD0, 0xD0, 0xD2),
                hover: Color32::from_rgb(0xE6, 0xE5, 0xE4),
                popover: Color32::from_rgb(0xF7, 0xF7, 0xF7),
                page_edge: Color32::from_rgb(0xCF, 0xCF, 0xD2),
                shadow: Color32::from_black_alpha(40),
            }
        }
    }
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn bold_font(size: f32) -> FontId {
    FontId::new(size, bold())
}

/// Uses the platform's fonts when available (see [`crate::fonts`]), falling
/// back to egui's built-in ones.
pub fn install_fonts(ctx: &egui::Context) {
    let data = |f: &fonts::FontFile| FontData { index: f.index, ..FontData::from_owned(f.bytes.clone()) };
    let mut defs = FontDefinitions::default();
    let base_prop = defs.families[&FontFamily::Proportional].clone();
    let add = |defs: &mut FontDefinitions, family: FontFamily, key: &str, file: Option<&fonts::FontFile>, base: &[String]| {
        let mut list = base.to_vec();
        if let Some(f) = file {
            defs.font_data.insert(key.into(), data(f));
            list.insert(0, key.into());
        }
        defs.families.insert(family, list);
    };
    add(&mut defs, FontFamily::Proportional, "ui", fonts::ui(Face { bold: false, italic: false }), &base_prop);
    let prop = defs.families[&FontFamily::Proportional].clone();
    add(&mut defs, bold(), "ui-bold", fonts::ui(Face { bold: true, italic: false }), &prop);
    ctx.set_fonts(defs);
}

pub fn apply_style(ctx: &egui::Context) {
    let p = Palette::get(ctx);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.visuals.panel_fill = p.canvas;
        s.visuals.window_fill = p.popover;
        s.visuals.window_rounding = egui::Rounding::same(12.0);
        s.visuals.menu_rounding = egui::Rounding::same(11.0);
        s.spacing.menu_margin = egui::Margin::same(5.0);
        s.visuals.window_stroke = if p.dark {
            egui::Stroke::new(1.0_f32, Color32::from_rgb(0x52, 0x52, 0x55))
        } else {
            egui::Stroke::new(1.0_f32, Color32::from_rgb(0xD2, 0xD2, 0xD4))
        };
        s.visuals.popup_shadow = egui::epaint::Shadow {
            offset: egui::vec2(0.0, 8.0),
            blur: 28.0,
            spread: 0.0,
            color: Color32::from_black_alpha(if p.dark { 120 } else { 50 }),
        };
        s.visuals.hyperlink_color = BLUE;
        for w in [
            &mut s.visuals.widgets.inactive,
            &mut s.visuals.widgets.hovered,
            &mut s.visuals.widgets.active,
            &mut s.visuals.widgets.open,
        ] {
            w.rounding = egui::Rounding::same(6.0);
        }
        s.visuals.widgets.noninteractive.bg_stroke.color = p.separator;
        s.text_styles.insert(egui::TextStyle::Body, font(13.5));
        s.text_styles.insert(egui::TextStyle::Button, font(13.5));
        s.text_styles.insert(egui::TextStyle::Small, font(11.0));
        s.text_styles.insert(egui::TextStyle::Heading, bold_font(20.0));
    });
}
