//! An open PDF and the edits made to it.
//!
//! The file's bytes are kept as they were read: the document is a list of
//! [`PageRef`]s into them (which of the file's pages, in which order, turned
//! how far). Editing changes the list only, so undo is a matter of keeping
//! old lists, and pages already drawn stay drawn. Saving writes the list
//! out as a new PDF (see [`crate::edit`]).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use eframe::egui::Vec2;

/// A page of the document: page `source` of the file, turned clockwise by
/// `rotation` degrees (0, 90, 180 or 270) more than the file has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageRef {
    pub source: usize,
    pub rotation: u16,
}

pub struct Document {
    /// Where it was read from, and is saved to.
    pub path: PathBuf,
    /// The file's bytes, as read.
    pub bytes: Arc<Vec<u8>>,
    /// Tells documents apart for the page drawer (one opened after another
    /// may be the same file, changed).
    pub id: u64,
    /// The size, in points, of each of the file's pages as it's drawn (its
    /// own `/Rotate` applied).
    pub sizes: Vec<Vec2>,
    pub pages: Vec<PageRef>,
    /// The pages as last saved (or opened): whether there are changes.
    saved: Vec<PageRef>,
    undo: Vec<Vec<PageRef>>,
    redo: Vec<Vec<PageRef>>,
}

impl Document {
    pub fn open(path: &Path) -> Result<Document> {
        let bytes = std::fs::read(path).with_context(|| format!("couldn't read {}", path.display()))?;
        Document::from_bytes(path.to_owned(), bytes)
    }

    pub fn from_bytes(path: PathBuf, bytes: Vec<u8>) -> Result<Document> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let bytes = Arc::new(bytes);
        // (A reader that panics on a damaged file makes it unreadable.)
        let sizes = std::panic::catch_unwind(|| page_sizes(&bytes)).unwrap_or_else(|_| bail!("the file is damaged"))?;
        let pages: Vec<PageRef> = (0..sizes.len()).map(|source| PageRef { source, rotation: 0 }).collect();
        Ok(Document {
            path,
            bytes,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            sizes,
            saved: pages.clone(),
            pages,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// The file's name.
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(|| "Untitled.pdf".to_owned(), |n| n.to_string_lossy().into_owned())
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Page `i`'s size in points, as shown (turned).
    pub fn page_size(&self, i: usize) -> Vec2 {
        let page = self.pages[i];
        let s = self.sizes[page.source];
        if page.rotation % 180 == 90 {
            Vec2::new(s.y, s.x)
        } else {
            s
        }
    }

    /// Whether there are changes since it was opened or saved.
    pub fn edited(&self) -> bool {
        self.pages != self.saved
    }

    fn change(&mut self, pages: Vec<PageRef>) {
        if pages != self.pages {
            self.undo.push(std::mem::replace(&mut self.pages, pages));
            self.redo.clear();
        }
    }

    /// Removes the pages at `indices`. A document keeps at least one page:
    /// removing all of them does nothing (and gives false).
    pub fn delete(&mut self, indices: &[usize]) -> bool {
        let pages: Vec<PageRef> = self.pages.iter().enumerate().filter(|(i, _)| !indices.contains(i)).map(|(_, p)| *p).collect();
        if pages.is_empty() || pages.len() == self.pages.len() {
            return false;
        }
        self.change(pages);
        true
    }

    /// Turns the pages at `indices` by `degrees` (a multiple of 90; negative
    /// is counterclockwise).
    pub fn rotate(&mut self, indices: &[usize], degrees: i32) {
        let mut pages = self.pages.clone();
        for &i in indices {
            if let Some(p) = pages.get_mut(i) {
                p.rotation = (p.rotation as i32 + degrees).rem_euclid(360) as u16;
            }
        }
        self.change(pages);
    }

    /// Moves the pages at `indices` (kept in their order) to before the page
    /// now at `to` (`len()`: to the end). Gives where they now start.
    pub fn move_pages(&mut self, indices: &[usize], to: usize) -> usize {
        let mut sorted: Vec<usize> = indices.iter().copied().filter(|&i| i < self.pages.len()).collect();
        sorted.sort_unstable();
        sorted.dedup();
        let moving: Vec<PageRef> = sorted.iter().map(|&i| self.pages[i]).collect();
        let start = to - sorted.iter().filter(|&&i| i < to).count();
        let mut pages: Vec<PageRef> = self.pages.iter().enumerate().filter(|(i, _)| !sorted.contains(i)).map(|(_, p)| *p).collect();
        pages.splice(start..start, moving);
        self.change(pages);
        start
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(pages) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.pages, pages));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(pages) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.pages, pages));
        true
    }

    /// Writes the document to `path` (its own, or a new one: Save As), as
    /// a new file that then replaces any there, so a failed save leaves the
    /// old file as it was.
    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        let unchanged = self.pages == (0..self.sizes.len()).map(|source| PageRef { source, rotation: 0 }).collect::<Vec<_>>();
        let out = if unchanged {
            // Nothing to rewrite: the file as it was.
            self.bytes.to_vec()
        } else {
            crate::edit::write(&self.bytes, &self.pages, self.sizes.len())?
        };
        let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let name = path.file_name().context("no file name")?.to_string_lossy();
        let tmp = dir.join(format!(".{name}.saving"));
        std::fs::write(&tmp, &out).with_context(|| format!("couldn't write to {}", dir.display()))?;
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e).with_context(|| format!("couldn't replace {}", path.display()));
        }
        self.path = path.to_owned();
        self.saved = self.pages.clone();
        Ok(())
    }
}

/// Whether `bytes` are a PDF's (its header may come after some junk, as
/// readers allow).
pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(1024)].windows(5).any(|w| w == b"%PDF-")
}

/// Each page's size in points, as hayro draws it.
fn page_sizes(bytes: &Arc<Vec<u8>>) -> Result<Vec<Vec2>> {
    if !is_pdf(bytes) {
        bail!("it isn't a PDF");
    }
    let pdf = match hayro::hayro_syntax::Pdf::new(bytes.clone()) {
        Ok(pdf) => pdf,
        Err(hayro::hayro_syntax::LoadPdfError::Decryption(_)) => bail!("it's protected with a password"),
        Err(_) => bail!("the file is damaged"),
    };
    let sizes: Vec<Vec2> = pdf
        .pages()
        .iter()
        .map(|p| {
            let (w, h) = p.render_dimensions();
            Vec2::new(w, h)
        })
        .collect();
    if sizes.is_empty() {
        bail!("it has no pages");
    }
    // (A page with no size is shown as a Letter-sized blank.)
    Ok(sizes.into_iter().map(|s| if s.x > 0.0 && s.y > 0.0 { s } else { Vec2::new(612.0, 792.0) }).collect())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A PDF with a page for each size (in points), each with a black
    /// square in its middle, and its pages in a two-level page tree with an
    /// inherited MediaBox.
    pub fn test_pdf(sizes: &[(u32, u32)]) -> Vec<u8> {
        let n = sizes.len();
        // 1: catalog, 2: root Pages, 3: a middle Pages node holding the
        // first half; then a page and its contents for each.
        let page_id = |i: usize| 4 + 2 * i;
        let half = n / 2;
        let mut root_kids = vec!["3 0 R".to_owned()];
        root_kids.extend((half..n).map(|i| format!("{} 0 R", page_id(i))));
        let mid_kids: Vec<String> = (0..half).map(|i| format!("{} 0 R", page_id(i))).collect();
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            format!("<< /Type /Pages /Kids [{}] /Count {n} >>", root_kids.join(" ")),
            format!("<< /Type /Pages /Parent 2 0 R /Kids [{}] /Count {half} /MediaBox [0 0 {} {}] >>", mid_kids.join(" "), sizes[0].0, sizes[0].1),
        ];
        for (i, (w, h)) in sizes.iter().enumerate() {
            let content = format!("0 g {} {} 40 40 re f", w / 2 - 20, h / 2 - 20);
            let (parent, media) = if i < half { (3, String::new()) } else { (2, format!("/MediaBox [0 0 {w} {h}] ")) };
            assert!(i >= half || (*w, *h) == sizes[0], "the first half of the pages inherit the first's size");
            objects.push(format!("<< /Type /Page /Parent {parent} 0 R {media}/Contents {} 0 R >>", page_id(i) + 1));
            objects.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for o in offsets {
            out.extend(format!("{o:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
        out
    }

    fn doc(sizes: &[(u32, u32)]) -> Document {
        Document::from_bytes("t.pdf".into(), test_pdf(sizes)).unwrap()
    }

    fn sources(d: &Document) -> Vec<usize> {
        d.pages.iter().map(|p| p.source).collect()
    }

    #[test]
    fn opens_and_reads_page_sizes() {
        let d = doc(&[(100, 200), (100, 200), (300, 100), (50, 60)]);
        assert_eq!(d.len(), 4);
        assert_eq!(d.page_size(0), Vec2::new(100.0, 200.0));
        assert_eq!(d.page_size(2), Vec2::new(300.0, 100.0));
        assert!(!d.edited());
        assert!(Document::from_bytes("x.pdf".into(), b"not a pdf".to_vec()).is_err());
        assert!(Document::from_bytes("x.pdf".into(), b"%PDF-1.4 cut short".to_vec()).is_err());
    }

    #[test]
    fn deleting_rotating_and_moving_pages_undo_and_redo() {
        let mut d = doc(&[(100, 200); 5]);
        assert!(d.delete(&[1, 3]));
        assert_eq!(sources(&d), [0, 2, 4]);
        assert!(d.edited());
        assert!(!d.delete(&[0, 1, 2]), "the last page stays");
        assert_eq!(d.len(), 3);
        d.rotate(&[0], 90);
        assert_eq!(d.page_size(0), Vec2::new(200.0, 100.0));
        d.rotate(&[0], -180);
        assert_eq!(d.pages[0].rotation, 270);
        assert_eq!(d.move_pages(&[0], 3), 2);
        assert_eq!(sources(&d), [2, 4, 0]);
        assert_eq!(d.move_pages(&[1, 2], 0), 0);
        assert_eq!(sources(&d), [4, 0, 2]);
        for _ in 0..5 {
            assert!(d.undo());
        }
        assert!(!d.undo());
        assert_eq!(sources(&d), [0, 1, 2, 3, 4]);
        assert!(!d.edited());
        assert!(d.redo());
        assert_eq!(sources(&d), [0, 2, 4]);
        // A new change drops what could be redone.
        d.rotate(&[1], 90);
        assert!(!d.can_redo());
    }
}
