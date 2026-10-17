//! A view-only app ([`PdfCraftApp::set_view_only`]) writes no file a user asks for: not over the
//! original, not a new PDF, print, export, attachment or copied image. Reading, copying text and
//! the app's own settings keep working.
//!
//! The output policy is process-wide, so these tests live in their own binary and take
//! [`LOCK`] to run one at a time.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use egui_kittest::Harness;
use pdfcraft_ui_egui::output_policy::{self, REFUSED};
use pdfcraft_ui_egui::{PdfCraftApp, SaveTarget};

static LOCK: Mutex<()> = Mutex::new(());

/// Run one test at a time, view-only unless the test says otherwise.
fn serial() -> MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    output_policy::set_blocked(true);
    guard
}

fn pdf(objs: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// One page that says "Hello view-only", with a text attachment `notes.txt`.
fn fixture() -> Vec<u8> {
    let body = "BT /F1 24 Tf 20 150 Td (Hello view-only) Tj ET";
    pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names [(notes.txt) 6 0 R] >> >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 300] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 7 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /EmbeddedFile /Length 11 >>\nstream\nhello notes\nendstream".into(),
        "<< /Type /Filespec /F (notes.txt) /UF (notes.txt) /EF << /F 5 0 R >> >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ])
}

/// A folder of its own for one test, removed afterwards.
struct Dir(PathBuf);

impl Dir {
    fn new(test: &str) -> Self {
        let d = std::env::temp_dir().join(format!("swiftpdf-view-only-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn files(&self) -> Vec<String> {
        std::fs::read_dir(&self.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A view-only app showing `original` (written to disk first), as the desktop app starts.
fn harness(original: &Path) -> Harness<'static, PdfCraftApp> {
    std::fs::write(original, fixture()).unwrap();
    let path = s(original);
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_view_only(true);
        app.system_clipboard = false;
        app.open_bytes("original.pdf", Some(path), fixture()).unwrap();
        app
    });
    h.run_steps(4);
    h
}

fn toast(h: &Harness<'static, PdfCraftApp>) -> String {
    h.state().toast.as_ref().map(|t| t.0.clone()).unwrap_or_default()
}

#[test]
fn the_original_is_never_written() {
    let _serial = serial();
    let dir = Dir::new("original");
    let original = dir.join("original.pdf");
    let mut h = harness(&original);
    assert!(h.state().session.view_only());
    assert!(!h.state_mut().save_active(SaveTarget::InPlace));
    assert_eq!(toast(&h), REFUSED);
    // Save As onto the original, and onto a new file, answered at once.
    for target in [s(&original), s(&dir.join("copy.pdf"))] {
        h.state_mut().toast = None;
        h.state_mut().save_override = Some(target);
        assert!(!h.state_mut().save_active(SaveTarget::As));
        h.state_mut().execute("file.save");
        h.run_steps(2);
    }
    assert_eq!(std::fs::read(&original).unwrap(), fixture(), "the original is untouched");
    assert_eq!(dir.files(), ["original.pdf"], "nothing else was written");
}

#[test]
fn a_save_panel_answered_later_writes_nothing() {
    let _serial = serial();
    let dir = Dir::new("delayed");
    let original = dir.join("original.pdf");
    let mut h = harness(&original);
    // The panel was asked for while outputs were allowed and editing too (so there is a change to
    // save), and answered after both were refused.
    output_policy::set_blocked(false);
    h.state_mut().session.set_view_only(false);
    let id = h.state().views[0].id;
    h.state_mut().apply_edit(pdfcraft_engine::Edit::SetInfo { key: "Title".into(), value: "Changed".into() });
    assert!(h.state().session.get(id).unwrap().dirty);
    h.state_mut().pick_override = Some(vec![s(&original)]);
    assert!(!h.state_mut().save_active(SaveTarget::As), "waits for the panel");
    output_policy::set_blocked(true);
    h.run_steps(4);
    assert!(toast(&h).contains(REFUSED), "{}", toast(&h));
    assert_eq!(std::fs::read(&original).unwrap(), fixture());
    assert_eq!(dir.files(), ["original.pdf"]);
}

#[test]
fn printing_is_refused_before_anything_is_prepared() {
    let _serial = serial();
    let dir = Dir::new("print");
    let mut h = harness(&dir.join("original.pdf"));
    h.state_mut().open_print();
    assert!(h.state().dialog.is_none(), "no print dialog: {:?}", h.state().dialog);
    assert_eq!(toast(&h), REFUSED);
    h.state_mut().toast = None;
    h.state_mut().save_override = Some(s(&dir.join("printed.pdf")));
    assert!(!h.state_mut().print_now(), "neither to paper nor as PDF");
    assert_eq!(toast(&h), REFUSED);
    assert_eq!(dir.files(), ["original.pdf"]);
}

#[test]
fn no_export_writes_a_file() {
    let _serial = serial();
    let dir = Dir::new("export");
    let mut h = harness(&dir.join("original.pdf"));
    let id = h.state().views[0].id;
    // Office: written straight to the preset path, so the writer itself refuses.
    for (format, name) in [(pdfcraft_engine::compare::OfficeFormat::Docx, "out.docx"), (pdfcraft_engine::compare::OfficeFormat::Html, "out.html")] {
        h.state_mut().toast = None;
        h.state_mut().save_override = Some(s(&dir.join(name)));
        h.state_mut().export_office_dialog(format);
        assert!(toast(&h).contains(REFUSED), "{name}: {}", toast(&h));
    }
    // Comments and form data (XFDF, CSV): the engine refuses to produce them.
    for name in ["out.xfdf", "out.csv"] {
        h.state_mut().toast = None;
        h.state_mut().save_override = Some(s(&dir.join(name)));
        h.state_mut().export_data_dialog(true, true);
        assert!(toast(&h).contains("view-only"), "{name}: {}", toast(&h));
    }
    // A spreadsheet merged from data files.
    h.state_mut().toast = None;
    h.state_mut().save_override = Some(s(&dir.join("report.csv")));
    h.state_mut().merge_data_files(vec![("form.pdf".into(), include_bytes!("data/form.pdf").to_vec())]);
    assert!(toast(&h).contains(REFUSED), "{}", toast(&h));
    // Through a save panel: refused before it opens.
    h.state_mut().save_override = None;
    h.state_mut().toast = None;
    h.state_mut().pick_override = Some(vec![s(&dir.join("panel.docx"))]);
    h.state_mut().export_office_dialog(pdfcraft_engine::compare::OfficeFormat::Docx);
    assert_eq!(toast(&h), REFUSED);
    h.run_steps(2);
    // New PDFs from the document: extracted pages, a comment summary.
    h.state_mut().toast = None;
    h.state_mut().extract_selection();
    assert!(toast(&h).contains("view-only"), "{}", toast(&h));
    h.state_mut().toast = None;
    h.state_mut().summarize_comments();
    assert!(toast(&h).contains("view-only"), "{}", toast(&h));
    assert_eq!(dir.files(), ["original.pdf"]);
    assert_eq!(h.state().views.len(), 1, "no new document opened");
    assert_eq!(h.state().views[0].id, id);
}

#[test]
fn attachments_open_but_are_not_saved() {
    let _serial = serial();
    let dir = Dir::new("attachment");
    let mut h = harness(&dir.join("original.pdf"));
    let id = h.state().views[0].id;
    let index = h.state().session.get(id).unwrap().info.attachments.iter().position(|a| a.name == "notes.txt").expect("the attachment");
    h.state_mut().pick_override = Some(vec![s(&dir.join("notes.txt"))]);
    h.state_mut().attachment_action(id, index, false);
    h.run_steps(2);
    assert_eq!(toast(&h), REFUSED);
    assert_eq!(dir.files(), ["original.pdf"]);
}

#[test]
fn a_picture_is_not_copied_as_a_file() {
    let _serial = serial();
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(40, 30, image::Rgba([10, 200, 30, 255])))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let pdf = pdfcraft_engine::Session::new().create_from_images(&[("p.png".into(), png.into_inner())]).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_view_only(true);
        app.system_clipboard = false;
        app.open_bytes("pic.pdf", None, pdf.to_vec()).unwrap();
        app
    });
    h.run_steps(4);
    let i = h.state().active.unwrap();
    let at = h.state().views[i].page_screen_rect(0).expect("on screen").center();
    h.hover_at(at);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    assert_eq!(h.state().views[i].picked_image.map(|p| (p.page, p.index)), Some((0, 0)), "the picture is selected");
    h.event(egui::Event::Copy);
    h.run_steps(2);
    assert!(h.state().last_copied_file.is_none(), "no copy was made");
    assert_eq!(toast(&h), REFUSED);
}

#[test]
fn text_is_still_copied_and_found() {
    let _serial = serial();
    let dir = Dir::new("text");
    let mut h = harness(&dir.join("original.pdf"));
    let mut loaded = false;
    for _ in 0..200 {
        h.run_steps(2);
        if h.state_mut().views[0].select_all() {
            loaded = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(loaded, "the page text loaded");
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("Hello view-only"));
    assert!(h.state_mut().execute("edit.copy"));
    h.step();
    let copied = h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "Hello view-only"));
    assert!(copied, "the text went to the clipboard");
    assert!(h.state().session.get(h.state().views[0].id).unwrap().words().iter().any(|w| w.text == "view-only"), "searchable");
}

#[test]
fn settings_and_the_last_session_are_still_kept() {
    let _serial = serial();
    let dir = Dir::new("settings");
    let original = dir.join("original.pdf");
    let mut h = harness(&original);
    h.state_mut().set_option("theme", "dark").unwrap();
    let saved = h.state().persist();
    assert!(!saved.is_empty());
    let mut again = PdfCraftApp::new();
    again.restore(&saved);
    let restored = again.persist();
    assert!(restored.contains("\"theme\":\"Dark\""), "the setting comes back");
    assert!(restored.contains("original.pdf"), "so does the recent file");
    // Settings are the app's own data: refusing outputs never touches them.
    assert!(output_policy::blocked());
}

#[test]
fn turning_view_only_off_allows_outputs_again() {
    let _serial = serial();
    let dir = Dir::new("off");
    let original = dir.join("original.pdf");
    let mut h = harness(&original);
    h.state_mut().set_view_only(false);
    assert!(!output_policy::blocked());
    let out = dir.join("out.html");
    h.state_mut().save_override = Some(s(&out));
    h.state_mut().export_office_dialog(pdfcraft_engine::compare::OfficeFormat::Html);
    assert!(out.exists(), "{}", toast(&h));
}
