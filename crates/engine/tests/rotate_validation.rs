//! Visual and signature checks for saved page rotation.
//!
//! Pictures are written outside the repository. The comparison is a clockwise bitmap turn of the
//! unrotated render against a fresh render of the `/Rotate` page. A centroid tolerance absorbs
//! antialiasing; it is not a substitute for the pictures.

use std::path::PathBuf;
use std::sync::Arc;

use pdfcraft_cos::{Dict, Document, Object, PdfString, SaveOptions, write_full, write_incremental};
use pdfcraft_engine::{AddedText, Capability, Edit, EditError, MarkFile, Session};

const SCALE: f32 = 2.0;
/// Scale 2. A fresh vector raster and a turn of an already antialiased bitmap disagree on edge
/// pixels. Six pixels is 3 pt, far inside the 28 pt markers, so a wrong quarter-turn fails.
const CENTROID_PX: f64 = 6.0;
/// Coverage can move when edges are re-rasterized. A missing mark falls to zero and fails.
const COUNT_LO: f64 = 0.65;
const COUNT_HI: f64 = 1.40;
/// Share of pixels whose channels differ by more than 40 after the bitmap turn. Edge
/// antialiasing accounts for a small share; a wrong direction moves whole regions.
const PIXEL_FRACTION: f64 = 0.05;

struct Shot {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

struct Region {
    name: &'static str,
    hit: fn(u8, u8, u8) -> bool,
}

/// `SWIFTPDF_VALIDATE_DIR` when set, otherwise a folder in the system temp directory.
fn out_dir() -> PathBuf {
    let dir = std::env::var_os("SWIFTPDF_VALIDATE_DIR").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("swiftpdf-rotate-validate"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn png(w: u32, h: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let px = paint(x, y);
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&px);
        }
    }
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    bytes
}

fn save_png(path: &std::path::Path, shot: &Shot) {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, shot.w, shot.h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&shot.rgba).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn marked_a4() -> Vec<u8> {
    let mut doc = Document::new_empty();
    let pages = doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let body = b"1 0 0 rg\n16 16 28 28 re f\n0 1 0 rg\n551 16 28 28 re f\n0 0 1 rg\n16 798 28 28 re f\n1 1 0 rg\n551 798 28 28 re f\n0.55 0 0.55 rg\n70 360 160 90 re f\n1 0.45 0 rg\n430 560 36 160 re f\n";
    let contents = doc.add(Object::Stream(pdfcraft_cos::Stream::from_raw(Dict::new(), body.to_vec())));
    let mut first = Dict::new();
    first.set(b"Type".to_vec(), Object::name("Page"));
    first.set(b"Parent".to_vec(), Object::Ref(pages));
    first.set(b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), 595.into(), 842.into()]));
    first.set(b"Contents".to_vec(), Object::Ref(contents));
    let a = doc.add(first);
    let mut second = Dict::new();
    second.set(b"Type".to_vec(), Object::name("Page"));
    second.set(b"Parent".to_vec(), Object::Ref(pages));
    second.set(b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), 595.into(), 842.into()]));
    let b = doc.add(second);
    doc.update_dict(pages, |d| {
        d.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(a), Object::Ref(b)]));
        d.set(b"Count".to_vec(), Object::Int(2));
    })
    .unwrap();
    write_full(&doc, &SaveOptions::default()).unwrap()
}

fn blank() -> Vec<u8> {
    let mut doc = Document::new_empty();
    let pages = doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let mut page = Dict::new();
    page.set(b"Type".to_vec(), Object::name("Page"));
    page.set(b"Parent".to_vec(), Object::Ref(pages));
    page.set(b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), 200.into(), 300.into()]));
    let pref = doc.add(page);
    doc.update_dict(pages, |d| {
        d.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(pref)]));
        d.set(b"Count".to_vec(), Object::Int(1));
    })
    .unwrap();
    write_full(&doc, &SaveOptions::default()).unwrap()
}

fn open_session(grants: &[Capability]) -> Session {
    let mut session = Session::new().with_clock(|| 1_700_000_000).with_view_only(true);
    for grant in grants {
        session.grant(*grant);
    }
    session
}

fn stamp(name: &str, bytes: Vec<u8>, rect: [f64; 4]) -> Edit {
    Edit::AddCustomStamp {
        page: 0,
        rect,
        name: name.into(),
        file: MarkFile { name: format!("{name}.png"), bytes: Arc::new(bytes), page: 0 },
        author: "validation".into(),
        opacity: 1.0,
    }
}

fn place(session: &mut Session, id: pdfcraft_engine::DocId) {
    assert!(pdfcraft_fonts::document_japanese_font().is_some(), "Japanese font is required for this check");
    let add = |session: &mut Session, text: &str, rect: [f64; 4]| {
        session
            .apply(
                id,
                Edit::AddText { page: 0, text: AddedText { rect, text: text.into(), size: 28.0, color: [0.0, 0.0, 0.0], ..AddedText::default() } },
            )
            .unwrap();
    };
    add(session, "東京", [48.0, 640.0, 220.0, 710.0]);
    add(session, "管理センター", [48.0, 560.0, 360.0, 630.0]);
    let general = png(32, 32, |_, _| [255, 0, 255, 255]);
    let date = png(32, 32, |x, y| {
        let dx = x as i32 - 16;
        let dy = y as i32 - 16;
        let d = dx * dx + dy * dy;
        if (100..225).contains(&d) { [0, 255, 255, 255] } else { [0, 0, 0, 0] }
    });
    let chairman = png(32, 32, |x, _| if x < 16 { [0, 128, 128, 255] } else { [170, 20, 20, 255] });
    session.apply(id, stamp("general", general, [80.0, 70.0, 150.0, 140.0])).unwrap();
    session.apply(id, stamp("date", date, [180.0, 60.0, 250.0, 130.0])).unwrap();
    session.apply(id, stamp("chairman", chairman, [300.0, 50.0, 390.0, 140.0])).unwrap();
}

/// Text and three synthetic seals on the marked page. The second page stays empty.
fn compose(dir: &std::path::Path) -> (Vec<u8>, Arc<Vec<u8>>) {
    let original = marked_a4();
    let source = dir.join("source.pdf");
    std::fs::write(&source, &original).unwrap();
    let mut session = open_session(&[Capability::EditText, Capability::Stamp, Capability::PageRotate, Capability::SaveAs]);
    let id = session.open("source.pdf", Some(source.to_string_lossy().into_owned()), Arc::new(original.clone()), None).unwrap();
    place(&mut session, id);
    assert_eq!(std::fs::read(&source).unwrap(), original, "composing must not touch the source file");
    let working = session.get(id).unwrap().bytes.clone();
    std::fs::write(dir.join("composed.pdf"), working.as_ref()).unwrap();
    (original, working)
}

fn render(bytes: &[u8], page: usize) -> Shot {
    let mut renderer = pdfcraft_render::PageRenderer::new(Arc::new(bytes.to_vec()), pdfcraft_render::RenderConfig::default());
    let shot = renderer.render(pdfcraft_render::RenderRequest {
        page,
        kind: pdfcraft_render::RequestKind::Pixels,
        tile: None,
        scale: SCALE,
        tag: 0,
        preview: false,
    });
    assert!(shot.error.is_none(), "page {page}: {:?}", shot.error);
    Shot { w: shot.width, h: shot.height, rgba: shot.rgba.to_vec() }
}

fn regions() -> Vec<Region> {
    vec![
        Region { name: "corner-red", hit: |r, g, b| r > 220 && g < 40 && b < 40 },
        Region { name: "corner-green", hit: |r, g, b| g > 220 && r < 40 && b < 40 },
        Region { name: "corner-blue", hit: |r, g, b| b > 220 && r < 40 && g < 40 },
        Region { name: "corner-yellow", hit: |r, g, b| r > 220 && g > 220 && b < 40 },
        Region { name: "block-purple", hit: |r, g, b| r > 90 && r < 190 && b > 90 && b < 190 && g < 50 && (r as i16 - b as i16).abs() < 40 },
        Region { name: "bar-orange", hit: |r, g, b| r > 220 && g > 70 && g < 170 && b < 40 },
        Region {
            name: "text-black",
            hit: |r, g, b| r < 70 && g < 70 && b < 70 && (r as i16 - g as i16).abs() < 20 && (g as i16 - b as i16).abs() < 20,
        },
        Region { name: "stamp-magenta", hit: |r, g, b| r > 220 && b > 220 && g < 40 },
        Region { name: "stamp-cyan", hit: |r, g, b| g > 220 && b > 220 && r < 40 },
        Region { name: "seal-teal", hit: |r, g, b| r < 40 && g > 80 && g < 180 && b > 80 && b < 180 && (g as i16 - b as i16).abs() < 40 },
        Region { name: "seal-red", hit: |r, g, b| r > 120 && r < 210 && g < 60 && b < 60 },
    ]
}

fn centroid(shot: &Shot, hit: fn(u8, u8, u8) -> bool) -> Option<(f64, f64, u32)> {
    let mut n = 0u64;
    let mut sx = 0u64;
    let mut sy = 0u64;
    for y in 0..shot.h {
        for x in 0..shot.w {
            let i = ((y * shot.w + x) * 4) as usize;
            let (r, g, b, a) = (shot.rgba[i], shot.rgba[i + 1], shot.rgba[i + 2], shot.rgba[i + 3]);
            if a > 200 && hit(r, g, b) {
                n += 1;
                sx += u64::from(x);
                sy += u64::from(y);
            }
        }
    }
    if n == 0 { None } else { Some((sx as f64 / n as f64, sy as f64 / n as f64, n as u32)) }
}

fn turn(src: &Shot, degrees: i64) -> Shot {
    let degrees = degrees.rem_euclid(360);
    if degrees == 0 {
        return Shot { w: src.w, h: src.h, rgba: src.rgba.clone() };
    }
    let (w, h) = if degrees == 180 { (src.w, src.h) } else { (src.h, src.w) };
    let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..src.h {
        for x in 0..src.w {
            let (dx, dy) = match degrees {
                90 => (src.h - 1 - y, x),
                180 => (src.w - 1 - x, src.h - 1 - y),
                270 => (y, src.w - 1 - x),
                _ => unreachable!("quarter turns only"),
            };
            let si = ((y * src.w + x) * 4) as usize;
            let di = ((dy * w + dx) * 4) as usize;
            rgba[di..di + 4].copy_from_slice(&src.rgba[si..si + 4]);
        }
    }
    Shot { w, h, rgba }
}

fn map_point(x: f64, y: f64, w: u32, h: u32, degrees: i64) -> (f64, f64) {
    match degrees.rem_euclid(360) {
        0 => (x, y),
        90 => ((h - 1) as f64 - y, x),
        180 => ((w - 1) as f64 - x, (h - 1) as f64 - y),
        270 => (y, (w - 1) as f64 - x),
        _ => unreachable!("quarter turns only"),
    }
}

fn pixel_fraction(expected: &Shot, actual: &Shot) -> f64 {
    assert_eq!((expected.w, expected.h), (actual.w, actual.h));
    let mut far = 0u64;
    let n = (expected.w as u64) * (expected.h as u64);
    for i in (0..expected.rgba.len()).step_by(4) {
        let d = (0..3).map(|c| expected.rgba[i + c].abs_diff(actual.rgba[i + c])).max().unwrap_or(0);
        if d > 40 {
            far += 1;
        }
    }
    far as f64 / n as f64
}

fn ink(shot: &Shot) -> u32 {
    shot.rgba.chunks_exact(4).filter(|px| px[3] > 200 && (px[0] < 250 || px[1] < 250 || px[2] < 250)).count() as u32
}

fn rotate_to(bytes: &Arc<Vec<u8>>, degrees: i64) -> Arc<Vec<u8>> {
    let mut session = open_session(&[Capability::PageRotate]);
    let id = session.open("composed.pdf", None, bytes.clone(), None).unwrap();
    session.apply(id, Edit::RotatePages { pages: vec![0], degrees }).unwrap();
    session.get(id).unwrap().bytes.clone()
}

fn page_objs(bytes: &Arc<Vec<u8>>) -> (Vec<pdfcraft_cos::ObjRef>, i64, i64) {
    let doc = Document::open(bytes.clone()).unwrap();
    let refs: Vec<_> = pdfcraft_organize::pages(&doc).unwrap().into_iter().map(|p| p.obj).collect();
    let root = doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let tree = doc.get(root);
    let tree = tree.as_dict().unwrap();
    let count = tree.get(b"Count").and_then(Object::as_int).unwrap_or(-1);
    let kids = tree.get(b"Kids").map(|k| doc.resolve(k)).and_then(|k| k.as_array().cloned()).map(|k| k.len() as i64).unwrap_or(-1);
    (refs, count, kids)
}

fn direct_rotate(bytes: &Arc<Vec<u8>>) -> Object {
    let doc = Document::open(bytes.clone()).unwrap();
    let page = pdfcraft_organize::pages(&doc).unwrap()[0].obj;
    doc.get(page).as_dict().unwrap().get(b"Rotate").cloned().unwrap_or(Object::Null)
}

#[test]
fn rendered_rotation_matches_a_turned_picture() {
    let dir = out_dir().join("render");
    std::fs::create_dir_all(&dir).unwrap();
    let (original, composed) = compose(&dir);
    let before = render(composed.as_ref(), 0);
    save_png(&dir.join("before.png"), &before);
    let blank_before = ink(&render(composed.as_ref(), 1));
    let mut notes = Vec::new();
    notes.push(format!("before {}x{} blank-page-ink={blank_before}", before.w, before.h));
    for region in regions() {
        let found = centroid(&before, region.hit);
        notes.push(format!("before {}: {:?}", region.name, found));
        assert!(found.is_some(), "unrotated page is missing {}", region.name);
    }
    let (refs, count, kids) = page_objs(&composed);
    assert_eq!(count, 2);
    assert_eq!(kids, 2);
    let cases = [("right-90", 90i64), ("left-90", -90), ("half", 180), ("full", 360)];
    for (label, degrees) in cases {
        let turned = if degrees == 360 {
            let mut bytes = composed.clone();
            for _ in 0..4 {
                bytes = rotate_to(&bytes, 90);
            }
            bytes
        } else {
            rotate_to(&composed, degrees)
        };
        let shown = degrees.rem_euclid(360);
        assert_eq!(direct_rotate(&turned), Object::Int(shown), "{label}");
        let (after_refs, after_count, after_kids) = page_objs(&turned);
        assert_eq!(after_refs, refs, "{label} changed a page object");
        assert_eq!((after_count, after_kids), (2, 2), "{label}");
        std::fs::write(dir.join(format!("{label}.pdf")), turned.as_ref()).unwrap();
        let shot = render(turned.as_ref(), 0);
        save_png(&dir.join(format!("{label}.png")), &shot);
        let expected = turn(&before, shown);
        save_png(&dir.join(format!("{label}-bitmap.png")), &expected);
        assert_eq!((shot.w, shot.h), (expected.w, expected.h), "{label} page size");
        let fraction = pixel_fraction(&expected, &shot);
        notes.push(format!("{label} pixels-beyond-40={fraction:.6} size={}x{}", shot.w, shot.h));
        assert!(fraction <= PIXEL_FRACTION, "{label} pixel fraction {fraction} exceeds {PIXEL_FRACTION}");
        assert!(ink(&render(turned.as_ref(), 1)) <= blank_before, "{label} drew on the other page");
        for region in regions() {
            let base = centroid(&before, region.hit).unwrap();
            let (ex, ey) = map_point(base.0, base.1, before.w, before.h, shown);
            let got = centroid(&shot, region.hit);
            notes.push(format!("{label} {} expected=({ex:.1},{ey:.1}) got={got:?}", region.name,));
            let Some((x, y, n)) = got else { panic!("{label} lost {}", region.name) };
            let err = ((x - ex).powi(2) + (y - ey).powi(2)).sqrt();
            let ratio = n as f64 / base.2 as f64;
            assert!(err <= CENTROID_PX, "{label} {} centroid moved by {err:.2}px", region.name);
            assert!((COUNT_LO..=COUNT_HI).contains(&ratio), "{label} {} coverage ratio {ratio:.3}", region.name);
        }
        let mut again = open_session(&[]);
        let id = again.open("reopen.pdf", None, turned.clone(), None).unwrap();
        let reopened = again.get(id).unwrap();
        assert_eq!(reopened.info.pages[0].rotation, u16::try_from(shown).unwrap());
        assert_eq!(reopened.info.pages[1].rotation, 0);
        let texts: Vec<_> = reopened
            .added()
            .iter()
            .filter_map(|item| match &item.content {
                pdfcraft_edit::Content::Text(text) => Some((item.page, text.text.clone())),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&(0, "東京".into())), "{texts:?}");
        assert!(texts.contains(&(0, "管理センター".into())), "{texts:?}");
        assert!(texts.iter().all(|(page, _)| *page == 0));
        let stamps: Vec<_> = reopened.info.annotations.iter().filter(|a| a.subtype == "Stamp").map(|a| a.page).collect();
        assert_eq!(stamps, vec![0, 0, 0], "{label} {stamps:?}");
        let fresh = render(reopened.bytes.as_ref(), 0);
        let reopen_fraction = pixel_fraction(&shot, &fresh);
        notes.push(format!("{label} reopen-delta={reopen_fraction:.6}"));
        assert!(reopen_fraction <= 0.001, "{label} reopen render drifted");
        assert_eq!(std::fs::read(dir.join("source.pdf")).unwrap(), original);
    }
    std::fs::write(dir.join("render.txt"), notes.join("\n")).unwrap();
}

#[test]
fn rotation_undo_keeps_text_stamps_and_refuses_other_page_edits() {
    let dir = out_dir().join("undo");
    std::fs::create_dir_all(&dir).unwrap();
    let original = marked_a4();
    let source = dir.join("source.pdf");
    std::fs::write(&source, &original).unwrap();
    let mut session = open_session(&[Capability::EditText, Capability::Stamp, Capability::PageRotate, Capability::SaveAs]);
    let id = session.open("source.pdf", Some(source.to_string_lossy().into_owned()), Arc::new(original.clone()), None).unwrap();
    place(&mut session, id);
    let composed = session.get(id).unwrap().bytes.clone();
    session.apply(id, Edit::RotatePages { pages: vec![0, 0], degrees: 90 }).unwrap();
    assert_eq!(session.get(id).unwrap().info.pages[0].rotation, 90, "a repeated index turns the page once");
    assert_eq!(session.undo(id).unwrap().as_str(), "Rotate pages");
    assert_eq!(session.get(id).unwrap().info.pages[0].rotation, 0);
    assert_eq!(stamp_pages(&session, id), vec![0, 0, 0]);
    assert!(session.redo(id).is_ok());
    assert_eq!(session.get(id).unwrap().info.pages[0].rotation, 90);
    assert_eq!(stamp_pages(&session, id), vec![0, 0, 0]);
    assert_eq!(session.undo(id).unwrap().as_str(), "Rotate pages");
    assert_eq!(session.undo(id).unwrap().as_str(), "Add stamp");
    assert_eq!(session.get(id).unwrap().info.pages[0].rotation, 0);
    assert_eq!(stamp_pages(&session, id), vec![0, 0], "undoing the stamp leaves the other stamps");
    assert!(texts_of(&session, id).contains(&"東京".to_string()));

    let mut placed_after = open_session(&[Capability::EditText, Capability::Stamp, Capability::PageRotate, Capability::SaveAs]);
    let id = placed_after.open("source.pdf", None, Arc::new(original.clone()), None).unwrap();
    place(&mut placed_after, id);
    placed_after.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let later = png(32, 32, |_, _| [0, 180, 0, 255]);
    placed_after.apply(id, stamp("later", later, [40.0, 200.0, 100.0, 260.0])).unwrap();
    assert_eq!(placed_after.undo(id).unwrap().as_str(), "Add stamp");
    assert_eq!(placed_after.get(id).unwrap().info.pages[0].rotation, 90, "undoing the later stamp leaves the rotation");
    assert_eq!(stamp_pages(&placed_after, id), vec![0, 0, 0]);
    let kept = render(placed_after.get(id).unwrap().bytes.as_ref(), 0);
    save_png(&dir.join("undo-stamp-keeps-rotation.png"), &kept);
    assert!(centroid(&kept, |r, g, b| r > 220 && b > 220 && g < 40).is_some(), "the earlier stamp is still drawn");
    assert_eq!(ink(&render(placed_after.get(id).unwrap().bytes.as_ref(), 1)), 0, "the later stamp did not land on the other page");
    assert_eq!(placed_after.undo(id).unwrap().as_str(), "Rotate page");
    assert_eq!(placed_after.get(id).unwrap().info.pages[0].rotation, 0);
    assert_eq!(stamp_pages(&placed_after, id), vec![0, 0, 0]);
    assert!(texts_of(&placed_after, id).contains(&"管理センター".to_string()));

    let mut bad = open_session(&[Capability::PageRotate]);
    let id = bad.open("source.pdf", None, composed.clone(), None).unwrap();
    let unchanged = bad.get(id).unwrap().bytes.clone();
    let err = bad.apply(id, Edit::RotatePages { pages: vec![0, 9], degrees: 90 }).unwrap_err();
    assert!(matches!(err, EditError::Organize(pdfcraft_organize::OrganizeError::NoSuchPage(9))));
    assert_eq!(bad.get(id).unwrap().info.pages[0].rotation, 0);
    assert_eq!(bad.get(id).unwrap().info.pages[1].rotation, 0);
    assert!(Arc::ptr_eq(&bad.get(id).unwrap().bytes, &unchanged));
    assert!(matches!(bad.apply(id, Edit::DeletePages { pages: vec![0] }), Err(EditError::ViewOnly)));
    assert!(matches!(bad.apply(id, Edit::MovePages { pages: vec![0], to: 1 }), Err(EditError::ViewOnly)));
    assert!(Arc::ptr_eq(&bad.get(id).unwrap().bytes, &unchanged));
    assert!(matches!(bad.save_bytes(id), Err(EditError::ViewOnly)));
    bad.grant(Capability::SaveAs);
    bad.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let failed = bad.save_bytes(id).unwrap_err();
    assert!(matches!(failed, EditError::ViewOnly));
    assert_eq!(bad.get(id).unwrap().info.pages[0].rotation, 90, "a refused save keeps the edit");
    let saved = bad.save_edited_copy_bytes(id).unwrap();
    assert!(saved.starts_with(composed.as_ref()), "rotation is an incremental update; the previous bytes stay");
    assert_eq!(std::fs::read(&source).unwrap(), original);
}

fn stamp_pages(session: &Session, id: pdfcraft_engine::DocId) -> Vec<usize> {
    session.get(id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").map(|a| a.page).collect()
}

fn texts_of(session: &Session, id: pdfcraft_engine::DocId) -> Vec<String> {
    session
        .get(id)
        .unwrap()
        .added()
        .iter()
        .filter_map(|item| match &item.content {
            pdfcraft_edit::Content::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect()
}

fn revise(base: &[u8], edit: impl FnOnce(&mut Document)) -> Vec<u8> {
    let mut doc = Document::open(Arc::new(base.to_vec())).unwrap();
    edit(&mut doc);
    let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
    assert!(saved.starts_with(base), "the previous revision stays at the front of the file");
    saved
}

fn sig_dict() -> Dict {
    let mut sig = Dict::new();
    sig.set(b"Type".to_vec(), Object::name("Sig"));
    sig.set(b"Filter".to_vec(), Object::name("Adobe.PPKLite"));
    sig.set(b"SubFilter".to_vec(), Object::name("adbe.pkcs7.detached"));
    sig.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]));
    sig.set(b"Contents".to_vec(), Object::String(PdfString::literal(vec![0, 1, 2, 3])));
    sig
}

fn widget(page: pdfcraft_cos::ObjRef, value: Option<Object>, named: bool) -> Dict {
    let mut field = Dict::new();
    field.set(b"Type".to_vec(), Object::name("Annot"));
    field.set(b"Subtype".to_vec(), Object::name("Widget"));
    field.set(b"FT".to_vec(), Object::name("Sig"));
    if named {
        field.set(b"T".to_vec(), Object::String(PdfString::literal(b"Sig1".to_vec())));
    }
    field.set(b"Rect".to_vec(), Object::Array(vec![10.into(), 10.into(), 80.into(), 40.into()]));
    field.set(b"P".to_vec(), Object::Ref(page));
    if let Some(value) = value {
        field.set(b"V".to_vec(), value);
    }
    field
}

fn attach_field(doc: &mut Document, field: Dict) -> pdfcraft_cos::ObjRef {
    let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
    let field_ref = doc.add(field);
    doc.update_dict(page, |d| d.set(b"Annots".to_vec(), Object::Array(vec![Object::Ref(field_ref)]))).unwrap();
    let root = doc.root().unwrap();
    let mut form = Dict::new();
    form.set(b"Fields".to_vec(), Object::Array(vec![Object::Ref(field_ref)]));
    doc.update_dict(root, |d| d.set(b"AcroForm".to_vec(), Object::Dict(form))).unwrap();
    field_ref
}

struct Probe {
    label: &'static str,
    bytes: Vec<u8>,
    /// `Some(true)` must refuse rotation. `Some(false)` is an unsigned field and may rotate.
    expect_refuse: Option<bool>,
}

#[test]
fn signature_structures_refuse_rotation_unless_the_field_has_no_value() {
    let dir = out_dir().join("signatures");
    std::fs::create_dir_all(&dir).unwrap();
    let base = blank();
    std::fs::write(dir.join("unsigned-base.pdf"), &base).unwrap();
    let with_value = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        let sig = doc.add(sig_dict());
        attach_field(doc, widget(page, Some(Object::Ref(sig)), true));
    });
    let empty = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        attach_field(doc, widget(page, None, true));
    });
    let null_value = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        attach_field(doc, widget(page, Some(Object::Null), true));
    });
    let foreign = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        attach_field(doc, widget(page, Some(Object::String(PdfString::literal(b"not-a-dictionary".to_vec()))), true));
    });
    let nameless = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        let sig = doc.add(sig_dict());
        attach_field(doc, widget(page, Some(Object::Ref(sig)), false));
    });
    let docmdp = revise(&base, |doc| {
        let sig = doc.add(sig_dict());
        let root = doc.root().unwrap();
        let mut perms = Dict::new();
        perms.set(b"DocMDP".to_vec(), Object::Ref(sig));
        doc.update_dict(root, |d| d.set(b"Perms".to_vec(), Object::Dict(perms))).unwrap();
    });
    let orphan = {
        let mut doc = Document::open(Arc::new(base.clone())).unwrap();
        let _ = doc.add(sig_dict());
        let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
        assert!(saved.starts_with(&base));
        saved
    };
    let timestamp = revise(&base, |doc| {
        let mut stamp = Dict::new();
        stamp.set(b"Type".to_vec(), Object::name("DocTimeStamp"));
        stamp.set(b"Filter".to_vec(), Object::name("Adobe.PPKLite"));
        stamp.set(b"SubFilter".to_vec(), Object::name("ETSI.RFC3161"));
        stamp.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]));
        stamp.set(b"Contents".to_vec(), Object::String(PdfString::literal(vec![9, 9])));
        let _ = doc.add(stamp);
    });
    let hierarchical = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        let mut value = Dict::new();
        value.set(b"Reason".to_vec(), Object::String(PdfString::literal(b"nested")));
        let mut child = Dict::new();
        child.set(b"Subtype".to_vec(), Object::name("Widget"));
        child.set(b"P".to_vec(), Object::Ref(page));
        child.set(b"V".to_vec(), Object::Dict(value));
        let child_ref = doc.add(child);
        let mut parent = Dict::new();
        parent.set(b"FT".to_vec(), Object::name("Sig"));
        parent.set(b"T".to_vec(), Object::String(PdfString::literal(b"Parent".to_vec())));
        parent.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(child_ref)]));
        attach_field(doc, parent);
    });
    let usage = |key: &[u8]| {
        let key = key.to_vec();
        revise(&base, move |doc| {
            let mut target = Dict::new();
            target.set(b"Reason".to_vec(), Object::String(PdfString::literal(b"usage")));
            let refer = doc.add(target);
            let root = doc.root().unwrap();
            let mut perms = Dict::new();
            perms.set(key, Object::Ref(refer));
            doc.update_dict(root, |dict| dict.set(b"Perms".to_vec(), Object::Dict(perms))).unwrap();
        })
    };
    let ur = usage(b"UR");
    let ur3 = usage(b"UR3");
    let broken = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        attach_field(doc, widget(page, Some(Object::Ref(pdfcraft_cos::ObjRef::new(999, 0))), true));
    });
    let removed_later = {
        let mut doc = Document::open(Arc::new(base.clone())).unwrap();
        let sig = doc.add(sig_dict());
        let with_sig = write_incremental(&doc, &SaveOptions::default()).unwrap();
        let mut doc = Document::open(Arc::new(with_sig)).unwrap();
        doc.free(sig);
        let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
        assert!(!pdfcraft_cos::Document::open(Arc::new(saved.clone())).unwrap().object_numbers().contains(&sig.num));
        saved
    };
    let mentioned = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        let stream = doc.add(Object::Stream(pdfcraft_cos::Stream::from_raw(Dict::new(), b"BT (/ByteRange /DocMDP /Sig) Tj ET".to_vec())));
        doc.update_dict(page, |dict| dict.set(b"Contents".to_vec(), Object::Ref(stream))).unwrap();
        let root = doc.root().unwrap();
        let mut note = Dict::new();
        note.set(b"Comment".to_vec(), Object::String(PdfString::literal(b"ByteRange DocMDP Sig")));
        doc.update_dict(root, |dict| dict.set(b"PrivateNote".to_vec(), Object::Dict(note))).unwrap();
    });
    let text_field = revise(&base, |doc| {
        let page = pdfcraft_organize::pages(doc).unwrap()[0].obj;
        let mut field = Dict::new();
        field.set(b"FT".to_vec(), Object::name("Tx"));
        field.set(b"T".to_vec(), Object::String(PdfString::literal(b"Name".to_vec())));
        field.set(b"V".to_vec(), Object::String(PdfString::literal(b"unsigned".to_vec())));
        field.set(b"P".to_vec(), Object::Ref(page));
        attach_field(doc, field);
    });
    let orphan_of = |dict: Dict| {
        revise(&base, move |doc| {
            let _ = doc.add(dict);
        })
    };
    let byte_range_only = orphan_of({
        let mut dict = Dict::new();
        dict.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]));
        dict
    });
    let byte_range_short = orphan_of({
        let mut dict = Dict::new();
        dict.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into()]));
        dict
    });
    let byte_range_text = orphan_of({
        let mut dict = Dict::new();
        dict.set(b"ByteRange".to_vec(), Object::String(PdfString::literal(b"not-an-array")));
        dict
    });
    let byte_range_indirect = revise(&base, |doc| {
        let values = doc.add(Object::Array(vec![0.into(), 1.into(), 2.into(), 3.into()]));
        let mut dict = Dict::new();
        dict.set(b"ByteRange".to_vec(), Object::Ref(values));
        let _ = doc.add(dict);
    });
    let byte_range_dangling = orphan_of({
        let mut dict = Dict::new();
        dict.set(b"ByteRange".to_vec(), Object::Ref(pdfcraft_cos::ObjRef::new(999, 0)));
        dict
    });
    let byte_range_contents = orphan_of({
        let mut dict = Dict::new();
        dict.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]));
        dict.set(b"Contents".to_vec(), Object::String(PdfString::literal(vec![4, 5])));
        dict
    });
    let byte_range_filter = orphan_of({
        let mut dict = Dict::new();
        dict.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]));
        dict.set(b"Filter".to_vec(), Object::name("FlateDecode"));
        dict
    });
    let subfilter_only = |name: &'static str| {
        orphan_of({
            let mut dict = Dict::new();
            dict.set(b"SubFilter".to_vec(), Object::name(name));
            dict
        })
    };
    let subfilter_indirect = revise(&base, |doc| {
        let name = doc.add(Object::name("adbe.pkcs7.detached"));
        let mut dict = Dict::new();
        dict.set(b"SubFilter".to_vec(), Object::Ref(name));
        let _ = doc.add(dict);
    });
    let subfilter_unknown = subfilter_only("Custom.NotASignature");
    let subfilter_integer = orphan_of({
        let mut dict = Dict::new();
        dict.set(b"SubFilter".to_vec(), Object::Int(7));
        dict
    });
    let byte_range_freed = {
        let mut doc = Document::open(Arc::new(base.clone())).unwrap();
        let mut dict = Dict::new();
        dict.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]));
        let added = doc.add(dict);
        let with_range = write_incremental(&doc, &SaveOptions::default()).unwrap();
        let mut doc = Document::open(Arc::new(with_range)).unwrap();
        doc.free(added);
        let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
        assert!(!pdfcraft_cos::Document::open(Arc::new(saved.clone())).unwrap().object_numbers().contains(&added.num));
        saved
    };
    let probes = [
        Probe { label: "sig-field-with-value", bytes: with_value, expect_refuse: Some(true) },
        Probe { label: "sig-field-without-value", bytes: empty, expect_refuse: Some(false) },
        Probe { label: "sig-field-null-value", bytes: null_value, expect_refuse: Some(false) },
        Probe { label: "sig-field-nondict-value", bytes: foreign, expect_refuse: Some(true) },
        Probe { label: "nameless-sig-field-with-value", bytes: nameless, expect_refuse: Some(true) },
        Probe { label: "perms-docmdp", bytes: docmdp, expect_refuse: Some(true) },
        Probe { label: "orphan-byte-range", bytes: orphan, expect_refuse: Some(true) },
        Probe { label: "doc-timestamp", bytes: timestamp, expect_refuse: Some(true) },
        Probe { label: "nested-signature-field", bytes: hierarchical, expect_refuse: Some(true) },
        Probe { label: "perms-ur", bytes: ur, expect_refuse: Some(true) },
        Probe { label: "perms-ur3", bytes: ur3, expect_refuse: Some(true) },
        Probe { label: "broken-signature-reference", bytes: broken, expect_refuse: Some(true) },
        Probe { label: "signature-freed-in-later-revision", bytes: removed_later, expect_refuse: Some(true) },
        Probe { label: "text-mentions-signature-words", bytes: mentioned, expect_refuse: Some(false) },
        Probe { label: "unsigned-text-field", bytes: text_field, expect_refuse: Some(false) },
        Probe { label: "byte-range-only", bytes: byte_range_only, expect_refuse: Some(true) },
        Probe { label: "byte-range-short", bytes: byte_range_short, expect_refuse: Some(true) },
        Probe { label: "byte-range-string", bytes: byte_range_text, expect_refuse: Some(true) },
        Probe { label: "byte-range-indirect", bytes: byte_range_indirect, expect_refuse: Some(true) },
        Probe { label: "byte-range-dangling", bytes: byte_range_dangling, expect_refuse: Some(true) },
        Probe { label: "byte-range-and-contents", bytes: byte_range_contents, expect_refuse: Some(true) },
        Probe { label: "byte-range-and-filter", bytes: byte_range_filter, expect_refuse: Some(true) },
        Probe { label: "subfilter-pkcs7-detached", bytes: subfilter_only("adbe.pkcs7.detached"), expect_refuse: Some(true) },
        Probe { label: "subfilter-pkcs7-sha1", bytes: subfilter_only("adbe.pkcs7.sha1"), expect_refuse: Some(true) },
        Probe { label: "subfilter-x509", bytes: subfilter_only("adbe.x509.rsa_sha1"), expect_refuse: Some(true) },
        Probe { label: "subfilter-cades", bytes: subfilter_only("ETSI.CAdES.detached"), expect_refuse: Some(true) },
        Probe { label: "subfilter-rfc3161", bytes: subfilter_only("ETSI.RFC3161"), expect_refuse: Some(true) },
        Probe { label: "subfilter-indirect", bytes: subfilter_indirect, expect_refuse: Some(true) },
        Probe { label: "subfilter-unknown", bytes: subfilter_unknown, expect_refuse: Some(true) },
        Probe { label: "subfilter-integer", bytes: subfilter_integer, expect_refuse: Some(true) },
        Probe { label: "byte-range-freed-in-later-revision", bytes: byte_range_freed, expect_refuse: Some(true) },
    ];
    let mut lines = Vec::new();
    let mut failures = Vec::new();
    for probe in probes {
        std::fs::write(dir.join(format!("{}.pdf", probe.label)), &probe.bytes).unwrap();
        let mut session = open_session(&[Capability::PageRotate]);
        let id = match session.open(probe.label, None, Arc::new(probe.bytes), None) {
            Ok(id) => id,
            Err(err) => {
                lines.push(format!("{}: open failed: {err}", probe.label));
                failures.push(probe.label);
                continue;
            }
        };
        let doc = session.get(id).unwrap();
        let signed = doc.is_signed();
        let dirty = doc.dirty;
        let undo = doc.can_undo().map(str::to_string);
        let redo = doc.can_redo().map(str::to_string);
        let fields: Vec<_> = doc.signatures.iter().map(|s| format!("{}:{}", s.field, s.signed)).collect();
        let ptr = doc.bytes.as_ptr();
        let rotation = doc.info.pages[0].rotation;
        let applied = session.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 });
        let after = session.get(id).unwrap();
        let unchanged = after.bytes.as_ptr() == ptr && after.info.pages[0].rotation == rotation;
        let refused = applied.is_err() && unchanged;
        lines.push(format!("{}: is_signed={signed} fields={fields:?} refused={refused} unchanged={unchanged} apply={applied:?}", probe.label));
        if let Some(expect_refuse) = probe.expect_refuse {
            let message_ok = match &applied {
                Err(EditError::NotPermitted("page rotation on a signed document")) => true,
                Err(EditError::Invalid(text)) => {
                    text == "Page rotation is unavailable because the document contains or may contain a digital signature."
                }
                _ => false,
            };
            let history_ok = !expect_refuse
                || (after.dirty == dirty && after.can_undo().map(str::to_string) == undo && after.can_redo().map(str::to_string) == redo);
            let safe = if expect_refuse { refused && message_ok && history_ok } else { !signed && applied.is_ok() };
            if !safe {
                failures.push(probe.label);
            }
        }
    }
    std::fs::write(dir.join("signatures.txt"), lines.join("\n")).unwrap();
    assert!(failures.is_empty(), "signature protection: {failures:?}\n{}", lines.join("\n"));
}

#[test]
fn refusing_rotation_keeps_text_stamps_and_the_original_file() {
    assert!(pdfcraft_fonts::document_japanese_font().is_some(), "Japanese font is required for this check");
    let dir = out_dir().join("guard");
    std::fs::create_dir_all(&dir).unwrap();
    let base = blank();
    let protected = revise(&base, |doc| {
        let sig = doc.add(sig_dict());
        let root = doc.root().unwrap();
        let mut perms = Dict::new();
        perms.set(b"DocMDP".to_vec(), Object::Ref(sig));
        doc.update_dict(root, |dict| dict.set(b"Perms".to_vec(), Object::Dict(perms))).unwrap();
    });
    let path = dir.join("protected.pdf");
    std::fs::write(&path, &protected).unwrap();
    let mut session = open_session(&[Capability::PageRotate, Capability::EditText, Capability::Stamp]);
    let id = session.open("protected.pdf", Some(path.to_string_lossy().into_owned()), Arc::new(protected.clone()), None).unwrap();
    session
        .apply(
            id,
            Edit::AddText { page: 0, text: AddedText { rect: [10.0, 40.0, 120.0, 80.0], text: "東京".into(), size: 18.0, ..AddedText::default() } },
        )
        .unwrap();
    session.apply(id, stamp("mark", png(8, 8, |_, _| [255, 0, 255, 255]), [20.0, 100.0, 60.0, 140.0])).unwrap();
    let before = session.get(id).unwrap().bytes.clone();
    assert!(session.get(id).unwrap().dirty);
    assert_eq!(session.get(id).unwrap().can_undo(), Some("Add stamp"));
    let err = session.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap_err();
    assert_eq!(err.to_string(), "Page rotation is unavailable because the document contains or may contain a digital signature.");
    let doc = session.get(id).unwrap();
    assert!(Arc::ptr_eq(&doc.bytes, &before));
    assert_eq!(doc.info.pages[0].rotation, 0);
    assert!(doc.dirty);
    assert_eq!(doc.can_undo(), Some("Add stamp"));
    assert!(texts_of(&session, id).contains(&"東京".to_string()));
    assert_eq!(stamp_pages(&session, id), vec![0]);
    assert_eq!(std::fs::read(&path).unwrap(), protected);
    assert_eq!(session.undo(id).unwrap().as_str(), "Add stamp");
    assert_eq!(session.get(id).unwrap().info.pages[0].rotation, 0);
    assert!(texts_of(&session, id).contains(&"東京".to_string()));
    assert!(session.apply(id, Edit::DeletePages { pages: vec![0] }).is_err());
    assert_eq!(session.get(id).unwrap().info.pages.len(), 1);
}
