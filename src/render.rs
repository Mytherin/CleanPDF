//! Pages drawn (by hayro) into textures, on worker threads.
//!
//! Each frame the view asks for the pages it shows, at the size it shows
//! them ([`Renderer::page`]); what isn't drawn yet becomes the workers' job
//! list for that frame, in the order asked (the main view's pages before the
//! thumbnails), so pages scrolled past before their turn are never drawn.
//! Meanwhile the nearest size already drawn stands in, scaled.
//!
//! Sizes are rounded up to steps of a quarter octave, so zooming draws a
//! page again only every ~19%. Drawn pages are kept up to [`MAX_PIXELS`],
//! letting those looked at longest ago go.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions, Vec2};

/// Pages are drawn at most this many pixels on a side (textures' limit).
const MAX_SIDE: f32 = 8192.0;
/// The most pixels kept drawn, over all pages (4 bytes each).
const MAX_PIXELS: usize = 96 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    doc: u64,
    source: usize,
    /// In pixels.
    height: u32,
}

enum Slot {
    /// The texture, when it was last shown, and its pixel count.
    Ready(TextureHandle, u64, usize),
    Failed,
}

#[derive(Default)]
struct State {
    /// The document being shown: its id and bytes.
    doc: Option<(u64, Arc<Vec<u8>>)>,
    jobs: Vec<Key>,
    busy: HashSet<Key>,
    done: Vec<(Key, Option<ColorImage>)>,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    ctx: egui::Context,
}

fn lock(s: &Shared) -> MutexGuard<'_, State> {
    s.state.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct Renderer {
    shared: Arc<Shared>,
    textures: HashMap<Key, Slot>,
    tick: u64,
    /// The pages asked for this frame and not drawn: the main view's, then
    /// thumbnails.
    wanted: Vec<Key>,
    wanted_thumbs: Vec<Key>,
}

/// What can be shown of a page now.
pub enum Page {
    Drawn(TextureId),
    /// Not drawn yet (it's being drawn): shown blank.
    Pending,
    /// It couldn't be drawn.
    Failed,
}

impl Renderer {
    pub fn new(ctx: &egui::Context) -> Renderer {
        let shared = Arc::new(Shared { state: Mutex::default(), wake: Condvar::new(), ctx: ctx.clone() });
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(1)).clamp(1, 4);
        for _ in 0..threads {
            let shared = shared.clone();
            std::thread::Builder::new().name("render".into()).spawn(move || worker(&shared)).expect("a render thread");
        }
        Renderer { shared, textures: HashMap::new(), tick: 0, wanted: Vec::new(), wanted_thumbs: Vec::new() }
    }

    /// Shows document `id` (with these bytes) from now on; another's pages
    /// are let go.
    pub fn set_document(&mut self, id: u64, bytes: Arc<Vec<u8>>) {
        let mut st = lock(&self.shared);
        st.doc = Some((id, bytes));
        st.jobs.clear();
        st.done.clear();
        self.textures.retain(|k, _| k.doc == id);
    }

    /// Call before asking for pages: takes in what the workers drew.
    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        self.tick += 1;
        self.wanted.clear();
        self.wanted_thumbs.clear();
        let done = std::mem::take(&mut lock(&self.shared).done);
        for (key, img) in done {
            let slot = match img {
                Some(img) => {
                    let pixels = img.pixels.len();
                    let tex = ctx.load_texture(format!("page:{}:{}:{}", key.doc, key.source, key.height), img, TextureOptions::LINEAR);
                    Slot::Ready(tex, self.tick, pixels)
                }
                None => Slot::Failed,
            };
            self.textures.insert(key, slot);
        }
    }

    /// Page `source` of document `doc` (whose size in points, unturned, is
    /// `size`), `height` pixels tall: drawn, or the nearest size drawn. A
    /// `thumb`nail is drawn after the main view's pages.
    pub fn page(&mut self, doc: u64, source: usize, size: Vec2, height: f32, thumb: bool) -> Page {
        let key = Key { doc, source, height: bucket(size, height) };
        match self.textures.get_mut(&key) {
            Some(Slot::Ready(tex, used, _)) => {
                *used = self.tick;
                return Page::Drawn(tex.id());
            }
            Some(Slot::Failed) => return Page::Failed,
            None => {}
        }
        if !self.wanted.contains(&key) && !self.wanted_thumbs.contains(&key) {
            if thumb { &mut self.wanted_thumbs } else { &mut self.wanted }.push(key);
        }
        let tick = self.tick;
        let nearest = self
            .textures
            .iter_mut()
            .filter_map(|(k, s)| match s {
                Slot::Ready(tex, used, _) if k.doc == doc && k.source == source => Some((k.height.abs_diff(key.height), tex, used)),
                _ => None,
            })
            .min_by_key(|(d, _, _)| *d);
        match nearest {
            Some((_, tex, used)) => {
                *used = tick;
                Page::Drawn(tex.id())
            }
            None => Page::Pending,
        }
    }

    /// Call after asking for pages: what isn't drawn becomes the workers'
    /// jobs, and pages not shown lately are let go.
    pub fn end_frame(&mut self) {
        {
            let mut st = lock(&self.shared);
            let (busy, done) = (&st.busy, &st.done);
            let jobs: Vec<Key> = self.wanted.iter().chain(&self.wanted_thumbs).copied().filter(|k| !busy.contains(k) && !done.iter().any(|(d, _)| d == k)).collect();
            st.jobs = jobs;
            if !st.jobs.is_empty() {
                self.shared.wake.notify_all();
            }
        }
        self.evict();
    }

    fn evict(&mut self) {
        let mut total: usize = self.textures.values().map(|s| if let Slot::Ready(_, _, n) = s { *n } else { 0 }).sum();
        if total <= MAX_PIXELS {
            return;
        }
        let mut old: Vec<(u64, Key, usize)> = self
            .textures
            .iter()
            .filter_map(|(k, s)| match s {
                Slot::Ready(_, used, n) if *used < self.tick => Some((*used, *k, *n)),
                _ => None,
            })
            .collect();
        old.sort_unstable_by_key(|(used, _, _)| *used);
        for (_, key, n) in old {
            if total <= MAX_PIXELS {
                break;
            }
            self.textures.remove(&key);
            total -= n;
        }
    }
}

/// `height` pixels rounded up to a quarter-octave step, and kept within
/// [`MAX_SIDE`] on both sides for a page of `size`.
fn bucket(size: Vec2, height: f32) -> u32 {
    let step = (height.max(16.0).log2() * 4.0).ceil() / 4.0;
    let limit = MAX_SIDE.min(MAX_SIDE * size.y / size.x.max(1.0));
    2f32.powf(step).min(limit).round().max(1.0) as u32
}

fn worker(shared: &Shared) {
    // The document last drawn from, read once.
    let mut parsed: Option<(u64, Option<hayro::hayro_syntax::Pdf>)> = None;
    loop {
        let (key, bytes) = {
            let mut st = lock(shared);
            loop {
                let doc = st.doc.as_ref().map(|(id, _)| *id);
                st.jobs.retain(|k| Some(k.doc) == doc);
                if let Some(pos) = st.jobs.iter().position(|k| !st.busy.contains(k)) {
                    let key = st.jobs.remove(pos);
                    st.busy.insert(key);
                    break (key, st.doc.as_ref().unwrap().1.clone());
                }
                st = shared.wake.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        };
        if parsed.as_ref().map(|(id, _)| *id) != Some(key.doc) {
            parsed = Some((key.doc, hayro::hayro_syntax::Pdf::new(bytes).ok()));
        }
        let pdf = parsed.as_ref().and_then(|(_, pdf)| pdf.as_ref());
        // (A page that panics hayro is one that can't be drawn.)
        let img = pdf.and_then(|pdf| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(pdf, key.source, key.height))).ok().flatten());
        let mut st = lock(shared);
        st.busy.remove(&key);
        if st.doc.as_ref().map(|(id, _)| *id) == Some(key.doc) {
            st.done.push((key, img));
            shared.ctx.request_repaint();
        }
    }
}

/// Page `index`, `height` pixels tall, on white.
pub(crate) fn render(pdf: &hayro::hayro_syntax::Pdf, index: usize, height: u32) -> Option<ColorImage> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::vello_cpu::color::palette::css::WHITE;
    let pages = pdf.pages();
    let page = pages.get(index)?;
    let (w, h) = page.render_dimensions();
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let scale = height as f32 / h;
    let settings = hayro::PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
    let pixmap = hayro::render(page, &hayro::RenderCache::new(), &InterpreterSettings::default(), &hayro::RenderSettings::default(), &settings);
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    if size[0] == 0 || size[1] == 0 {
        return None;
    }
    // (Over a white page every pixel is opaque: premultiplied or not is the
    // same.)
    Some(ColorImage::from_rgba_unmultiplied(size, pixmap.data_as_u8_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::tests::test_pdf;

    #[test]
    fn sizes_go_in_steps_within_limits() {
        let a4 = Vec2::new(595.0, 842.0);
        assert_eq!(bucket(a4, 1000.0), 1024);
        assert_eq!(bucket(a4, 1024.0), 1024);
        assert_eq!(bucket(a4, 1025.0), 1218);
        assert_eq!(bucket(a4, 3.0), 16);
        assert_eq!(bucket(a4, 1e6), 8192);
        // A wide page is held to MAX_SIDE across.
        assert_eq!(bucket(Vec2::new(2000.0, 500.0), 1e6), 2048);
    }

    #[test]
    fn draws_pages() {
        let bytes = test_pdf(&[(200, 100), (100, 300)]);
        let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(bytes)).unwrap();
        let img = render(&pdf, 0, 500).expect("the page is drawn");
        assert_eq!(img.size, [1000, 500]);
        let px = |x: usize, y: usize| img.pixels[y * img.size[0] + x];
        assert_eq!(px(5, 5), egui::Color32::WHITE);
        assert_eq!(px(500, 250), egui::Color32::BLACK);
        assert!(render(&pdf, 2, 500).is_none());
    }

    #[test]
    fn the_renderer_draws_what_is_asked_for() {
        let ctx = egui::Context::default();
        let bytes = Arc::new(test_pdf(&[(100, 100); 3]));
        let mut r = Renderer::new(&ctx);
        r.set_document(7, bytes);
        let size = Vec2::new(100.0, 100.0);
        let mut drawn = false;
        for _ in 0..500 {
            r.begin_frame(&ctx);
            drawn = matches!(r.page(7, 1, size, 200.0, false), Page::Drawn(_));
            r.end_frame();
            if drawn {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(drawn);
        // Another size: the drawn one stands in meanwhile.
        r.begin_frame(&ctx);
        assert!(matches!(r.page(7, 1, size, 900.0, false), Page::Drawn(_)));
        assert!(matches!(r.page(7, 2, size, 200.0, true), Page::Pending));
        r.end_frame();
    }
}
