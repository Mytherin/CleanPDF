//! The window: a toolbar, the pages' thumbnails in a sidebar, and the pages
//! themselves in a scrolling column, as Preview's.

use std::collections::BTreeSet;
use std::path::PathBuf;

use eframe::egui::{self, Key, Modifiers};

use crate::document::Document;
use crate::render::Renderer;
use crate::theme::{self, Palette};

mod pages;
mod thumbnails;
mod toolbar;
mod widgets;

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 8.0;
/// How much Zoom In and Zoom Out zoom.
const ZOOM_STEP: f32 = 1.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// The widest page fills the window's width.
    FitWidth,
    /// Points on screen per point of the PDF.
    Scale(f32),
}

/// What the toolbar, menus and keys do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Save,
    SaveAs,
    Close,
    Undo,
    Redo,
    RotateLeft,
    RotateRight,
    Delete,
    SelectAll,
    ZoomIn,
    ZoomOut,
    ActualSize,
    FitWidth,
    ToggleSidebar,
}

enum Dialog {
    Error { title: String, detail: String },
    /// Changes would be lost: save them first?
    Unsaved(After),
}

/// What to do once the changes are saved (or not).
#[derive(Clone)]
enum After {
    Close,
    /// Open this file, or ask which.
    Open(Option<PathBuf>),
}

pub struct App {
    doc: Option<Document>,
    renderer: Renderer,
    /// The selected pages (indices into the document's pages).
    selection: BTreeSet<usize>,
    /// Where a Shift-click selection extends from.
    anchor: Option<usize>,
    zoom: Zoom,
    /// The scale the pages were last shown at.
    scale: f32,
    sidebar: bool,
    /// The page in view.
    current: usize,
    /// The page and how far down it (0..1) the middle of the view was.
    view_center: (usize, f32),
    /// The view is to scroll to this page's top.
    scroll_to: Option<usize>,
    /// The view is to keep this point (as `view_center`) in its middle (it
    /// was zoomed).
    keep_center: Option<(usize, f32)>,
    /// The scroll position the view was put at, until scrolled by hand.
    placed_at: Option<f32>,
    /// The sidebar is to scroll this page's thumbnail into view.
    reveal_thumb: Option<usize>,
    /// The sidebar's scroll position and height, as last shown.
    thumbs_view: (f32, f32),
    drag: Option<thumbnails::Drag>,
    dialog: Option<Dialog>,
    /// Closing was confirmed: don't ask again.
    may_close: bool,
    /// Close the window at the end of this frame.
    close_now: bool,
    title: String,
    dark: Option<bool>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> App {
        theme::install_fonts(&cc.egui_ctx);
        // Ctrl +/- zoom the pages, not the whole window.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut app = App::with_renderer(Renderer::new(&cc.egui_ctx));
        if let Some(path) = path {
            app.open(Some(path));
        }
        app
    }

    fn with_renderer(renderer: Renderer) -> App {
        App {
            doc: None,
            renderer,
            selection: BTreeSet::new(),
            anchor: None,
            zoom: Zoom::FitWidth,
            scale: 1.0,
            sidebar: true,
            current: 0,
            view_center: (0, 0.0),
            scroll_to: None,
            keep_center: None,
            placed_at: None,
            reveal_thumb: None,
            thumbs_view: (0.0, 0.0),
            drag: None,
            dialog: None,
            may_close: false,
            close_now: false,
            title: String::new(),
            dark: None,
        }
    }

    pub fn run(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Open => self.request(After::Open(None)),
            Action::Save => {
                self.save();
            }
            Action::SaveAs => {
                self.save_as();
            }
            Action::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::Undo => {
                if self.doc.as_mut().is_some_and(|d| d.undo()) {
                    self.after_change();
                }
            }
            Action::Redo => {
                if self.doc.as_mut().is_some_and(|d| d.redo()) {
                    self.after_change();
                }
            }
            Action::RotateLeft => self.rotate(-90),
            Action::RotateRight => self.rotate(90),
            Action::Delete => self.delete(),
            Action::SelectAll => {
                if let Some(doc) = &self.doc {
                    self.selection = (0..doc.len()).collect();
                }
            }
            Action::ZoomIn => self.zoom_by(ZOOM_STEP),
            Action::ZoomOut => self.zoom_by(1.0 / ZOOM_STEP),
            Action::ActualSize => self.set_zoom(Zoom::Scale(1.0)),
            Action::FitWidth => self.set_zoom(Zoom::FitWidth),
            Action::ToggleSidebar => self.sidebar = !self.sidebar,
        }
    }

    /// Whether `action` can be done now.
    pub fn can(&self, action: Action) -> bool {
        let Some(doc) = &self.doc else { return matches!(action, Action::Open | Action::Close) };
        match action {
            Action::Save => doc.edited(),
            Action::Undo => doc.can_undo(),
            Action::Redo => doc.can_redo(),
            Action::Delete => self.targets().len() < doc.len(),
            _ => true,
        }
    }

    /// Opens `path` (or a file picked), once changes are saved or let go.
    fn request(&mut self, after: After) {
        if self.doc.as_ref().is_some_and(|d| d.edited()) {
            self.dialog = Some(Dialog::Unsaved(after));
        } else {
            self.proceed(after);
        }
    }

    fn proceed(&mut self, after: After) {
        match after {
            After::Open(path) => self.open(path),
            After::Close => {
                self.may_close = true;
                self.close_now = true;
            }
        }
    }

    /// Opens `path`, or a PDF picked in a dialog.
    fn open(&mut self, path: Option<PathBuf>) {
        let path = match path {
            Some(p) => p,
            None => {
                let mut dialog = rfd::FileDialog::new().set_title("Open").add_filter("PDF Documents", &["pdf"]);
                if let Some(dir) = self.doc.as_ref().and_then(|d| d.path.parent()) {
                    dialog = dialog.set_directory(dir);
                }
                match dialog.pick_file() {
                    Some(p) => p,
                    None => return,
                }
            }
        };
        match Document::open(&path) {
            Ok(doc) => {
                self.renderer.set_document(doc.id, doc.bytes.clone());
                self.doc = Some(doc);
                self.selection = BTreeSet::from([0]);
                self.anchor = Some(0);
                self.current = 0;
                self.view_center = (0, 0.0);
                self.zoom = Zoom::FitWidth;
                self.scroll_to = Some(0);
                self.thumbs_view = (0.0, 0.0);
                self.drag = None;
            }
            Err(e) => {
                let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
                self.error(format!("“{name}” couldn't be opened."), e);
            }
        }
    }

    fn error(&mut self, title: String, e: anyhow::Error) {
        let detail = format!("{e:#}");
        let detail = detail[..1].to_uppercase() + &detail[1..] + ".";
        self.dialog = Some(Dialog::Error { title, detail });
    }

    /// Saves to the file it came from. Whether it was saved.
    fn save(&mut self) -> bool {
        let Some(doc) = &mut self.doc else { return false };
        let path = doc.path.clone();
        match doc.save_as(&path) {
            Ok(()) => true,
            Err(e) => {
                let name = doc.name();
                self.error(format!("“{name}” couldn't be saved."), e);
                false
            }
        }
    }

    /// Saves to a file picked. Whether it was saved.
    fn save_as(&mut self) -> bool {
        let Some(doc) = &mut self.doc else { return false };
        let mut dialog = rfd::FileDialog::new().set_title("Save As").add_filter("PDF Documents", &["pdf"]).set_file_name(doc.name());
        if let Some(dir) = doc.path.parent() {
            dialog = dialog.set_directory(dir);
        }
        let Some(mut path) = dialog.save_file() else { return false };
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            path.as_mut_os_string().push(".pdf");
        }
        match doc.save_as(&path) {
            Ok(()) => true,
            Err(e) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.error(format!("“{name}” couldn't be saved."), e);
                false
            }
        }
    }

    /// The pages an action applies to: the selected ones, else the one in
    /// view.
    fn targets(&self) -> Vec<usize> {
        if self.selection.is_empty() {
            vec![self.current]
        } else {
            self.selection.iter().copied().collect()
        }
    }

    fn delete(&mut self) {
        let targets = self.targets();
        let Some(doc) = &mut self.doc else { return };
        if targets.len() >= doc.len() {
            let detail = "A PDF needs at least one page: select fewer pages to delete.";
            self.dialog = Some(Dialog::Error { title: "All pages can't be deleted.".into(), detail: detail.into() });
            return;
        }
        if doc.delete(&targets) {
            let next = targets[0].min(doc.len() - 1);
            self.select_only(next);
            self.current = next;
            self.reveal_thumb = Some(next);
        }
    }

    fn rotate(&mut self, degrees: i32) {
        let targets = self.targets();
        if let Some(doc) = &mut self.doc {
            doc.rotate(&targets, degrees);
        }
    }

    /// After an undo or redo: the selection and page in view stay within
    /// the pages.
    fn after_change(&mut self) {
        let Some(doc) = &self.doc else { return };
        let n = doc.len();
        self.selection.retain(|&i| i < n);
        self.current = self.current.min(n - 1);
        if self.selection.is_empty() {
            self.select_only(self.current);
        }
    }

    fn select_only(&mut self, i: usize) {
        self.selection = BTreeSet::from([i]);
        self.anchor = Some(i);
    }

    /// Selects page `i` (`extend`ing the selection from its anchor) and
    /// scrolls to it.
    fn go_to(&mut self, i: usize, extend: bool) {
        let Some(doc) = &self.doc else { return };
        let i = i.min(doc.len() - 1);
        match (extend, self.anchor) {
            (true, Some(a)) => self.selection = (a.min(i)..=a.max(i)).collect(),
            _ => self.select_only(i),
        }
        self.current = i;
        self.scroll_to = Some(i);
        self.reveal_thumb = Some(i);
    }

    fn set_zoom(&mut self, zoom: Zoom) {
        if zoom != self.zoom {
            self.zoom = zoom;
            self.keep_center = Some(self.view_center);
        }
    }

    fn zoom_by(&mut self, factor: f32) {
        let scale = (self.scale * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.set_zoom(Zoom::Scale(scale));
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use Action::*;
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let keys: &[(Modifiers, Key, Action)] = &[
            (cmd, Key::O, Open),
            (cmd, Key::S, Save),
            (cmd_shift, Key::S, SaveAs),
            (cmd, Key::W, Close),
            (cmd, Key::Z, Undo),
            (cmd_shift, Key::Z, Redo),
            (cmd, Key::Y, Redo),
            (cmd, Key::L, RotateLeft),
            (cmd, Key::R, RotateRight),
            (Modifiers::NONE, Key::Delete, Delete),
            (Modifiers::NONE, Key::Backspace, Delete),
            (cmd, Key::A, SelectAll),
            (cmd, Key::Equals, ZoomIn),
            (cmd, Key::Plus, ZoomIn),
            (cmd_shift, Key::Equals, ZoomIn),
            (cmd, Key::Minus, ZoomOut),
            (cmd, Key::Num0, ActualSize),
            (cmd, Key::Num9, FitWidth),
        ];
        let hit: Vec<Action> = ctx.input_mut(|i| keys.iter().filter(|(m, k, _)| consume_exact(i, *m, *k)).map(|(_, _, a)| *a).collect());
        for a in hit {
            if self.can(a) {
                self.run(ctx, a);
            }
        }
        // Moving between pages; with Shift, selecting as it goes.
        let Some(n) = self.doc.as_ref().map(|d| d.len()) else { return };
        let from = self.current;
        let moves: &[(Key, isize)] = &[
            (Key::ArrowUp, -1),
            (Key::ArrowLeft, -1),
            (Key::PageUp, -1),
            (Key::ArrowDown, 1),
            (Key::ArrowRight, 1),
            (Key::PageDown, 1),
            (Key::Home, isize::MIN),
            (Key::End, isize::MAX),
        ];
        for &(key, d) in moves {
            for (m, extend) in [(Modifiers::NONE, false), (Modifiers::SHIFT, true)] {
                if ctx.input_mut(|i| consume_exact(i, m, key)) {
                    let to = match d {
                        isize::MIN => 0,
                        isize::MAX => n - 1,
                        d => (from as isize + d).clamp(0, n as isize - 1) as usize,
                    };
                    self.go_to(to, extend);
                }
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(dialog) = &self.dialog else { return };
        match dialog {
            Dialog::Error { title, detail } => {
                let (title, detail) = (title.clone(), detail.clone());
                let mut ok = false;
                widgets::modal(ctx, "error", p, |ui| {
                    ui.set_width(320.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(&title).font(theme::bold_font(14.0)).color(p.text));
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new(&detail).size(12.0).color(p.secondary));
                        ui.add_space(10.0);
                        let b = egui::Button::new(egui::RichText::new("OK").color(egui::Color32::WHITE)).fill(theme::BLUE);
                        ok = ui.add_sized([ui.available_width(), 26.0], b).clicked() || ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape));
                    });
                });
                if ok {
                    self.dialog = None;
                }
            }
            Dialog::Unsaved(after) => {
                let after = after.clone();
                let name = self.doc.as_ref().map(|d| d.name()).unwrap_or_default();
                let mut answer = None;
                widgets::modal(ctx, "unsaved", p, |ui| {
                    ui.set_width(320.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);
                        let title = format!("Do you want to save the changes made to “{name}”?");
                        ui.label(egui::RichText::new(title).font(theme::bold_font(14.0)).color(p.text));
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new("Your changes will be lost if you don't save them.").size(12.0).color(p.secondary));
                        ui.add_space(10.0);
                    });
                    let save = egui::Button::new(egui::RichText::new("Save").color(egui::Color32::WHITE)).fill(theme::BLUE);
                    if ui.add_sized([ui.available_width(), 26.0], save).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        answer = Some(Some(true));
                    }
                    ui.add_space(2.0);
                    if ui.add_sized([ui.available_width(), 26.0], egui::Button::new("Don't Save")).clicked() {
                        answer = Some(Some(false));
                    }
                    ui.add_space(2.0);
                    if ui.add_sized([ui.available_width(), 26.0], egui::Button::new("Cancel")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        answer = Some(None);
                    }
                });
                match answer {
                    Some(Some(save)) => {
                        self.dialog = None;
                        if !save || self.save() {
                            self.proceed(after);
                        }
                    }
                    Some(None) => self.dialog = None,
                    None => {}
                }
            }
        }
    }

    /// The window's title: the file's name, and whether it's edited.
    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.doc {
            Some(d) if d.edited() => format!("{} — Edited", d.name()),
            Some(d) => d.name(),
            None => "CleanPDF".to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// Files dropped on the window: the first PDF is opened.
    fn dropped_files(&mut self, ctx: &egui::Context, p: &Palette) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering {
            let screen = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(if p.dark { 60 } else { 20 }));
            painter.rect_stroke(screen.shrink(6.0), 10.0, egui::Stroke::new(3.0_f32, theme::BLUE));
        }
        let dropped: Option<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            self.request(After::Open(Some(path)));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dark = ctx.style().visuals.dark_mode;
        if self.dark != Some(dark) {
            theme::apply_style(ctx);
            self.dark = Some(dark);
        }
        let p = Palette::get(ctx);

        if ctx.input(|i| i.viewport().close_requested()) && !self.may_close && self.doc.as_ref().is_some_and(|d| d.edited()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::Unsaved(After::Close));
        }

        self.renderer.begin_frame(ctx);
        if self.dialog.is_none() {
            self.dropped_files(ctx, &p);
            self.shortcuts(ctx);
        }
        toolbar::show(self, ctx, &p);
        if self.doc.is_some() && self.sidebar {
            thumbnails::show(self, ctx, &p);
        }
        pages::show(self, ctx, &p);
        self.dialogs(ctx, &p);
        self.renderer.end_frame();
        self.update_title(ctx);
        if std::mem::take(&mut self.close_now) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Consumes a press of `key` with exactly these modifiers (⌘ meaning Ctrl
/// off macOS). Unlike egui's `consume_shortcut`, extra Shift/Alt don't match.
fn consume_exact(i: &mut egui::InputState, modifiers: Modifiers, key: Key) -> bool {
    let mut found = false;
    i.events.retain(|e| {
        let hit = matches!(e, egui::Event::Key { key: k, pressed: true, modifiers: m, .. } if *k == key && m.matches_exact(modifiers));
        found |= hit;
        !hit
    });
    found
}
