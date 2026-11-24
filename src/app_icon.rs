//! The app icon: a page with a folded corner, lines of text and a red band,
//! drawn in code so it's sharp at every size (as CleanNotes' is). `cargo
//! test app_icon::tests::write_exe_icon -- --ignored` writes
//! `assets/CleanPDF.ico` for the executable (see `build.rs`).

use eframe::egui::IconData;

/// RGBA pixels of the icon at `size`×`size`.
pub fn render(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    const SS: u32 = 4;
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (x as f32 + (sx as f32 + 0.5) / SS as f32) / s;
                    let fy = (y as f32 + (sy as f32 + 0.5) / SS as f32) / s;
                    if let Some(c) = color_at(fx, fy, s) {
                        for i in 0..3 {
                            acc[i] += c[i] as f32;
                        }
                        acc[3] += 255.0;
                    }
                }
            }
            let i = ((y * size + x) * 4) as usize;
            let a = acc[3] / (SS * SS) as f32;
            if a > 0.0 {
                for c in 0..3 {
                    px[i + c] = (acc[c] / (acc[3] / 255.0)).round().min(255.0) as u8;
                }
            }
            px[i + 3] = a.round() as u8;
        }
    }
    px
}

/// The color at (x, y), both 0..1 across the icon; `None` outside it.
fn color_at(x: f32, y: f32, s: f32) -> Option<[u8; 3]> {
    let (l, r, t, b) = (0.17, 0.83, 0.06, 0.94);
    let fold = 0.2;
    let edge = 1.0 / s.max(16.0) * 1.2;
    if x < l || x > r || y < t || y > b {
        return None;
    }
    // The folded corner, top right: cut away, with the flap below it.
    let into_corner = (x - (r - fold)) + (t + fold - y);
    if into_corner > fold {
        return None;
    }
    if x > r - fold && y < t + fold {
        if into_corner > fold - edge * 1.5 {
            return Some([150, 150, 156]);
        }
        if x > r - fold + edge && y < t + fold - edge {
            return Some([226, 226, 230]);
        }
    }
    // The page's outline.
    if x < l + edge || x > r - edge || y > b - edge || y < t + edge || (x > r - fold - edge * 0.5 && x < r - fold + edge * 0.5 && y < t + fold) || (y > t + fold - edge * 0.5 && y < t + fold + edge * 0.5 && x > r - fold) {
        return Some([170, 170, 176]);
    }
    // The red band, low on the page.
    if y > 0.6 && y < 0.8 && x > l - 0.07 && x < r - 0.12 {
        return Some([228, 54, 44]);
    }
    // Lines of text.
    if s >= 32.0 {
        for (i, top) in [0.3, 0.38, 0.46].iter().enumerate() {
            let end = if i == 0 { r - fold - 0.05 } else { r - 0.1 };
            if y > *top && y < top + 0.035 && x > l + 0.1 && x < end {
                return Some([196, 196, 202]);
            }
        }
    }
    Some([252, 252, 252])
}

pub fn icon_data(size: u32) -> IconData {
    IconData { rgba: render(size), width: size, height: size }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_with_a_page_and_red_band() {
        let px = render(64);
        let at = |x: u32, y: u32| &px[((y * 64 + x) * 4) as usize..][..4];
        assert_eq!(at(0, 0)[3], 0);
        assert_eq!(at(62, 2)[3], 0, "the folded corner is cut away");
        assert_eq!(at(32, 16), [252, 252, 252, 255]);
        let red = at(32, 45);
        assert!(red[0] > 200 && red[1] < 80, "the red band");
    }

    /// Writes assets/CleanPDF.ico (PNG-compressed images).
    #[test]
    #[ignore]
    fn write_exe_icon() {
        let sizes = [16u32, 24, 32, 48, 64, 128, 256];
        let images: Vec<Vec<u8>> = sizes
            .iter()
            .map(|&s| {
                let mut out = Vec::new();
                let mut enc = png::Encoder::new(&mut out, s, s);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                enc.write_header().unwrap().write_image_data(&render(s)).unwrap();
                out
            })
            .collect();
        let mut ico = Vec::new();
        ico.extend_from_slice(&[0, 0, 1, 0]);
        ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
        let mut offset = 6 + 16 * sizes.len() as u32;
        for (s, img) in sizes.iter().zip(&images) {
            let b = if *s >= 256 { 0 } else { *s as u8 };
            ico.extend_from_slice(&[b, b, 0, 0]);
            ico.extend_from_slice(&1u16.to_le_bytes());
            ico.extend_from_slice(&32u16.to_le_bytes());
            ico.extend_from_slice(&(img.len() as u32).to_le_bytes());
            ico.extend_from_slice(&offset.to_le_bytes());
            offset += img.len() as u32;
        }
        for img in &images {
            ico.extend_from_slice(img);
        }
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/CleanPDF.ico"), ico).unwrap();
    }
}
