//! A view-only app runs only the commands a reader needs (`command_policy`): menus grey out the
//! rest, and every other way in (shortcuts, the palette, the tool panels, `execute`) refuses
//! them, unknown commands included. Reading, the view and the app keep working.
//!
//! [`PdfCraftApp::set_view_only`] also sets the process-wide output policy, so these tests live in
//! their own binary and take [`LOCK`] to run one at a time.

use std::sync::{Mutex, MutexGuard, PoisonError};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::command_policy::REFUSED;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
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

/// Two pages that say "Hello view-only".
fn fixture() -> Vec<u8> {
    let body = "BT /F1 24 Tf 20 150 Td (Hello view-only) Tj ET";
    pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 /MediaBox [0 0 300 300] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 6 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 6 0 R >> >> >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ])
}

/// A view-only app showing `bytes`, as the desktop app starts.
fn harness_with(bytes: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_view_only(true);
        app.system_clipboard = false;
        app.open_bytes("doc.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    h
}

fn harness() -> Harness<'static, PdfCraftApp> {
    harness_with(fixture())
}

fn toast(h: &Harness<'static, PdfCraftApp>) -> String {
    h.state().toast.as_ref().map(|t| t.0.clone()).unwrap_or_default()
}

/// The document as the engine holds it: page count, each page's own rotation, unsaved changes.
fn document(h: &Harness<'static, PdfCraftApp>) -> (usize, Vec<u16>, bool) {
    let d = h.state().session.get(h.state().views[0].id).unwrap();
    (d.info.pages.len(), d.info.pages.iter().map(|p| p.rotation).collect(), d.dirty)
}

/// Wait for page 1's text and select all of it.
fn select_all_text(h: &mut Harness<'static, PdfCraftApp>) {
    for _ in 0..200 {
        h.run_steps(2);
        if h.state_mut().views[0].select_all() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the page text never loaded");
}

fn copied_text(h: &Harness<'static, PdfCraftApp>) -> Option<String> {
    h.output().platform_output.commands.iter().find_map(|c| match c {
        egui::OutputCommand::CopyText(t) => Some(t.clone()),
        _ => None,
    })
}

#[test]
fn menus_grey_out_what_changes_or_writes_files() {
    let _serial = serial();
    let mut h = harness();
    // A menu item's label is followed by its shortcut ("Save Ctrl+S", "Revert ").
    let disabled = |h: &Harness<'static, PdfCraftApp>, label: &str| {
        let item = format!("{label} ");
        let all = h.query_all_by(|n| n.label().is_some_and(|l| l.starts_with(&item))).count();
        assert!(all > 0, "{label} is in the menu");
        h.query_all_by(|n| n.label().is_some_and(|l| l.starts_with(&item)) && n.is_disabled()).count() == all
    };
    h.get_by_label("Menu").click();
    h.run_steps(2);
    h.get_by_label("File ⏵").hover();
    h.run_steps(3);
    for label in ["Save", "Save as…", "Print…", "Export to Word…", "Reduce file size…", "Combine files…", "New blank PDF"] {
        assert!(disabled(&h, label), "File ▸ {label} is disabled");
    }
    for label in ["Open…", "Document properties…", "Close file"] {
        assert!(!disabled(&h, label), "File ▸ {label} still works");
    }
    h.get_by_label("Pages ⏵").hover();
    h.run_steps(3);
    for label in [
        "Rotate current page clockwise 90°",
        "Rotate current page counterclockwise 90°",
        "Delete pages",
        "Extract pages…",
        "Split document…",
        "New bookmark",
    ] {
        assert!(disabled(&h, label), "Pages ▸ {label} is disabled");
    }
    // A disabled item does nothing when clicked.
    h.get_by(|n| n.label().is_some_and(|l| l.starts_with("Delete pages "))).click();
    h.run_steps(3);
    assert_eq!(document(&h), (2, vec![0, 0], false));
    // The toolbar's Print and Save as well.
    h.key_press(Key::Escape);
    h.run_steps(2);
    for tip in ["Print (", "Save ("] {
        let all = h.query_all_by(|n| n.label().is_some_and(|l| l.starts_with(tip))).count();
        assert!(all > 0, "the toolbar has {tip}…)");
        let off = h.query_all_by(|n| n.label().is_some_and(|l| l.starts_with(tip)) && n.is_disabled()).count();
        assert_eq!(off, all, "the toolbar's {tip}…) is disabled");
    }
}

#[test]
fn shortcuts_that_change_or_write_are_refused() {
    let _serial = serial();
    let mut h = harness();
    for (modifiers, key, what) in [
        (Modifiers::COMMAND, Key::S, "save"),
        (Modifiers::COMMAND | Modifiers::SHIFT, Key::S, "save as"),
        (Modifiers::COMMAND, Key::P, "print"),
        (Modifiers::COMMAND, Key::B, "new bookmark"),
        (Modifiers::COMMAND, Key::Z, "undo"),
        (Modifiers::COMMAND, Key::J, "JavaScript console"),
    ] {
        h.state_mut().toast = None;
        h.key_press_modifiers(modifiers, key);
        h.run_steps(3);
        assert_eq!(toast(&h), REFUSED, "{what}");
        assert!(h.state().dialog.is_none(), "{what} opened {:?}", h.state().dialog);
    }
    assert_eq!(document(&h), (2, vec![0, 0], false));
    // A reader's shortcuts still run: document properties.
    h.key_press_modifiers(Modifiers::COMMAND, Key::D);
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Some(Dialog::Properties(_))), "{:?}", h.state().dialog);
}

#[test]
fn edits_outputs_and_unknown_commands_are_refused() {
    let _serial = serial();
    let mut h = harness();
    for id in [
        "file.save",
        "file.save_as",
        "file.revert",
        "create.blank",
        "page.combine",
        "page.rotate",
        "page.rotate_ccw",
        "page.delete",
        "page.insert_blank",
        "page.duplicate",
        "page.organize",
        "page.extract",
        "page.split",
        "print.dialog",
        "export.image",
        "export.text",
        "export.docx",
        "optimize.reduce",
        "comment.note",
        "comment.highlight",
        "comment.stamp",
        "comment.export",
        "form.prepare",
        "form.fields",
        "sign.digital",
        "sign.certify_invisible",
        "protect.unlock",
        "protect.password",
        "edit.snapshot",
        "edit.edit_text",
        "edit.undo",
        "redact.mark",
        "tools.js_console",
        "tools.document_js",
        "no.such.command",
    ] {
        h.state_mut().toast = None;
        assert!(!h.state_mut().execute(id), "{id} ran");
        assert_eq!(toast(&h), REFUSED, "{id}");
        h.run_steps(1);
    }
    assert!(h.state().dialog.is_none(), "{:?}", h.state().dialog);
    assert!(matches!(h.state().quick_tool, pdfcraft_ui_egui::QuickTool::Select), "no editing tool was chosen");
    assert!(!h.state().views[0].organize);
    assert_eq!(document(&h), (2, vec![0, 0], false));
    // The tool panels and palette run commands this way.
    h.state_mut().toast = None;
    h.state_mut().run_command("export.text");
    assert_eq!(toast(&h), REFUSED);
    h.state_mut().run_command("no.such.tool");
    assert_eq!(toast(&h), REFUSED);
    assert!(h.state().dialog.is_none());
}

#[test]
fn a_refused_tool_does_not_ask_for_a_file_first() {
    let _serial = serial();
    let dir = std::env::temp_dir().join(format!("swiftpdf-view-only-commands-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("doc.pdf");
    std::fs::write(&file, fixture()).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_view_only(true);
        app
    });
    h.run_steps(2);
    // With no document open, a tool needing one would ask for a file and run on it.
    h.state_mut().pick_override = Some(vec![file.to_string_lossy().into_owned()]);
    h.state_mut().run_command("page.split");
    h.run_steps(6);
    assert_eq!(toast(&h), REFUSED);
    assert!(h.state().views.is_empty(), "no file was asked for");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reading_commands_still_work() {
    let _serial = serial();
    let mut h = harness();
    let id = h.state().views[0].id;
    assert!(h.state_mut().execute("view.layout.two_up"));
    assert_eq!(h.state().views[0].layout, pdfcraft_ui_egui::canvas::PageLayout::TwoUp);
    assert!(h.state_mut().execute("view.layout.single"));
    assert_eq!(h.state().views[0].layout, pdfcraft_ui_egui::canvas::PageLayout::Single);
    assert!(h.state_mut().execute("view.continuous"));
    // Zoom with the view's own keys.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num1);
    h.run_steps(3);
    assert!((h.state().views[0].zoom - 1.0).abs() < 1e-3, "actual size: {}", h.state().views[0].zoom);
    // Rotating the view leaves the pages as they are.
    h.get_by_label("Menu").click();
    h.run_steps(2);
    h.get_by_label("View ⏵").hover();
    h.run_steps(3);
    // The menu item, not the toolbar button
    // ("Rotate the view 90° clockwise (not saved in the PDF) (Ctrl+Shift++)").
    h.get_by(|n| n.label().is_some_and(|l| l.starts_with("Rotate view clockwise ") && !l.contains('('))).click();
    h.run_steps(3);
    assert_eq!(h.state().views[0].rotation, 90, "the view turned");
    assert_eq!(document(&h), (2, vec![0, 0], false), "the pages didn't");
    // Navigation and finding.
    h.state_mut().views[0].go_to_page(1);
    h.run_steps(4);
    assert_eq!(h.state().views[0].current, 1);
    assert!(h.state_mut().execute("edit.find"));
    assert!(h.state().views[0].find.is_some());
    // Properties, signatures, preferences and help.
    for (command, check) in [
        ("file.properties", (|d: &Dialog| matches!(d, Dialog::Properties(_))) as fn(&Dialog) -> bool),
        ("protect.properties", |d| matches!(d, Dialog::Properties(_))),
        ("app.preferences", |d| matches!(d, Dialog::Preferences)),
        ("help.shortcuts", |d| matches!(d, Dialog::Shortcuts)),
        ("help.about", |d| matches!(d, Dialog::About)),
    ] {
        h.state_mut().dialog = None;
        assert!(h.state_mut().execute(command), "{command}");
        assert!(h.state().dialog.as_ref().is_some_and(check), "{command}: {:?}", h.state().dialog);
    }
    h.state_mut().dialog = None;
    assert!(h.state_mut().execute("sign.panel"));
    assert!(h.state_mut().execute("sign.validate"));
    assert!(h.state_mut().execute("comment.list"));
    assert!(h.state_mut().execute("view.theme.dark"));
    h.run_steps(2);
    // Closing the tab.
    assert!(h.state_mut().execute("file.close"));
    h.run_steps(4);
    assert!(h.state().session.get(id).is_none(), "closed");
}

#[test]
fn text_is_copied_as_the_document_allows() {
    let _serial = serial();
    let mut h = harness();
    select_all_text(&mut h);
    assert!(h.state_mut().execute("edit.copy"));
    h.step();
    assert_eq!(copied_text(&h).as_deref(), Some("Hello view-only"));

    // A document that forbids copying still refuses it.
    let mut author = pdfcraft_engine::Session::new();
    let id = author.open("doc.pdf", None, std::sync::Arc::new(fixture()), None).unwrap();
    let protection =
        pdfcraft_engine::Protection { permissions_password: Some("owner".into()), copy: false, ..pdfcraft_engine::Protection::default() };
    author.apply(id, pdfcraft_engine::Edit::Protect(protection)).unwrap();
    let locked = author.save_bytes(id).unwrap().to_vec();
    let mut h = harness_with(locked);
    select_all_text(&mut h);
    assert!(!h.state_mut().execute("edit.copy"));
    h.step();
    assert_eq!(copied_text(&h), None, "nothing reached the clipboard");
    assert_ne!(toast(&h), REFUSED, "refused for the document's permission, not by view-only: {}", toast(&h));
}

#[test]
fn pictures_and_snapshots_are_not_copied_to_the_clipboard() {
    let _serial = serial();
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(40, 30, image::Rgba([10, 200, 30, 255])))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let pdf = pdfcraft_engine::Session::new().create_from_images(&[("p.png".into(), png.into_inner())]).unwrap();
    let mut h = harness_with(pdf.to_vec());
    let i = h.state().active.unwrap();
    let at = h.state().views[i].page_screen_rect(0).expect("on screen").center();
    h.hover_at(at);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Secondary, pressed, modifiers: Default::default() });
    }
    h.run_steps(2);
    h.get_by_label("Copy Image").click();
    h.run_steps(2);
    assert!(h.state().last_copied_image.is_none(), "the picture wasn't copied");
    assert_eq!(toast(&h), REFUSED);
    // The snapshot tool can't be chosen, and taking one is refused anyway.
    assert!(!h.state_mut().execute("edit.snapshot"));
    assert_eq!(h.state_mut().snapshot(i, 0, [0.0, 0.0, 100.0, 100.0]), Err(REFUSED.to_string()));
    assert!(h.state().last_snapshot.is_none());
}

#[test]
fn turning_view_only_off_restores_every_command() {
    let _serial = serial();
    let mut h = harness();
    h.state_mut().set_view_only(false);
    assert!(h.state_mut().execute("page.rotate"));
    h.run_steps(2);
    assert_eq!(document(&h), (2, vec![90, 0], true));
    assert!(h.state_mut().execute("edit.snapshot"));
    assert!(matches!(h.state().quick_tool, pdfcraft_ui_egui::QuickTool::Snapshot));
}
