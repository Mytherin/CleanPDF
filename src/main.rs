//! CleanPDF: a PDF viewer and page editor in the style of macOS's Preview.

// A windowed app on Windows, in debug builds too: no console window.
#![windows_subsystem = "windows"]

/// The label for a shortcut on egui's "command" modifier: `cmd_key!("S")` is
/// "⌘S" on macOS and "Ctrl+S" elsewhere.
macro_rules! cmd_key {
    ($key:literal) => {
        if cfg!(target_os = "macos") {
            concat!("\u{2318}", $key)
        } else {
            concat!("Ctrl+", $key)
        }
    };
}

mod app;
mod app_icon;
mod document;
mod edit;
mod fonts;
mod icons;
mod menu;
mod render;
mod theme;

fn main() -> eframe::Result {
    // A PDF to open: "Open with", or dropped on the executable.
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("CleanPDF")
            .with_inner_size([1000.0, 820.0])
            .with_min_inner_size([520.0, 360.0])
            .with_icon(app_icon::icon_data(256))
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("CleanPDF", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, path)))))
}
