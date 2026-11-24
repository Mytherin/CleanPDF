//! Writing an edited document: the original file, with its pages removed,
//! reordered and turned as the document's [`PageRef`]s say (with lopdf).
//!
//! The pages kept are put straight under the root of the page tree, in
//! their new order, each first given the attributes it inherited from the
//! tree (resources, boxes, rotation) so it looks the same. References to
//! pages removed (from bookmarks, links, form fields) become `null`, and
//! whatever only they used is dropped, so their content leaves the file.
//!
//! hayro, which draws the pages, reads damaged files more forgivingly than
//! lopdf: what lopdf misses or reads differently is taken as hayro reads it
//! (see [`repair`]), so what's saved is what was shown.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use hayro::hayro_syntax::object::{self as h, MaybeRef, ObjectIdentifier};
use lopdf::{Dictionary, Object, ObjectId};

use crate::document::PageRef;

/// What a page inherits from the page tree when it doesn't say.
const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// The PDF `original` (with `source_count` pages) made of `pages`.
pub fn write(original: &Arc<Vec<u8>>, pages: &[PageRef], source_count: usize) -> Result<Vec<u8>> {
    std::panic::catch_unwind(|| write_inner(original, pages, source_count)).unwrap_or_else(|_| Err(anyhow!("the file couldn't be read to save it")))
}

fn write_inner(original: &Arc<Vec<u8>>, pages: &[PageRef], source_count: usize) -> Result<Vec<u8>> {
    if pages.is_empty() {
        bail!("a PDF needs at least one page");
    }
    let mut doc = lopdf::Document::load_mem(original).map_err(|e| anyhow!("the file couldn't be read to save it ({e})"))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        bail!("changes to password-protected PDFs can't be saved");
    }
    repair(&mut doc, original)?;
    let ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    if ids.len() != source_count {
        bail!("the file's pages couldn't be read to save it (found {} of {source_count})", ids.len());
    }
    let catalog = doc.trailer.get(b"Root").and_then(Object::as_reference).context("the file has no catalog")?;
    let root = doc.get_dictionary(catalog).and_then(|c| c.get(b"Pages")).and_then(Object::as_reference).context("the file has no page tree")?;

    for &id in &ids {
        inherit(&mut doc, id)?;
    }

    let kept: Vec<ObjectId> = pages.iter().map(|p| ids[p.source]).collect();
    for (page, &id) in pages.iter().zip(&kept) {
        let dict = doc.get_dictionary_mut(id)?;
        let own = dict.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
        let rotate = (own + page.rotation as i64).rem_euclid(360);
        if rotate == 0 {
            dict.remove(b"Rotate");
        } else {
            dict.set("Rotate", rotate);
        }
        dict.set("Parent", root);
    }
    let root_dict = doc.get_dictionary_mut(root)?;
    root_dict.set("Kids", kept.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>());
    root_dict.set("Count", kept.len() as i64);

    let kept: HashSet<ObjectId> = kept.into_iter().collect();
    let removed: HashSet<ObjectId> = ids.into_iter().filter(|id| !kept.contains(id)).collect();
    if !removed.is_empty() {
        doc.traverse_objects(|o| {
            if matches!(o, Object::Reference(r) if removed.contains(r)) {
                *o = Object::Null;
            }
        });
    }
    doc.prune_objects();

    let mut out = Vec::new();
    // Object and cross-reference streams (smaller) when the file may have
    // them: PDF 1.5 and on.
    let modern = doc.version.as_str() >= "1.5";
    let saved = if modern { doc.save_modern(&mut out) } else { doc.save_to(&mut out) };
    saved.map_err(|e| anyhow!("the PDF couldn't be written ({e})"))?;
    Ok(out)
}

/// Gives page `id` the attributes it inherits from its ancestors in the
/// page tree.
fn inherit(doc: &mut lopdf::Document, id: ObjectId) -> Result<()> {
    let page = doc.get_dictionary(id)?;
    let mut missing: Vec<&[u8]> = INHERITED.iter().copied().filter(|k| !page.has(k)).collect();
    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    while let Some(node_id) = parent {
        if missing.is_empty() || !seen.insert(node_id) {
            break;
        }
        let Ok(node) = doc.get_dictionary(node_id) else { break };
        missing.retain(|k| match node.get(k) {
            Ok(v) => {
                found.push((k.to_vec(), v.clone()));
                false
            }
            Err(_) => true,
        });
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
    }
    let page = doc.get_dictionary_mut(id)?;
    for (k, v) in found {
        page.set(k, v);
    }
    Ok(())
}

/// Makes lopdf's reading of the file hayro's where they differ: streams
/// whose bytes lopdf read otherwise (a wrong `/Length`, say) and objects
/// referred to that lopdf couldn't read at all are taken from hayro.
fn repair(doc: &mut lopdf::Document, original: &Arc<Vec<u8>>) -> Result<()> {
    let pdf = hayro::hayro_syntax::Pdf::new(original.clone()).map_err(|_| anyhow!("the file couldn't be read to save it"))?;
    let xref = pdf.xref();
    let hayro_id = |id: ObjectId| ObjectIdentifier::new(id.0, id.1 as i32);

    let ids: Vec<ObjectId> = doc.objects.iter().filter(|(_, o)| matches!(o, Object::Stream(_))).map(|(id, _)| *id).collect();
    for id in ids {
        let Some(theirs) = xref.get::<h::Stream<'_>>(hayro_id(id)) else { continue };
        let same = matches!(doc.objects.get(&id), Some(Object::Stream(ours)) if ours.content.as_slice() == theirs.raw_data().as_ref());
        if !same {
            doc.objects.insert(id, convert(&h::Object::Stream(theirs)));
        }
    }

    let mut tried = HashSet::new();
    loop {
        let mut missing = HashSet::new();
        for v in doc.trailer.iter().map(|(_, v)| v).chain(doc.objects.values()) {
            refs(v, &mut |id| {
                if !doc.objects.contains_key(&id) && !tried.contains(&id) {
                    missing.insert(id);
                }
            });
        }
        if missing.is_empty() {
            return Ok(());
        }
        for id in missing {
            tried.insert(id);
            if let Some(o) = xref.get::<h::Object<'_>>(hayro_id(id)) {
                doc.objects.insert(id, convert(&o));
            }
        }
    }
}

/// Calls `f` with each reference in `o`.
fn refs(o: &Object, f: &mut impl FnMut(ObjectId)) {
    match o {
        Object::Reference(id) => f(*id),
        Object::Array(a) => a.iter().for_each(|o| refs(o, f)),
        Object::Dictionary(d) => d.iter().for_each(|(_, o)| refs(o, f)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, o)| refs(o, f)),
        _ => {}
    }
}

/// A hayro object as lopdf's (references stay references).
fn convert(o: &h::Object<'_>) -> Object {
    match o {
        h::Object::Null(_) => Object::Null,
        h::Object::Boolean(b) => Object::Boolean(*b),
        h::Object::Number(n) => {
            let v = n.as_f64();
            if v.fract() == 0.0 && v.abs() < 9e15 {
                Object::Integer(v as i64)
            } else {
                Object::Real(v as f32)
            }
        }
        h::Object::String(s) => Object::String(s.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        h::Object::Name(n) => Object::Name(n.as_ref().to_vec()),
        h::Object::Dict(d) => Object::Dictionary(convert_dict(d)),
        h::Object::Array(a) => Object::Array(a.raw_iter().map(|m| convert_maybe(&m)).collect()),
        // (Its `/Length` is set to the data's.)
        h::Object::Stream(s) => Object::Stream(lopdf::Stream::new(convert_dict(s.dict()), s.raw_data().into_owned())),
    }
}

fn convert_maybe(m: &MaybeRef<h::Object<'_>>) -> Object {
    match m {
        MaybeRef::Ref(r) => Object::Reference((r.obj_number, r.gen_number as u16)),
        MaybeRef::NotRef(o) => convert(o),
    }
}

fn convert_dict(d: &h::Dict<'_>) -> Dictionary {
    let mut out = Dictionary::new();
    for (k, v) in d.entries() {
        out.set(k.as_ref().to_vec(), convert_maybe(&v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::tests::test_pdf;
    use crate::document::Document;
    use lopdf::dictionary;

    fn page(source: usize, rotation: u16) -> PageRef {
        PageRef { source, rotation }
    }

    /// The pages of a saved PDF, as hayro reads them: each one's size, and
    /// whether its middle is black (the test pages' square).
    fn read_back(bytes: Vec<u8>) -> Vec<(u32, u32)> {
        let d = Document::from_bytes("out.pdf".into(), bytes).expect("the saved file opens");
        (0..d.len()).map(|i| (d.page_size(i).x as u32, d.page_size(i).y as u32)).collect()
    }

    #[test]
    fn removes_reorders_and_turns_pages() {
        // Pages 0 and 1 inherit their size from a middle node of the tree.
        let pdf = test_pdf(&[(100, 200), (100, 200), (300, 100), (50, 60)]);
        let out = write(&Arc::new(pdf.clone()), &[page(3, 0), page(1, 0), page(2, 90)], 4).unwrap();
        assert_eq!(read_back(out.clone()), [(50, 60), (100, 200), (100, 300)]);

        let doc = lopdf::Document::load_mem(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
        // Removed pages' content streams are gone: 1 catalog, 1 page tree,
        // and 3 pages with their contents.
        assert_eq!(doc.objects.len(), 2 + 3 * 2);
        let rotate = |n: u32| doc.get_dictionary(doc.get_pages()[&n]).unwrap().get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
        assert_eq!((rotate(1), rotate(2), rotate(3)), (0, 0, 90));
    }

    #[test]
    fn turning_adds_to_a_pages_own_rotation_and_links_to_removed_pages_go() {
        let pdf = test_pdf(&[(100, 200), (100, 200), (300, 100)]);
        let mut doc = lopdf::Document::load_mem(&pdf).unwrap();
        let ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
        doc.get_dictionary_mut(ids[2]).unwrap().set("Rotate", 270);
        // A bookmark to the page that will be removed.
        let outline_item = doc.add_object(dictionary! {
            "Title" => Object::string_literal("Second"),
            "Dest" => vec![Object::Reference(ids[1]), "Fit".into()],
        });
        let outlines = doc.add_object(dictionary! { "Type" => "Outlines", "First" => outline_item, "Last" => outline_item, "Count" => 1 });
        let catalog = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(catalog).unwrap().set("Outlines", outlines);
        let mut pdf = Vec::new();
        doc.save_to(&mut pdf).unwrap();

        let out = write(&Arc::new(pdf.clone()), &[page(0, 0), page(2, 180)], 3).unwrap();
        assert_eq!(read_back(out.clone()), [(100, 200), (100, 300)]);
        let doc = lopdf::Document::load_mem(&out).unwrap();
        let second = doc.get_pages()[&2];
        assert_eq!(doc.get_dictionary(second).unwrap().get(b"Rotate").unwrap().as_i64().unwrap(), 90);
        let item = doc.objects.values().find_map(|o| o.as_dict().ok().filter(|d| d.has(b"Title"))).unwrap();
        assert_eq!(item.get(b"Dest").unwrap().as_array().unwrap()[0], Object::Null);
    }

    #[test]
    fn a_stream_lopdf_misreads_is_saved_as_shown() {
        // A content stream whose /Length is 2 short, as in a real scanned
        // file: hayro reads it to `endstream`, lopdf drops it.
        let pdf = test_pdf(&[(100, 100), (100, 100)]);
        let text = String::from_utf8(pdf).unwrap();
        let content = "0 g 30 30 40 40 re f";
        let damaged = text.replacen(&format!("/Length {}", content.len()), &format!("/Length {}", content.len() - 2), 1);
        let damaged = Arc::new(damaged.into_bytes());
        let original = hayro::hayro_syntax::Pdf::new(damaged.clone()).unwrap();
        let before = crate::render::render(&original, 0, 100).unwrap();
        assert_eq!(before.pixels[50 * 100 + 50], eframe::egui::Color32::BLACK, "hayro shows the square");

        let out = Arc::new(write(&damaged, &[page(0, 0)], 2).unwrap());
        let saved = hayro::hayro_syntax::Pdf::new(out).unwrap();
        assert_eq!(crate::render::render(&saved, 0, 100).unwrap().pixels, before.pixels);
    }

    #[test]
    fn saving_writes_the_file_and_clears_edited() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pdf");
        std::fs::write(&path, test_pdf(&[(100, 200); 3])).unwrap();
        let mut d = Document::open(&path).unwrap();
        d.delete(&[0]);
        assert!(d.edited());
        d.save_as(&path).unwrap();
        assert!(!d.edited());
        assert_eq!(Document::open(&path).unwrap().len(), 2);
        // Save As elsewhere; the open document now is that file.
        let other = dir.path().join("b.pdf");
        d.delete(&[0]);
        d.save_as(&other).unwrap();
        assert_eq!(d.path, other);
        assert_eq!(Document::open(&other).unwrap().len(), 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2, "no temporary file is left");
    }
}

/// Edits and saves every PDF in the folder `CLEANPDF_SAMPLES` names:
/// `CLEANPDF_SAMPLES=dir cargo test samples -- --ignored --nocapture`.
#[cfg(test)]
mod samples {
    use crate::document::{Document, PageRef};

    #[test]
    #[ignore]
    fn edit_and_save_samples() {
        let dir = std::path::PathBuf::from(std::env::var("CLEANPDF_SAMPLES").expect("CLEANPDF_SAMPLES"));
        let out = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let mut doc = match Document::open(&path) {
                Ok(d) => d,
                Err(e) => {
                    println!("{name}: doesn't open: {e:#}");
                    continue;
                }
            };
            let n = doc.len();
            if n > 1 {
                doc.delete(&[0]);
            }
            doc.rotate(&[0], 90);
            if doc.len() > 1 {
                doc.move_pages(&[doc.len() - 1], 0);
            }
            let expected: Vec<PageRef> = doc.pages.clone();
            let sizes: Vec<_> = (0..doc.len()).map(|i| doc.page_size(i)).collect();
            let target = std::env::var("CLEANPDF_DUMP").map(std::path::PathBuf::from).unwrap_or(out.path().to_owned()).join(&name);
            let before = std::fs::metadata(&path).unwrap().len();
            match doc.save_as(&target) {
                Ok(()) => {
                    let again = Document::open(&target).expect("the saved file opens");
                    let got: Vec<_> = (0..again.len()).map(|i| again.page_size(i)).collect();
                    let ok = got.len() == expected.len() && got.iter().zip(&sizes).all(|(a, b)| (*a - *b).length() < 1.0);
                    println!("{name}: {n} pages -> {} ({before} -> {} bytes) sizes match: {ok}", again.len(), std::fs::metadata(&target).unwrap().len());
                    assert!(ok);
                    let (a, b) = (hayro::hayro_syntax::Pdf::new(doc.bytes.clone()).unwrap(), hayro::hayro_syntax::Pdf::new(again.bytes.clone()).unwrap());
                    for (i, p) in expected.iter().enumerate().filter(|(_, p)| p.rotation == 0) {
                        let (x, y) = (crate::render::render(&a, p.source, 300).unwrap(), crate::render::render(&b, i, 300).unwrap());
                        assert_eq!(x.size, y.size);
                        let diff = x.pixels.iter().zip(&y.pixels).filter(|(c, d)| c != d).count();
                        println!("  page {i} (was {}): {diff} of {} pixels differ", p.source + 1, x.pixels.len());
                        if diff > 0 { if let Ok(d) = std::env::var("CLEANPDF_DUMP") { for (tag, img) in [("a", &x), ("b", &y)] { let mut f = std::fs::File::create(format!("{d}/{tag}.png")).unwrap(); let mut e = png::Encoder::new(&mut f, img.size[0] as u32, img.size[1] as u32); e.set_color(png::ColorType::Rgba); e.write_header().unwrap().write_image_data(img.as_raw()).unwrap(); } } }
                        assert_eq!(diff, 0);
                    }
                }
                Err(e) => println!("{name}: {n} pages, save failed: {e:#}"),
            }
        }
    }
}
