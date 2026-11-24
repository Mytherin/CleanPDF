//! The sidebar of page thumbnails, as Preview's: click to go to a page,
//! Ctrl/Shift-click to select several, drag to reorder, right-click for
//! rotate and delete.

use std::collections::BTreeSet;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};

use super::widgets::paint_texture;
use super::{Action, App};
use crate::icons::Icon;
use crate::menu;
use crate::render::Page;
use crate::theme::{self, Palette};

const WIDTH: f32 = 172.0;
/// The most room a thumbnail takes.
const THUMB: Vec2 = Vec2::new(108.0, 136.0);
/// Around a thumbnail (its selection backdrop), and under it for its number.
const PAD: f32 = 6.0;
const LABEL_H: f32 = 22.0;
const SPACING: f32 = 8.0;

/// Pages being dragged to a new place.
pub struct Drag {
    pages: Vec<usize>,
    /// Where they'd go: before this page (or at the end).
    insert: Option<usize>,
}

pub fn show(app: &mut App, ctx: &egui::Context, p: &Palette) {
    let frame = egui::Frame::none().fill(p.sidebar);
    egui::SidePanel::left("thumbnails").exact_width(WIDTH).resizable(false).frame(frame).show(ctx, |ui| thumbnails(app, ui, p));
}

fn thumbnails(app: &mut App, ui: &mut egui::Ui, p: &Palette) {
    let doc = app.doc.as_ref().unwrap();
    let n = doc.len();
    // Each thumbnail's size, and where its item starts.
    let thumbs: Vec<Vec2> = (0..n)
        .map(|i| {
            let s = doc.page_size(i);
            s * (THUMB.x / s.x).min(THUMB.y / s.y)
        })
        .collect();
    let mut tops = Vec::with_capacity(n);
    let mut y = 14.0;
    for t in &thumbs {
        tops.push(y);
        y += t.y + 2.0 * PAD + LABEL_H + SPACING;
    }
    let height = y + 6.0;
    let item_h = |i: usize| thumbs[i].y + 2.0 * PAD + LABEL_H;

    let mut area = egui::ScrollArea::vertical().id_salt(("thumbs", doc.id)).auto_shrink([false, false]);
    if let Some(i) = app.reveal_thumb.take().filter(|&i| i < n) {
        let (offset, view_h) = app.thumbs_view;
        let (top, bottom) = (tops[i] - SPACING, tops[i] + item_h(i) + SPACING);
        if view_h > 0.0 && (top < offset || bottom > offset + view_h) {
            let to = if top < offset { top } else { bottom - view_h };
            area = area.vertical_scroll_offset(to.max(0.0));
        }
    }

    let ppp = ui.ctx().pixels_per_point();
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let (doc_id, pages, source_sizes) = (doc.id, doc.pages.clone(), &doc.sizes);
    let renderer = &mut app.renderer;
    let (selection, dragging) = (&app.selection, app.drag.as_ref().map(|d| d.pages.clone()));
    let mut clicked: Option<(usize, egui::Modifiers)> = None;
    let mut action: Option<(usize, Action)> = None;
    let mut drag_start = None;
    let mut insert = None;
    let mut ghost = None;

    let out = area.show_viewport(ui, |ui, viewport| {
        ui.set_min_size(Vec2::new(ui.available_width(), height));
        let origin = ui.max_rect().min;
        let w = ui.max_rect().width();
        let first = tops.partition_point(|&t| t < viewport.min.y - THUMB.y).saturating_sub(1);
        for i in first..n {
            if tops[i] > viewport.max.y {
                break;
            }
            let item = Rect::from_min_size(Pos2::new(origin.x, origin.y + tops[i]), Vec2::new(w, item_h(i)));
            let thumb = Rect::from_center_size(Pos2::new(item.center().x, item.top() + PAD + thumbs[i].y / 2.0), thumbs[i]);
            let selected = selection.contains(&i);
            if selected {
                ui.painter().rect_filled(thumb.expand(PAD), 6.0, p.selection_inactive);
            }
            let page = pages[i];
            let source = source_sizes[page.source];
            let tint = if dragging.as_ref().is_some_and(|d| d.contains(&i)) { Color32::from_white_alpha(110) } else { Color32::WHITE };
            ui.painter().rect_filled(thumb, 0.0, tint);
            let drawn = renderer.page(doc_id, page.source, source, source.y * (thumbs[i].y / sizes_y(page.rotation, source)) * ppp, true);
            if let Page::Drawn(tex) = drawn {
                paint_texture(ui.painter(), thumb, tex, page.rotation, tint);
                if dragging.as_ref().is_some_and(|d| d.first() == Some(&i)) {
                    ghost = Some((tex, page.rotation, thumbs[i]));
                }
            }
            ui.painter().rect_stroke(thumb, 0.0, egui::Stroke::new(1.0_f32, p.page_edge));
            // The page number, in a blue pill when selected.
            let label = (i + 1).to_string();
            let font = theme::font(11.5);
            let label_c = Pos2::new(item.center().x, thumb.bottom() + PAD + LABEL_H / 2.0);
            if selected {
                let galley = ui.painter().layout_no_wrap(label.clone(), font.clone(), Color32::WHITE);
                let pill = Rect::from_center_size(label_c, Vec2::new(galley.size().x + 14.0, 17.0));
                ui.painter().rect_filled(pill, 8.5, p.selection);
                ui.painter().galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
            } else {
                ui.painter().text(label_c, egui::Align2::CENTER_CENTER, label, font, p.secondary);
            }

            let resp = ui.interact(thumb.expand(PAD).union(Rect::from_center_size(label_c, Vec2::new(40.0, LABEL_H))), ui.id().with(("thumb", i)), Sense::click_and_drag());
            if resp.clicked() {
                clicked = Some((i, ui.input(|inp| inp.modifiers)));
            }
            if resp.drag_started() {
                drag_start = Some(i);
            }
            resp.context_menu(|ui| {
                menu::begin(ui, 190.0);
                let many = selected && selection.len() > 1;
                if menu::item("Rotate Left").icon(Icon::RotateLeft).show(ui) {
                    action = Some((i, Action::RotateLeft));
                }
                if menu::item("Rotate Right").icon(Icon::RotateRight).show(ui) {
                    action = Some((i, Action::RotateRight));
                }
                let delete = if many { "Delete Pages" } else { "Delete Page" };
                if menu::item(delete).icon(Icon::Trash).enabled(n > 1 && !(many && selection.len() >= n)).show(ui) {
                    action = Some((i, Action::Delete));
                }
            });
        }

        // Where dragged pages would go: before the first page whose middle
        // is below the pointer.
        if let (Some(_), Some(pos)) = (&dragging, pointer) {
            let clip = ui.clip_rect();
            if clip.contains(pos) {
                let y = pos.y - origin.y;
                let at = (0..n).find(|&i| tops[i] + item_h(i) / 2.0 > y).unwrap_or(n);
                insert = Some(at);
                let line_y = origin.y + if at < n { tops[at] - SPACING / 2.0 } else { tops[n - 1] + item_h(n - 1) + SPACING / 2.0 };
                let stroke = egui::Stroke::new(2.5_f32, theme::BLUE);
                ui.painter().hline(origin.x + 18.0..=origin.x + w - 18.0, line_y, stroke);
                ui.painter().circle_stroke(Pos2::new(origin.x + 15.0, line_y), 3.0, stroke);
                // Near an edge: scroll that way.
                let edge = 36.0;
                if pos.y < clip.top() + edge {
                    ui.scroll_with_delta(Vec2::new(0.0, 6.0));
                } else if pos.y > clip.bottom() - edge {
                    ui.scroll_with_delta(Vec2::new(0.0, -6.0));
                }
                ui.ctx().request_repaint();
            }
        }
    });
    app.thumbs_view = (out.state.offset.y, out.inner_rect.height());

    // The pages being dragged follow the pointer.
    if let (Some((tex, rotation, size)), Some(pos)) = (ghost, pointer) {
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag-ghost")));
        let rect = Rect::from_center_size(pos + Vec2::new(10.0, 10.0), size * 0.6);
        paint_texture(&painter, rect, tex, rotation, Color32::from_white_alpha(200));
        let count = dragging.as_ref().map_or(1, |d| d.len());
        if count > 1 {
            let badge = Pos2::new(rect.right(), rect.top());
            painter.circle_filled(badge, 10.0, theme::RED);
            painter.text(badge, egui::Align2::CENTER_CENTER, count.to_string(), theme::bold_font(11.0), Color32::WHITE);
        }
    }

    if let Some(i) = drag_start {
        let pages: Vec<usize> = if app.selection.contains(&i) { app.selection.iter().copied().collect() } else { vec![i] };
        if !app.selection.contains(&i) {
            app.select_only(i);
        }
        app.drag = Some(Drag { pages, insert: None });
    }
    if let Some(drag) = &mut app.drag {
        drag.insert = insert;
        if ui.input(|i| i.pointer.any_released() || !i.pointer.any_down()) {
            let drag = app.drag.take().unwrap();
            if let (Some(to), Some(doc)) = (drag.insert, &mut app.doc) {
                let start = doc.move_pages(&drag.pages, to);
                app.selection = (start..start + drag.pages.len()).collect::<BTreeSet<_>>();
                app.anchor = Some(start);
                app.current = start;
                app.reveal_thumb = Some(start);
            }
        }
    }
    if let Some((i, m)) = clicked {
        if m.command {
            if !app.selection.remove(&i) {
                app.selection.insert(i);
            }
            app.anchor = Some(i);
        } else if m.shift {
            let a = app.anchor.unwrap_or(i);
            app.selection = (a.min(i)..=a.max(i)).collect();
        } else {
            app.go_to(i, false);
        }
    }
    if let Some((i, a)) = action {
        // On a page outside the selection, it's that page's alone.
        if !app.selection.contains(&i) {
            app.select_only(i);
        }
        app.run(ui.ctx(), a);
    }
}

/// The height, in points, a page of unturned `size` has when turned by
/// `rotation`.
fn sizes_y(rotation: u16, size: Vec2) -> f32 {
    if rotation % 180 == 90 {
        size.x
    } else {
        size.y
    }
}
