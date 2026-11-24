//! Menus drawn as macOS 26's (as CleanNotes' are): rows with an icon
//! column, a blue rounded highlight with white text, inset separators and
//! ✓ on chosen items. (The menu's frame — rounding, edge, shadow — is
//! egui's, set in [`crate::theme::apply_style`].)

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};

const ROW_H: f32 = 24.0;
/// Where a row's text starts, and its icon's center, from the row's left.
const TEXT_X: f32 = 30.0;
const ICON_X: f32 = 15.0;
const ICON_SIZE: f32 = 15.0;
const END_PAD: f32 = 14.0;
const ROUNDING: f32 = 6.0;

fn font() -> egui::FontId {
    theme::font(13.5)
}

/// Call first in a menu's contents: rows touch.
pub fn begin(ui: &mut Ui, min_width: f32) {
    ui.set_min_width(min_width);
    ui.spacing_mut().item_spacing.y = 0.0;
}

/// A menu row.
pub struct Item<'a> {
    label: &'a str,
    icon: Option<Icon>,
    shortcut: &'a str,
    enabled: bool,
    checked: bool,
}

pub fn item(label: &str) -> Item<'_> {
    Item { label, icon: None, shortcut: "", enabled: true, checked: false }
}

impl<'a> Item<'a> {
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn shortcut(mut self, shortcut: &'a str) -> Self {
        self.shortcut = shortcut;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A ✓ in the icon column.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Whether it was clicked (the menu then closes).
    pub fn show(self, ui: &mut Ui) -> bool {
        let p = Palette::get(ui.ctx());
        let label = ui.painter().layout_no_wrap(self.label.to_owned(), font(), Color32::PLACEHOLDER);
        let shortcut = (!self.shortcut.is_empty()).then(|| ui.painter().layout_no_wrap(self.shortcut.to_owned(), font(), Color32::PLACEHOLDER));
        let width = TEXT_X + label.size().x + shortcut.as_ref().map_or(0.0, |s| 28.0 + s.size().x) + END_PAD;
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, resp) = ui.allocate_at_least(Vec2::new(width, ROW_H), sense);
        let lit = self.enabled && resp.hovered();
        let color = if !self.enabled {
            p.tertiary
        } else if lit {
            Color32::WHITE
        } else {
            p.text
        };
        if lit {
            ui.painter().rect_filled(rect, ROUNDING, theme::BLUE);
        }
        let icon = if self.checked { Some(Icon::Check) } else { self.icon };
        if let Some(icon) = icon {
            let r = Rect::from_center_size(Pos2::new(rect.left() + ICON_X, rect.center().y), Vec2::splat(ICON_SIZE));
            icons::paint(ui.painter(), icon, r, color);
        }
        let y = rect.center().y - label.size().y / 2.0;
        ui.painter().galley(Pos2::new(rect.left() + TEXT_X, y), label, color);
        if let Some(s) = shortcut {
            let at = Pos2::new(rect.right() - END_PAD - s.size().x, rect.center().y - s.size().y / 2.0);
            ui.painter().galley(at, s, if lit { color } else { p.secondary });
        }
        if resp.clicked() {
            ui.close_menu();
        }
        resp.clicked()
    }
}

/// A line between groups of rows, inset from the menu's edges.
pub fn separator(ui: &mut Ui) {
    let p = Palette::get(ui.ctx());
    let (rect, _) = ui.allocate_at_least(Vec2::new(0.0, 11.0), Sense::hover());
    let y = rect.center().y.round() + 0.5;
    let edge = if p.dark { Color32::from_white_alpha(0x24) } else { Color32::from_black_alpha(0x1C) };
    ui.painter().hline(rect.left() + 10.0..=rect.right() - 10.0, y, egui::Stroke::new(1.0_f32, edge));
}
