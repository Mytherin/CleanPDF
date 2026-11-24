//! The toolbar: the sidebar button and the document's name on the left;
//! zoom, rotate, delete and a "…" menu of everything on the right.

use eframe::egui::{self, Pos2, Rect, Vec2};

use super::widgets::{capsule_item, glass, glass_button, tool_button};
use super::{Action, App, Zoom};
use crate::icons::Icon;
use crate::menu;
use crate::theme::{self, Palette};

const HEIGHT: f32 = 52.0;

pub fn show(app: &mut App, ctx: &egui::Context, p: &Palette) {
    let mut actions = Vec::new();
    egui::TopBottomPanel::top("toolbar").exact_height(HEIGHT).frame(egui::Frame::none().fill(p.bg)).show(ctx, |ui| {
        let r = ui.max_rect();
        let cy = r.center().y;
        let has_doc = app.doc.is_some();

        let button = Rect::from_center_size(Pos2::new(r.left() + 30.0, cy), Vec2::new(36.0, 28.0));
        let tip = if app.sidebar { "Hide Sidebar" } else { "Show Sidebar" };
        if tool_button(ui, p, button, Icon::Sidebar, tip, has_doc, false) {
            actions.push(Action::ToggleSidebar);
        }

        // From the right: "…", delete, rotate, zoom.
        let mut x = r.right() - 14.0;
        let more = glass_button(ui, p, Pos2::new(x - 17.0, cy), Icon::More, "More", true);
        let menu_id = ui.make_persistent_id("more-menu");
        if more.clicked() {
            ui.memory_mut(|m| m.toggle_popup(menu_id));
        }
        egui::popup_below_widget(ui, menu_id, &more, egui::PopupCloseBehavior::CloseOnClick, |ui| more_menu(app, ui, &mut actions));
        x -= 34.0 + 12.0;
        let n = app.targets().len();
        let tip = if n > 1 { "Delete Pages" } else { "Delete Page" };
        if glass_button(ui, p, Pos2::new(x - 17.0, cy), Icon::Trash, tip, app.can(Action::Delete)).clicked() {
            actions.push(Action::Delete);
        }
        x -= 34.0 + 8.0;
        if glass_button(ui, p, Pos2::new(x - 17.0, cy), Icon::RotateLeft, "Rotate Left", has_doc).clicked() {
            actions.push(Action::RotateLeft);
        }
        x -= 34.0 + 12.0;
        let capsule = Rect::from_min_size(Pos2::new(x - 76.0, cy - 17.0), Vec2::new(76.0, 34.0));
        glass(ui, p, capsule);
        let (left, right) = capsule.split_left_right_at_fraction(0.5);
        if capsule_item(ui, p, left, Icon::ZoomOut, "Zoom Out", has_doc && app.scale > super::MIN_ZOOM).clicked() {
            actions.push(Action::ZoomOut);
        }
        if capsule_item(ui, p, right, Icon::ZoomIn, "Zoom In", has_doc && app.scale < super::MAX_ZOOM).clicked() {
            actions.push(Action::ZoomIn);
        }
        let title_right = capsule.left() - 12.0;

        // The name, "Edited", and which page is in view.
        let left = r.left() + 58.0;
        let clip = Rect::from_x_y_ranges(left..=title_right.max(left), r.y_range());
        let painter = ui.painter().with_clip_rect(clip);
        let (name, sub) = match &app.doc {
            Some(doc) => (doc.name(), format!("Page {} of {}", app.current + 1, doc.len())),
            None => ("CleanPDF".to_owned(), String::new()),
        };
        let edited = app.doc.as_ref().is_some_and(|d| d.edited());
        let name_y = if sub.is_empty() { cy } else { cy - 8.0 };
        let shown = painter.text(Pos2::new(left, name_y), egui::Align2::LEFT_CENTER, name, theme::bold_font(13.5), p.text);
        if edited {
            painter.text(Pos2::new(shown.right() + 6.0, name_y), egui::Align2::LEFT_CENTER, "Edited", theme::font(12.0), p.secondary);
        }
        if !sub.is_empty() {
            painter.text(Pos2::new(left, cy + 9.0), egui::Align2::LEFT_CENTER, sub, theme::font(11.5), p.secondary);
        }
    });
    for a in actions {
        app.run(ctx, a);
    }
}

fn more_menu(app: &App, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    menu::begin(ui, 230.0);
    let mut row = |ui: &mut egui::Ui, label: &str, icon: Option<Icon>, key: &str, action: Action, checked: bool| {
        let mut item = menu::item(label).shortcut(key).enabled(app.can(action)).checked(checked);
        if let Some(icon) = icon {
            item = item.icon(icon);
        }
        if item.show(ui) {
            actions.push(action);
        }
    };
    let pages = if app.targets().len() > 1 { "Delete Pages" } else { "Delete Page" };
    row(ui, "Open…", Some(Icon::Folder), cmd_key!("O"), Action::Open, false);
    row(ui, "Save", Some(Icon::Save), cmd_key!("S"), Action::Save, false);
    row(ui, "Save As…", None, cmd_key!("Shift+S"), Action::SaveAs, false);
    menu::separator(ui);
    row(ui, "Undo", Some(Icon::Undo), cmd_key!("Z"), Action::Undo, false);
    row(ui, "Redo", Some(Icon::Redo), cmd_key!("Shift+Z"), Action::Redo, false);
    menu::separator(ui);
    row(ui, "Rotate Left", Some(Icon::RotateLeft), cmd_key!("L"), Action::RotateLeft, false);
    row(ui, "Rotate Right", Some(Icon::RotateRight), cmd_key!("R"), Action::RotateRight, false);
    row(ui, pages, Some(Icon::Trash), "Delete", Action::Delete, false);
    row(ui, "Select All Pages", None, cmd_key!("A"), Action::SelectAll, false);
    menu::separator(ui);
    row(ui, "Zoom In", Some(Icon::ZoomIn), cmd_key!("+"), Action::ZoomIn, false);
    row(ui, "Zoom Out", Some(Icon::ZoomOut), cmd_key!("−"), Action::ZoomOut, false);
    row(ui, "Actual Size", None, cmd_key!("0"), Action::ActualSize, app.doc.is_some() && app.zoom == Zoom::Scale(1.0));
    row(ui, "Fit Width", None, cmd_key!("9"), Action::FitWidth, app.doc.is_some() && app.zoom == Zoom::FitWidth);
    row(ui, "Show Sidebar", Some(Icon::Sidebar), "", Action::ToggleSidebar, app.doc.is_some() && app.sidebar);
}
