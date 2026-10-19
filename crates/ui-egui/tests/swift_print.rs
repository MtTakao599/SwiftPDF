//! SwiftPDF v0.4.0: printing stays a separate grant. View-only stays on, in-place save stays
//! refused, and a print uses the existing print dialog. Nothing here submits a job to a printer.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use egui::Key;
use egui::Modifiers;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::print::spool::Printer;
use pdfcraft_engine::{AddedText, Edit};
use pdfcraft_ui_egui::command_policy::REFUSED;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp, PrintWhich, UiMode};
use sha2::{Digest, Sha256};

static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn pdf() -> Vec<u8> {
    let body = "BT /F1 24 Tf 40 180 Td (Hello page) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 /MediaBox [0 0 300 300] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 6 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 6 0 R >> >> >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

struct Dir(PathBuf);

impl Dir {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!("swiftpdf-print-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn printers() -> pdfcraft_ui_egui::PrinterSource {
    Arc::new(|| {
        vec![
            Printer { name: "Office".into(), default: false, two_sided: Some(true) },
            Printer { name: "Front Desk".into(), default: true, two_sided: Some(false) },
        ]
    })
}

fn harness(bytes: Vec<u8>, path: Option<String>) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.run_inline = true;
        app.set_view_only(true);
        app.grant_save_as();
        app.grant_edit_text();
        app.grant_print();
        app.set_ui_mode(UiMode::Simple);
        app.printer_source = Some(printers());
        app.open_bytes("opened.pdf", path, bytes).unwrap();
        app
    });
    h.run_steps(4);
    h
}

fn sheet_text(bytes: Vec<u8>) -> String {
    let bytes = Arc::new(bytes);
    let mut renderer = pdfcraft_render::PageRenderer::new(bytes, Default::default());
    renderer
        .render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Text, scale: 1.0, ..Default::default() })
        .text
        .map(|text| text.plain_text())
        .unwrap_or_default()
}

fn wait_for_printers(h: &mut Harness<'static, PdfCraftApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.state().print_draft.printers_loading && std::time::Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!h.state().print_draft.printers_loading, "the test printer list did not arrive");
}

/// A closed combo stores its current text as the widget value. Open it, then pick the menu row.
fn choose(h: &mut Harness<'static, PdfCraftApp>, current: &str, choice: &str) {
    h.get_by_value(current).click();
    h.run_steps(2);
    h.get_by_label(choice).click();
    h.run_steps(2);
}

/// Save as PDF, never the Print button, so no spooler job is submitted.
fn save_print(h: &mut Harness<'static, PdfCraftApp>, dest: &str) {
    h.state_mut().print_draft.printer = None;
    h.state_mut().print_draft.printer_picked = true;
    h.state_mut().save_override = Some(dest.to_owned());
    h.run_steps(2);
    h.get_by_label("Save as PDF").click();
    h.run_steps(4);
}

#[test]
fn the_print_dialog_previews_and_changes_printer_paper_orientation_and_range() {
    let _serial = serial();
    let mut h = harness(pdf(), None);
    assert!(h.state().session.view_only());
    assert!(pdfcraft_ui_egui::output_policy::blocked());
    assert!(h.state().command_allowed("print.dialog"));
    assert!(!h.state().command_allowed("file.save"));
    assert!(!h.state().command_allowed("page.combine"));
    assert!(!h.state().command_allowed("comment.stamp"));
    h.get_by_label("印刷").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print));
    h.get_by_label("Sheet 1 of 2");
    wait_for_printers(&mut h);
    assert_eq!(h.state().print_draft.printer.as_deref(), Some("Front Desk"));
    choose(&mut h, "Front Desk", "Office");
    assert_eq!(h.state().print_draft.printer.as_deref(), Some("Office"));
    h.get_by_label("Landscape").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.orientation, pdfcraft_engine::print::Orientation::Landscape);
    choose(&mut h, "US Letter", "A4");
    let a4 = pdfcraft_engine::print::PAPERS.iter().position(|(name, _)| *name == "A4").unwrap();
    assert_eq!(h.state().print_draft.paper, a4);
    h.get_by_label("Actual size").click();
    h.run_steps(1);
    assert!(matches!(h.state().print_draft.size, pdfcraft_engine::print::SizeMode::Actual));
    h.get_by_label("Fit").click();
    h.run_steps(1);
    let rect = h.get_by(|node| format!("{:?}", node.role()) == "SpinButton" && node.value().as_deref() == Some("1")).rect();
    let start = rect.center();
    h.hover_at(start);
    h.run_steps(1);
    h.drag_at(start);
    h.run_steps(1);
    let up = start + egui::vec2(0.0, -20.0);
    h.hover_at(up);
    h.run_steps(1);
    h.drop_at(up);
    h.run_steps(2);
    assert!(h.state().print_draft.copies >= 2, "dragging the copies field left it at {}", h.state().print_draft.copies);
    h.get_by_label("Print in grayscale").click();
    h.run_steps(1);
    assert!(h.state().print_draft.grayscale);
    choose(&mut h, "Off", "Flip on long edge");
    assert_eq!(h.state().print_draft.duplex, pdfcraft_engine::print::spool::Duplex::LongEdge);
    h.get_by_label("Pages").click();
    h.run_steps(1);
    h.state_mut().print_draft.range = "1".into();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.which, PrintWhich::Range);
    h.get_by_label("Sheet 1 of 1");
    let id = h.state().views[0].id;
    let undo = h.state().session.get(id).unwrap().can_undo().map(str::to_owned);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().session.get(id).unwrap().can_undo().map(str::to_owned), undo);
    assert!(!h.state().session.get(id).unwrap().dirty);
}

#[test]
fn unsaved_text_is_printed_without_touching_the_original_or_undo() {
    let _serial = serial();
    let dir = Dir::new("unsaved");
    let original = dir.join("opened.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    let hash = sha256(&bytes);
    let sample = AddedText { text: "日本語".into(), rect: [40.0, 200.0, 240.0, 260.0], size: 16.0, ..AddedText::default() };
    let typed = if pdfcraft_engine::first_undrawable(&sample).is_none() { "日本語" } else { "Printed note" };
    let mut h = harness(bytes, Some(original.to_string_lossy().into_owned()));
    let scale = h.state().views[0].page_screen_rect(0).expect("page").width() / 300.0;
    let at = h.state().views[0].page_screen_rect(0).unwrap().min + egui::vec2(40.0, 36.0) * scale;
    h.get_by_label("文字入力").click();
    h.run_steps(3);
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(3);
    h.event(egui::Event::Text(typed.into()));
    h.run_steps(2);
    let id = h.state().views[0].id;
    assert_eq!(h.state().views[0].content.draft.as_ref().map(|d| d.text.as_str()), Some(typed));
    assert!(h.state().session.get(id).unwrap().can_undo().is_none(), "typing has not been committed");
    let dest = dir.join("print.pdf");
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print));
    h.get_by_label("Sheet 1 of 2");
    save_print(&mut h, &dest.to_string_lossy());
    assert!(h.state().dialog.is_none(), "a saved print closes the dialog: {:?}", h.state().toast);
    let printed = std::fs::read(&dest).unwrap();
    let shown = sheet_text(printed.clone());
    assert!(shown.contains(typed), "{shown}");
    if typed == "日本語" {
        assert!(printed.windows(5).any(|w| w == b"Type3") || shown.contains("日本語"));
    }
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
    assert_eq!(h.state().views[0].content.draft.as_ref().map(|d| d.text.as_str()), Some(typed), "printing does not commit the draft");
    assert!(h.state().session.get(id).unwrap().can_undo().is_none());
    assert!(!h.state().session.get(id).unwrap().dirty);
    assert_eq!(h.state().session.get(id).unwrap().path.as_deref(), Some(original.to_string_lossy().as_ref()));

    h.get_by_label("Done").click();
    h.run_steps(4);
    assert!(h.state().views[0].content.draft.is_none());
    assert!(h.state().session.get(id).unwrap().can_undo().is_some());
    assert!(h.state().session.get(id).unwrap().dirty);
    let again = dir.join("print-again.pdf");
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    save_print(&mut h, &again.to_string_lossy());
    assert!(sheet_text(std::fs::read(&again).unwrap()).contains(typed));
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
    assert!(h.state().session.get(id).unwrap().dirty, "a print does not count as saving the document");
    assert!(h.state_mut().execute("edit.undo"));
    h.run_steps(2);
    assert!(h.state().session.get(id).unwrap().added().is_empty());
    assert!(h.state_mut().execute("edit.redo"));
    h.run_steps(2);
    assert!(!h.state().session.get(id).unwrap().added().is_empty());
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);

    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert!(!h.state().session.get(id).unwrap().added().is_empty(), "cancel leaves the edit");

    let existing = dir.join("already.pdf");
    std::fs::write(&existing, b"keep").unwrap();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    save_print(&mut h, &original.to_string_lossy());
    assert_eq!(h.state().dialog, Some(Dialog::Print), "writing onto the original fails and the dialog stays");
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
    h.state_mut().dialog = None;
    h.state_mut().toast = None;
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    save_print(&mut h, &existing.to_string_lossy());
    assert_eq!(std::fs::read(&existing).unwrap(), b"keep");
    assert_eq!(h.state().dialog, Some(Dialog::Print));
    assert!(!h.state().session.get(id).unwrap().added().is_empty());
    h.get_by_label("Cancel").click();
    h.run_steps(2);

    h.state_mut().toast = None;
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run_steps(3);
    assert_eq!(h.state().toast.as_ref().map(|(text, _)| text.as_str()), Some(REFUSED));
    assert!(!h.state_mut().execute("file.save"));
    h.state_mut().save_override = Some(original.to_string_lossy().into_owned());
    assert!(h.state_mut().execute("file.save_as"));
    h.run_steps(2);
    assert!(!h.state_mut().confirm_save_copy(), "the original is not overwritten");
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
}

#[test]
fn a_print_ban_is_honoured_and_simple_matches_detailed() {
    let _serial = serial();
    use pdfcraft_engine::{Changes, Printing, Protection, Session};
    let mut author = Session::new().with_clock(|| 1_700_000_000);
    let id = author.open("opened.pdf", None, Arc::new(pdf()), None).unwrap();
    author
        .apply(
            id,
            Edit::Protect(Protection {
                permissions_password: Some("owner".into()),
                printing: Printing::None,
                changes: Changes::None,
                ..Protection::default()
            }),
        )
        .unwrap();
    let locked = author.save_bytes(id).unwrap();
    let hash = sha256(locked.as_ref());
    let mut banned = harness(locked.as_ref().clone(), None);
    banned.state_mut().toast = None;
    assert!(banned.state_mut().execute("print.dialog"), "the command runs, then the document's own ban refuses it");
    banned.run_steps(2);
    assert!(banned.state().dialog.is_none());
    let toast = banned.state().toast.as_ref().map(|(text, _)| text.clone()).unwrap_or_default();
    assert!(toast.contains("does not allow printing") || toast.contains("印刷が許可されて"), "{toast}");
    assert_eq!(sha256(banned.state().session.get(banned.state().views[0].id).unwrap().bytes.as_ref()), hash);

    let mut h = harness(pdf(), None);
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "Ctrl+P");
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    h.get_by_label("Home").click();
    h.run_steps(3);
    h.get_by_label("印刷").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "Home 印刷 uses the print dialog");
    assert!(h.state().active.is_some());
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    h.get_by_label("Home").click();
    h.run_steps(2);
    h.state_mut().toast = None;
    h.get_by_label("結合").click();
    h.run_steps(2);
    assert_eq!(h.state().toast.as_ref().map(|(text, _)| text.as_str()), Some("「結合」は準備中です。"));
    assert!(h.state().dialog.is_none());

    h.state_mut().set_ui_mode(UiMode::Detailed);
    h.state_mut().active = Some(0);
    h.run_steps(4);
    h.get_by(|node| node.label().is_some_and(|label| label.starts_with("Print ("))).click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "the detailed print button");
    h.get_by_label("Cancel").click();
    h.run_steps(2);
}

#[test]
fn the_simple_print_button_stays_on_screen_when_the_window_is_narrow() {
    let _serial = serial();
    let mut h = Harness::builder().with_size(egui::vec2(420.0, 700.0)).build_eframe(|_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.run_inline = true;
        app.set_view_only(true);
        app.grant_save_as();
        app.grant_edit_text();
        app.grant_print();
        app.set_ui_mode(UiMode::Simple);
        app.printer_source = Some(printers());
        app.open_bytes("opened.pdf", None, pdf()).unwrap();
        app
    });
    h.run_steps(4);
    let rect = h.get_by_label("印刷").rect();
    assert!(rect.left() >= 0.0 && rect.right() <= 430.0 && rect.bottom() < 260.0, "印刷 is off screen: {rect:?}");
}
