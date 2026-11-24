//! The pages, in a column that scrolls (both ways when zoomed in), each
//! drawn at the size it's shown; or, with no document, a place to open one.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};

use super::widgets::paint_texture;
use super::{Action, App, Zoom, MAX_ZOOM, MIN_ZOOM};
use crate::icons::{self, Icon};
use crate::render::Page;
use crate::theme::{self, Palette};

/// Around the column, and between pages.
const MARGIN: f32 = 24.0;
const GAP: f32 = 16.0;

pub fn show(app: &mut App, ctx: &egui::Context, p: &Palette) {
    egui::CentralPanel::default().frame(egui::Frame::none().fill(p.canvas)).show(ctx, |ui| {
        if app.doc.is_none() {
            if empty(ui, p) {
                app.run(ctx, Action::Open);
            }
            return;
        }
        pages(app, ui, p);
    });
}

fn pages(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    let doc = app.doc.as_ref().unwrap();
    let n = doc.len();
    let sizes: Vec<Vec2> = (0..n).map(|i| doc.page_size(i)).collect();
    let widest = sizes.iter().fold(1.0f32, |w, s| w.max(s.x));
    let avail = ui.available_size();
    // (Room for the scroll bar, so fitting doesn't make it scroll across.)
    let bar = ui.spacing().scroll.allocated_width();
    let scale = match app.zoom {
        Zoom::FitWidth => ((avail.x - 2.0 * MARGIN - bar) / widest).clamp(MIN_ZOOM, MAX_ZOOM),
        Zoom::Scale(s) => s,
    };
    app.scale = scale;
    let mut tops = Vec::with_capacity(n);
    let mut y = MARGIN;
    for s in &sizes {
        tops.push(y);
        y += s.y * scale + GAP;
    }
    let height = y - GAP + MARGIN;
    let width = widest * scale + 2.0 * MARGIN;

    let mut area = egui::ScrollArea::both().id_salt(("pages", doc.id)).auto_shrink([false, false]);
    let mut placed = false;
    if let Some(i) = app.scroll_to.take().filter(|&i| i < n) {
        area = area.vertical_scroll_offset(tops[i] - GAP / 2.0);
        app.current = i;
        placed = true;
    } else if let Some((i, f)) = app.keep_center.take().filter(|&(i, _)| i < n) {
        let y = tops[i] + f * sizes[i].y * scale - avail.y / 2.0;
        area = area.vertical_scroll_offset(y.max(0.0)).horizontal_scroll_offset(((width - avail.x) / 2.0).max(0.0));
        placed = true;
    }

    let mut clicked = None;
    let ppp = ui.ctx().pixels_per_point();
    let (doc_id, pages, source_sizes) = (doc.id, doc.pages.clone(), &doc.sizes);
    let renderer = &mut app.renderer;
    let out = area.show_viewport(ui, |ui, viewport| {
        let content_w = width.max(viewport.width());
        ui.set_min_size(Vec2::new(content_w, height));
        let origin = ui.max_rect().min;
        let first = tops.partition_point(|&t| t < viewport.min.y).saturating_sub(1);
        for i in first..n {
            if tops[i] > viewport.max.y {
                break;
            }
            let size = sizes[i] * scale;
            let rect = Rect::from_min_size(Pos2::new(origin.x + (content_w - size.x) / 2.0, origin.y + tops[i]), size);
            let shadow = egui::epaint::Shadow { offset: Vec2::new(0.0, 1.0), blur: 8.0, spread: 0.0, color: p.shadow };
            ui.painter().add(shadow.as_shape(rect, 0.0));
            ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
            let page = pages[i];
            let source = source_sizes[page.source];
            match renderer.page(doc_id, page.source, source, source.y * scale * ppp, false) {
                Page::Drawn(tex) => paint_texture(ui.painter(), rect, tex, page.rotation, Color32::WHITE),
                Page::Pending => {}
                Page::Failed => {
                    let text = "This page couldn't be shown.";
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, theme::font(12.0), Color32::GRAY);
                }
            }
            if p.dark {
                ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0_f32, p.page_edge));
            }
            if ui.interact(rect, ui.id().with(("page", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }
    });

    // Ctrl + the wheel, or a pinch, zooms.
    let zoom = ui.input(|i| i.zoom_delta());
    if zoom != 1.0 && out.inner_rect.contains(ui.input(|i| i.pointer.hover_pos()).unwrap_or(Pos2::new(-1.0, -1.0))) {
        app.zoom_by(zoom);
    }
    if let Some(i) = clicked {
        app.select_only(i);
        app.reveal_thumb = Some(i);
    }

    // Which page is in view: the one taking up most of it.
    let offset = out.state.offset.y;
    let view_h = out.inner_rect.height();
    let center = offset + view_h / 2.0;
    let at = tops.partition_point(|&t| t <= center).saturating_sub(1);
    app.view_center = (at, ((center - tops[at]) / (sizes[at].y * scale)).clamp(0.0, 1.0));
    if placed {
        app.placed_at = Some(offset);
    } else if app.placed_at.is_some_and(|o| (o - offset).abs() > 0.5) {
        app.placed_at = None;
    }
    if app.placed_at.is_none() {
        let visible = |i: usize| (tops[i] + sizes[i].y * scale).min(offset + view_h) - tops[i].max(offset);
        let first = tops.partition_point(|&t| t < offset).saturating_sub(1);
        let best = (first..n).take_while(|&i| tops[i] < offset + view_h).max_by(|&a, &b| visible(a).total_cmp(&visible(b)).then(b.cmp(&a)));
        if let Some(best) = best.filter(|&b| b != app.current) {
            app.current = best;
            // Scrolling to another page selects it, as in Preview, unless
            // several pages are selected.
            if app.selection.len() <= 1 {
                app.select_only(best);
                app.reveal_thumb = Some(best);
            }
        }
    }
}

/// With no document: an invitation to open one. Whether Open was clicked.
fn empty(ui: &mut egui::Ui, p: &Palette) -> bool {
    let r = ui.max_rect();
    let c = r.center();
    icons::paint(ui.painter(), Icon::Doc, Rect::from_center_size(c - Vec2::new(0.0, 70.0), Vec2::splat(56.0)), p.tertiary);
    ui.painter().text(c - Vec2::new(0.0, 22.0), egui::Align2::CENTER_CENTER, "No PDF Open", theme::bold_font(17.0), p.text);
    ui.painter().text(c + Vec2::new(0.0, 2.0), egui::Align2::CENTER_CENTER, "Open a PDF, or drop one here.", theme::font(13.0), p.secondary);
    let button = Rect::from_center_size(c + Vec2::new(0.0, 40.0), Vec2::new(110.0, 30.0));
    let b = egui::Button::new(egui::RichText::new("Open…").color(Color32::WHITE)).fill(theme::BLUE).rounding(15.0);
    ui.put(button, b).clicked()
}
