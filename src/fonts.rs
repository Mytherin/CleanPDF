//! Finds the platform's font files: the UI font (Segoe UI on Windows, San
//! Francisco with Helvetica Neue for the other weights on macOS, DejaVu Sans
//! on Linux) in regular, bold, italic and bold italic, as CleanNotes does.
//!
//! Callers fall back to egui's bundled fonts when nothing's found.

use std::sync::OnceLock;

/// A font file's bytes and which face in it to use (non-zero only for
/// collections such as `.ttc`).
#[derive(Clone)]
pub struct FontFile {
    pub bytes: Vec<u8>,
    pub index: u32,
}

/// Which face of a family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    pub bold: bool,
    pub italic: bool,
}

const REGULAR: &[&str] = &[
    // Windows
    "C:\\Windows\\Fonts\\segoeui.ttf",
    // macOS: SF is a variable font whose default instance is Regular.
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/HelveticaNeue.ttc",
    // Linux (Debian/Ubuntu, Fedora, Arch)
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

const BOLD: &[&str] = &[
    // Windows: Semibold, else Bold.
    "C:\\Windows\\Fonts\\seguisb.ttf",
    "C:\\Windows\\Fonts\\segoeuib.ttf",
    // macOS: SF ships no static bold, and egui can't pick a variable font's
    // weight, so use Helvetica Neue's Bold face (picked out of the collection).
    "/System/Library/Fonts/HelveticaNeue.ttc",
    // Linux
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
];

const ITALIC: &[&str] = &[
    "C:\\Windows\\Fonts\\segoeuii.ttf",
    "/System/Library/Fonts/HelveticaNeue.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Oblique.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Oblique.ttf",
];

const BOLD_ITALIC: &[&str] = &[
    "C:\\Windows\\Fonts\\seguisbi.ttf",
    "C:\\Windows\\Fonts\\segoeuiz.ttf",
    "/System/Library/Fonts/HelveticaNeue.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-BoldOblique.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-BoldOblique.ttf",
];

/// The UI font in the given face.
pub fn ui(face: Face) -> Option<&'static FontFile> {
    static F: [OnceLock<Option<FontFile>>; 4] = [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let (i, paths) = match (face.bold, face.italic) {
        (false, false) => (0, REGULAR),
        (true, false) => (1, BOLD),
        (false, true) => (2, ITALIC),
        (true, true) => (3, BOLD_ITALIC),
    };
    F[i].get_or_init(|| find(paths, face)).as_ref()
}

fn find(paths: &[&str], face: Face) -> Option<FontFile> {
    paths.iter().find_map(|p| {
        let bytes = std::fs::read(p).ok()?;
        let index = pick_face(&bytes, face)?;
        Some(FontFile { bytes, index })
    })
}

/// The face to use from a font file. A single-face file is taken as is (the
/// path lists say what it is); in a collection, the first normal-width face
/// of the wanted weight and slant.
fn pick_face(bytes: &[u8], want: Face) -> Option<u32> {
    let Some(count) = ttf_parser::fonts_in_collection(bytes) else {
        return ttf_parser::Face::parse(bytes, 0).is_ok().then_some(0);
    };
    (0..count).find(|&i| {
        ttf_parser::Face::parse(bytes, i).is_ok_and(|f| {
            let weight = f.weight().to_number();
            (f.is_italic() || f.is_oblique()) == want.italic
                && f.width() == ttf_parser::Width::Normal
                && if want.bold { (600..=700).contains(&weight) } else { weight == 400 }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_is_not_a_font() {
        let face = Face { bold: false, italic: false };
        assert_eq!(pick_face(b"not a font", face), None);
        assert!(find(&["/no/such/font.ttf"], face).is_none());
    }

    #[test]
    fn every_platform_finds_its_fonts_when_present() {
        // Whatever is found must parse as the face we said to use.
        let faces = [(false, false), (true, false), (false, true), (true, true)];
        let found = faces.iter().map(|&(bold, italic)| ui(Face { bold, italic }));
        for f in found.flatten() {
            assert!(ttf_parser::Face::parse(&f.bytes, f.index).is_ok());
        }
    }
}
