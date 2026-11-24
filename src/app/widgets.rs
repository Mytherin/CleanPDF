//! Toolbar buttons, dialogs and page painting (the buttons and dialogs are
//! CleanNotes').

use eframe::egui::{self, Color32, Pos2, Rect, Sense, TextureId, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::theme::Palette;

/// A round-cornered toolbar button with an icon. Returns whether it was
/// clicked.
pub fn tool_button(ui: &mut Ui, p: &Palette, rect: Rect, icon: Icon, tip: &str, enabled: bool, on: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(("tool", tip)), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && (resp.hovered() || on) {
        ui.painter().rect_filled(rect, rect.height() / 2.0, p.hover);
    }
    let color = if enabled { p.text } else { p.tertiary };
    icons::paint(ui.painter(), icon, rect.shrink(rect.height() * 0.2), color);
    let resp = resp.on_hover_text(tip);
    enabled && resp.clicked()
}

/// Paints a "glass" shape: a raised, softly outlined round rect.
pub fn glass(ui: &Ui, p: &Palette, rect: Rect) {
    let r = rect.height() / 2.0;
    let painter = ui.painter();
    painter.rect_filled(rect.translate(Vec2::new(0.0, 1.5)), r, Color32::from_black_alpha(if p.dark { 70 } else { 18 }));
    painter.rect(rect, r, p.glass, egui::Stroke::new(1.0_f32, p.glass_edge));
}

/// A round glass toolbar button.
pub fn glass_button(ui: &mut Ui, p: &Palette, center: Pos2, icon: Icon, tip: &str, enabled: bool) -> egui::Response {
    let rect = Rect::from_center_size(center, Vec2::splat(34.0));
    glass(ui, p, rect);
    capsule_item(ui, p, rect, icon, tip, enabled)
}

/// A button inside a glass capsule (or a glass button's face).
pub fn capsule_item(ui: &mut Ui, p: &Palette, rect: Rect, icon: Icon, tip: &str, enabled: bool) -> egui::Response {
    let resp = ui.interact(rect, ui.id().with(("glass", tip)), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect.shrink(3.0), (rect.height() - 6.0) / 2.0, p.hover);
    }
    let color = if enabled { p.text } else { p.tertiary };
    icons::paint(ui.painter(), icon, Rect::from_center_size(rect.center(), Vec2::splat(rect.height() * 0.52)), color);
    resp.on_hover_text(tip)
}

/// A page, `tex` drawn into `rect` turned clockwise by `rotation` degrees,
/// tinted (for a page being dragged, say).
pub fn paint_texture(painter: &egui::Painter, rect: Rect, tex: TextureId, rotation: u16, tint: Color32) {
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    let uvs = [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)];
    let turns = (rotation / 90) as usize % 4;
    let mut mesh = egui::Mesh::with_texture(tex);
    for (j, corner) in corners.iter().enumerate() {
        mesh.vertices.push(egui::epaint::Vertex { pos: *corner, uv: uvs[(j + 4 - turns) % 4], color: tint });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// A dialog in the middle of the window, dimming everything behind it.
pub fn modal<R>(ctx: &egui::Context, id: &str, p: &Palette, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new((id, "dim"))).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(if p.dark { 110 } else { 60 }));
    });
    egui::Window::new(id)
        .id(egui::Id::new((id, "window")))
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(egui::Frame::window(&ctx.style()).fill(p.popover).inner_margin(egui::Margin::same(18.0)).rounding(12.0))
        .show(ctx, add)
        .and_then(|r| r.inner)
}
