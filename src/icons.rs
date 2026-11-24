//! Toolbar and menu icons, drawn as shapes so they're crisp at any scale
//! (in the spirit of SF Symbols, as CleanNotes').

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Sidebar,
    Trash,
    ZoomIn,
    ZoomOut,
    RotateLeft,
    RotateRight,
    /// "…": more actions.
    More,
    Check,
    /// A folder: Open.
    Folder,
    /// A tray with an arrow into it: Save.
    Save,
    /// A page: a PDF.
    Doc,
    Undo,
    Redo,
}

fn line(p: &Painter, pts: &[Pos2], s: Stroke) {
    p.add(Shape::line(pts.to_vec(), s));
}

/// Draws `icon` centered in `r`, about `r.height()` tall.
pub fn paint(p: &Painter, icon: Icon, r: Rect, color: Color32) {
    let c = r.center();
    let h = r.height().min(r.width());
    let u = h / 16.0;
    let s = Stroke::new((1.3 * u).max(1.0), color);
    let at = |x: f32, y: f32| Pos2::new(c.x + x * u, c.y + y * u);
    // An arc around (cx, cy) of radius r from angle a0 to a1 (radians,
    // clockwise from 3 o'clock, as screen y points down).
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| -> Vec<Pos2> {
        (0..=24).map(|i| a0 + (a1 - a0) * i as f32 / 24.0).map(|a| at(cx + r * a.cos(), cy + r * a.sin())).collect()
    };
    match icon {
        Icon::Sidebar => {
            let b = Rect::from_center_size(c, Vec2::new(15.0 * u, 12.0 * u));
            p.rect_stroke(b, 2.5 * u, s);
            line(p, &[at(-2.5, -6.0), at(-2.5, 6.0)], s);
            for y in [-3.0, -0.5, 2.0] {
                line(p, &[at(-6.0, y), at(-4.2, y)], s);
            }
        }
        Icon::Trash => {
            line(p, &[at(-6.5, -4.5), at(6.5, -4.5)], s);
            line(p, &[at(-2.5, -4.5), at(-2.0, -6.5), at(2.0, -6.5), at(2.5, -4.5)], s);
            line(p, &[at(-5.0, -4.5), at(-4.2, 6.5), at(4.2, 6.5), at(5.0, -4.5)], s);
            line(p, &[at(-1.6, -1.5), at(-1.4, 4.0)], s);
            line(p, &[at(1.6, -1.5), at(1.4, 4.0)], s);
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            p.circle_stroke(at(-1.2, -1.2), 5.2 * u, s);
            line(p, &[at(2.6, 2.6), at(6.5, 6.5)], Stroke::new(s.width * 1.3, color));
            line(p, &[at(-3.8, -1.2), at(1.4, -1.2)], s);
            if icon == Icon::ZoomIn {
                line(p, &[at(-1.2, -3.8), at(-1.2, 1.4)], s);
            }
        }
        Icon::RotateLeft | Icon::RotateRight => {
            // A page, and an arrow curling over its top corner.
            let flip = if icon == Icon::RotateLeft { 1.0 } else { -1.0 };
            let fx = |x: f32| x * flip;
            let page = [at(fx(-6.0), -1.5), at(fx(2.5), -1.5), at(fx(2.5), 7.0), at(fx(-6.0), 7.0), at(fx(-6.0), -1.5)];
            line(p, &page, s);
            let pts: Vec<Pos2> = arc(0.0, 0.0, 5.5, -std::f32::consts::PI * 0.95, -std::f32::consts::PI * 0.05)
                .into_iter()
                .map(|q| Pos2::new(c.x + (q.x - c.x) * flip, q.y - 1.0 * u))
                .collect();
            let end = *pts.last().unwrap();
            line(p, &pts, s);
            let tip = |dx: f32, dy: f32| Pos2::new(end.x + dx * u * flip, end.y + dy * u);
            line(p, &[tip(-2.4, -1.2), tip(0.0, 0.6), tip(1.6, -1.8)], s);
        }
        Icon::More => {
            for x in [-5.0, 0.0, 5.0] {
                p.circle_filled(at(x, 0.0), 1.3 * u, color);
            }
        }
        Icon::Check => line(p, &[at(-5.5, 0.5), at(-1.8, 4.5), at(5.5, -5.0)], Stroke::new(s.width * 1.2, color)),
        Icon::Folder => {
            line(p, &[at(-7.0, -4.5), at(-2.5, -4.5), at(-1.0, -3.0), at(7.0, -3.0), at(7.0, 5.5), at(-7.0, 5.5), at(-7.0, -4.5)], s);
            line(p, &[at(-7.0, -1.0), at(7.0, -1.0)], s);
        }
        Icon::Save => {
            line(p, &[at(-6.5, 1.5), at(-6.5, 6.5), at(6.5, 6.5), at(6.5, 1.5)], s);
            line(p, &[at(0.0, -7.0), at(0.0, 3.0)], s);
            line(p, &[at(-3.2, -0.2), at(0.0, 3.0), at(3.2, -0.2)], s);
        }
        Icon::Doc => {
            line(p, &[at(1.5, -7.0), at(-5.5, -7.0), at(-5.5, 7.0), at(5.5, 7.0), at(5.5, -3.0), at(1.5, -7.0), at(1.5, -3.0), at(5.5, -3.0)], s);
        }
        Icon::Undo | Icon::Redo => {
            let flip = if icon == Icon::Undo { 1.0 } else { -1.0 };
            let q = |x: f32, y: f32| at(x * flip, y);
            let mut pts = vec![q(-6.0, -2.5), q(2.0, -2.5)];
            pts.extend(arc(0.0, 0.0, 4.0, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2).into_iter().map(|p| Pos2::new(c.x + (p.x - c.x + 2.0 * u) * flip, p.y + 1.5 * u)));
            pts.push(q(-3.0, 5.5));
            line(p, &pts, s);
            line(p, &[q(-3.0, -5.5), q(-6.0, -2.5), q(-3.0, 0.5)], s);
        }
    }
}
