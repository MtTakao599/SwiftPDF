//! A view-only session may save the opened PDF to a new file, and nothing else.
//!
//! The output block stays on, so in-place save, print and export still write nothing. These tests
//! share that process-wide flag, so they take [`LOCK`] and run one at a time.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::{Capability, Changes, Edit, Printing, Protection, Session};
use pdfcraft_ui_egui::command_policy::REFUSED;
use pdfcraft_ui_egui::output_policy;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp, SaveTarget, UiMode};
use sha2::{Digest, Sha256};

static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    output_policy::set_blocked(true);
    output_policy::set_save_as_allowed(false);
    guard
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn pdf() -> Vec<u8> {
    let body = "BT /F1 24 Tf 20 150 Td (Hello copy) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 300] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
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
        let path = std::env::temp_dir().join(format!("swiftpdf-safe-save-{test}-{}", std::process::id()));
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

fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn app(grant: bool) -> PdfCraftApp {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.run_inline = true;
    app.set_view_only(true);
    if grant {
        app.grant_save_as();
    }
    app
}

fn open_file(app: &mut PdfCraftApp, path: &Path, bytes: &[u8]) {
    app.open_bytes("opened.pdf", Some(s(path)), bytes.to_vec()).unwrap();
}

fn toast(app: &PdfCraftApp) -> String {
    app.toast.as_ref().map(|(text, _)| text.clone()).unwrap_or_default()
}

#[test]
fn saving_a_copy_is_refused_until_it_is_granted() {
    let _serial = serial();
    let dir = Dir::new("refused");
    let original = dir.join("opened.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    let hash = sha256(&bytes);
    let mut app = app(false);
    open_file(&mut app, &original, &bytes);
    app.save_override = Some(s(&dir.join("copy.pdf")));
    assert!(!app.session.allows(Capability::SaveAs));
    assert!(!app.command_allowed("file.save_as"));
    assert!(!app.execute("file.save_as"));
    assert_eq!(toast(&app), REFUSED);
    assert!(app.dialog.is_none());
    assert!(!dir.join("copy.pdf").exists());
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
}

#[test]
fn a_copy_is_a_new_file_and_the_original_is_unchanged() {
    let _serial = serial();
    let dir = Dir::new("copy");
    let original = dir.join("opened.pdf");
    let dest = dir.join("copy.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    let hash = sha256(&bytes);
    let mut app = app(true);
    assert!(output_policy::blocked());
    assert!(output_policy::save_as_allowed());
    open_file(&mut app, &original, &bytes);
    let id = app.views[0].id;
    app.save_override = Some(s(&dest));
    assert!(app.command_allowed("file.save_as"));
    assert!(!app.command_allowed("file.save"));
    assert!(!app.command_allowed("print.dialog"));
    assert!(!app.command_allowed("page.combine"));
    assert!(!app.command_allowed("page.split"));
    assert!(!app.command_allowed("edit.text"));
    assert!(!app.command_allowed("comment.stamp"));
    assert!(app.execute("file.save_as"));
    let pending = app.pending_save_copy.clone().expect("the copy waits for confirmation");
    assert_eq!(pending.source_name, "opened.pdf");
    assert_eq!(pending.source_path.as_deref(), Some(s(&original).as_str()));
    assert_eq!(pending.dest, s(&dest));
    assert!(matches!(app.dialog, Some(Dialog::SaveCopy)));
    assert!(!dest.exists(), "confirming is what writes");
    assert!(app.confirm_save_copy());
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash, "the original bytes");
    let copied = std::fs::read(&dest).unwrap();
    assert_eq!(sha256(&copied), hash, "the copy is the opened file");
    assert_eq!(app.session.get(id).unwrap().path.as_deref(), Some(s(&original).as_str()));
    assert!(!app.session.get(id).unwrap().dirty);
    assert!(app.views.len() >= 2, "the copy opens for reading");
    let mut again = Session::new().with_view_only(true);
    let opened = again.open("copy.pdf", Some(s(&dest)), Arc::new(copied), None).unwrap();
    assert_eq!(again.get(opened).unwrap().info.pages.len(), 1);
    assert!(output_policy::blocked(), "print, export and in-place save stay blocked");
    assert!(!dir.join("blocked.pdf").exists());
    assert!(!app.execute("file.save"));
    assert_eq!(toast(&app), REFUSED);
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
}

#[test]
fn the_same_file_and_an_existing_file_are_not_written() {
    let _serial = serial();
    let dir = Dir::new("refuse-dest");
    let nested = dir.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    let original = dir.join("opened.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    let hash = sha256(&bytes);
    let mut app = app(true);
    open_file(&mut app, &original, &bytes);

    let via_parent = nested.join("..").join("opened.pdf");
    app.save_override = Some(s(&via_parent));
    assert!(app.execute("file.save_as"));
    assert!(!app.confirm_save_copy());
    assert!(toast(&app).contains("original file"), "{}", toast(&app));
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);

    #[cfg(windows)]
    {
        app.save_override = Some(s(&dir.join("OPENED.PDF")));
        assert!(app.execute("file.save_as"));
        assert!(!app.confirm_save_copy());
        assert!(toast(&app).contains("original file"), "{}", toast(&app));
        assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
    }

    let link = dir.join("hard.pdf");
    std::fs::hard_link(&original, &link).unwrap();
    app.save_override = Some(s(&link));
    assert!(app.execute("file.save_as"));
    assert!(!app.confirm_save_copy());
    assert!(toast(&app).contains("original file"), "{}", toast(&app));
    assert_eq!(std::fs::read(&link).unwrap(), bytes);

    let existing = dir.join("already.pdf");
    std::fs::write(&existing, b"KEEP").unwrap();
    app.save_override = Some(s(&existing));
    assert!(app.execute("file.save_as"));
    assert!(!app.confirm_save_copy());
    assert!(toast(&app).contains("not overwritten"), "{}", toast(&app));
    assert_eq!(std::fs::read(&existing).unwrap(), b"KEEP");
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);

    std::fs::write(&original, b"CHANGED-OUTSIDE").unwrap();
    app.save_override = Some(s(&original));
    assert!(app.execute("file.save_as"));
    assert!(!app.confirm_save_copy());
    assert_eq!(std::fs::read(&original).unwrap(), b"CHANGED-OUTSIDE");
}

#[test]
fn an_external_change_to_the_original_is_left_in_place() {
    let _serial = serial();
    let dir = Dir::new("external");
    let original = dir.join("opened.pdf");
    let dest = dir.join("copy.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    let mut app = app(true);
    open_file(&mut app, &original, &bytes);
    std::fs::write(&original, b"CHANGED-OUTSIDE").unwrap();
    app.save_override = Some(s(&dest));
    assert!(app.execute("file.save_as"));
    assert!(app.confirm_save_copy());
    assert_eq!(std::fs::read(&original).unwrap(), b"CHANGED-OUTSIDE");
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    assert!(toast(&app).contains("left as it is") || toast(&app).contains("原本"), "{}", toast(&app));
}

#[test]
fn a_japanese_path_saves_and_reopens() {
    let _serial = serial();
    let dir = Dir::new("japanese");
    let folder = dir.join("保存先");
    std::fs::create_dir_all(&folder).unwrap();
    let original = folder.join("原本.pdf");
    let dest = folder.join("副本.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    let hash = sha256(&bytes);
    let mut app = app(true);
    app.open_bytes("原本.pdf", Some(s(&original)), bytes.clone()).unwrap();
    app.save_override = Some(s(&dest));
    assert!(app.execute("file.save_as"));
    let pending = app.pending_save_copy.clone().unwrap();
    assert_eq!(pending.source_name, "原本.pdf");
    assert!(pending.dest.contains("副本.pdf"));
    assert!(app.confirm_save_copy());
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    let mut again = Session::new().with_view_only(true);
    let id = again.open("副本.pdf", Some(s(&dest)), Arc::new(std::fs::read(&dest).unwrap()), None).unwrap();
    assert_eq!(again.get(id).unwrap().info.pages.len(), 1);
}

#[test]
fn a_restricted_pdf_keeps_its_restrictions_in_the_copy() {
    let _serial = serial();
    let mut author = Session::new().with_clock(|| 1_700_000_000);
    let id = author.open("opened.pdf", None, Arc::new(pdf()), None).unwrap();
    author
        .apply(
            id,
            Edit::Protect(Protection {
                open_password: Some("reader".into()),
                permissions_password: Some("owner".into()),
                printing: Printing::None,
                changes: Changes::None,
                copy: false,
                ..Protection::default()
            }),
        )
        .unwrap();
    let locked = author.save_bytes(id).unwrap();
    let dir = Dir::new("restricted");
    let original = dir.join("opened.pdf");
    let dest = dir.join("copy.pdf");
    std::fs::write(&original, locked.as_ref()).unwrap();
    let hash = sha256(locked.as_ref());
    let mut app = app(true);
    app.open_bytes("opened.pdf", Some(s(&original)), locked.as_ref().to_vec()).unwrap();
    assert!(app.password_prompt.is_some(), "the open password is still required");
    app.submit_password(Some("reader".into()));
    assert!(app.password_prompt.is_none());
    assert!(!app.session.get(app.views[0].id).unwrap().allows_copying());
    assert!(!app.session.get(app.views[0].id).unwrap().allows_printing());
    app.save_override = Some(s(&dest));
    assert!(app.execute("file.save_as"));
    assert!(app.confirm_save_copy());
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
    let copied = std::fs::read(&dest).unwrap();
    assert_eq!(sha256(&copied), hash, "the copy is still the encrypted file");
    let mut again = Session::new().with_view_only(true);
    let id = again.open("copy.pdf", Some(s(&dest)), Arc::new(copied), Some("reader")).unwrap();
    let doc = again.get(id).unwrap();
    assert!(!doc.allows_copying());
    assert!(!doc.allows_printing());
    assert!(!doc.allows_modification());
    assert_eq!(again.unlock(id, "owner"), Err(pdfcraft_engine::EditError::ViewOnly));
    assert!(!app.execute("protect.unlock"));
}

#[test]
fn simple_and_detailed_modes_use_the_same_save() {
    let _serial = serial();
    let dir = Dir::new("modes");
    let original = dir.join("opened.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    for mode in [UiMode::Simple, UiMode::Detailed] {
        let name = format!("{mode:?}.pdf");
        let dest = dir.join(&name);
        let mut app = app(true);
        app.set_ui_mode(mode);
        open_file(&mut app, &original, &bytes);
        app.save_override = Some(s(&dest));
        assert!(app.execute("file.save_as"), "{mode:?}");
        assert!(matches!(app.dialog, Some(Dialog::SaveCopy)), "{mode:?}");
        let pending = app.pending_save_copy.clone().unwrap();
        assert_eq!(pending.source_name, "opened.pdf");
        assert_eq!(pending.dest, s(&dest));
        assert!(app.confirm_save_copy(), "{mode:?}");
        assert_eq!(std::fs::read(&dest).unwrap(), bytes);
        assert!(!app.execute("file.save"), "{mode:?}");
    }
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), sha256(&pdf()));
}

fn viewer(mode: UiMode, original: PathBuf, dest: PathBuf, bytes: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    let original_s = s(&original);
    let dest_s = s(&dest);
    let mut harness = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.run_inline = true;
        app.set_view_only(true);
        app.grant_save_as();
        app.set_ui_mode(mode);
        app.save_override = Some(dest_s);
        app.open_bytes("opened.pdf", Some(original_s.clone()), bytes).unwrap();
        app
    });
    harness.run_steps(4);
    harness
}

#[test]
fn the_simple_bar_and_the_detailed_menu_confirm_the_same_copy() {
    let _serial = serial();
    let dir = Dir::new("gui");
    let original = dir.join("opened.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    let hash = sha256(&bytes);

    let simple_dest = dir.join("simple.pdf");
    let mut simple = viewer(UiMode::Simple, original.clone(), simple_dest.clone(), bytes.clone());
    assert!(simple.query_by_label("名前を付けて保存").is_some());
    simple.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    simple.run_steps(3);
    assert!(matches!(simple.state().dialog, Some(Dialog::SaveCopy)), "Ctrl+Shift+S opens the copy confirmation");
    simple.get_by_label("Cancel").click();
    simple.run_steps(2);
    assert!(simple.state().dialog.is_none(), "cancel writes nothing");
    assert!(!simple_dest.exists());
    simple.get_by_label("名前を付けて保存").click();
    simple.run_steps(3);
    assert!(matches!(simple.state().dialog, Some(Dialog::SaveCopy)));
    let pending = simple.state().pending_save_copy.clone().unwrap();
    assert_eq!(pending.source_name, "opened.pdf");
    assert!(pending.dest.ends_with("simple.pdf"));
    let text_has = |needle: &str| {
        simple
            .query_all_by(|node| {
                let text = format!("{}{}", node.label().unwrap_or_default(), node.value().unwrap_or_default());
                text.contains(needle)
            })
            .count()
    };
    assert!(text_has("opened.pdf") >= 1, "the confirmation names the original");
    assert!(text_has("simple.pdf") >= 1, "the confirmation names the destination");
    simple.get_by_label("Save").click();
    simple.run_steps(4);
    assert_eq!(std::fs::read(&simple_dest).unwrap(), bytes);
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
    simple.state_mut().toast = None;
    simple.key_press_modifiers(Modifiers::COMMAND, Key::S);
    simple.run_steps(3);
    assert_eq!(simple.state().toast.as_ref().map(|(text, _)| text.as_str()), Some(REFUSED));
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);

    let detailed_dest = dir.join("detailed.pdf");
    let mut detailed = viewer(UiMode::Detailed, original.clone(), detailed_dest.clone(), bytes.clone());
    assert!(detailed.query_by_label("名前を付けて保存").is_none(), "the simple bar is not the detailed window");
    detailed.get_by_label("Menu").click();
    detailed.run_steps(2);
    detailed.get_by_label("File ⏵").hover();
    detailed.run_steps(3);
    let offered = detailed.query_all_by(|node| node.label().is_some_and(|label| label.starts_with("Save as…")) && !node.is_disabled()).count();
    assert!(offered > 0, "Save as is offered once the capability is granted");
    detailed.get_by(|node| node.label().is_some_and(|label| label.starts_with("Save as…"))).click();
    detailed.run_steps(3);
    assert!(matches!(detailed.state().dialog, Some(Dialog::SaveCopy)));
    detailed.get_by_label("Save").click();
    detailed.run_steps(4);
    assert_eq!(std::fs::read(&detailed_dest).unwrap(), bytes);
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
    let save = detailed.query_all_by(|node| node.label().is_some_and(|label| label.starts_with("Save ("))).count();
    let save_off = detailed.query_all_by(|node| node.label().is_some_and(|label| label.starts_with("Save (")) && node.is_disabled()).count();
    assert!(save > 0 && save == save_off, "Ctrl+S / the toolbar Save stays disabled");
}

#[test]
fn without_the_grant_the_simple_bar_has_no_save() {
    let _serial = serial();
    let mut harness = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.run_inline = true;
        app.set_view_only(true);
        app.set_ui_mode(UiMode::Simple);
        app.open_bytes("opened.pdf", None, pdf()).unwrap();
        app
    });
    harness.run_steps(4);
    assert!(harness.query_by_label("名前を付けて保存").is_none());
    assert!(harness.query_by_label("検索").is_some());
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    harness.run_steps(3);
    assert_eq!(harness.state().toast.as_ref().map(|(text, _)| text.as_str()), Some(REFUSED));
    assert!(harness.state().dialog.is_none());
}

/// The real window has no `save_override`, so Save as goes through the native panel. That panel
/// used to be refused as ordinary output (`Ask::Save`) even after the copy was granted.
fn picker(mode: UiMode, original: PathBuf, dest: PathBuf, bytes: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    let original_s = original.to_string_lossy().into_owned();
    let dest_s = dest.to_string_lossy().into_owned();
    let mut harness = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.run_inline = true;
        app.set_view_only(true);
        app.grant_save_as();
        app.set_ui_mode(mode);
        app.pick_override = Some(vec![dest_s]);
        app.open_bytes("opened.pdf", Some(original_s), bytes).unwrap();
        app
    });
    harness.run_steps(4);
    harness
}

fn output_refused(harness: &Harness<'_, PdfCraftApp>) -> bool {
    harness.state().toast.as_ref().is_some_and(|(text, _)| text == output_policy::REFUSED)
}

#[test]
fn the_granted_copy_panel_is_not_refused_as_view_only_output() {
    let _serial = serial();
    let dir = Dir::new("panel");
    let original = dir.join("opened.pdf");
    let bytes = pdf();
    std::fs::write(&original, &bytes).unwrap();
    let hash = sha256(&bytes);

    let simple_dest = dir.join("simple.pdf");
    let mut simple = picker(UiMode::Simple, original.clone(), simple_dest.clone(), bytes.clone());
    simple.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    simple.run_steps(4);
    assert!(!output_refused(&simple), "Ctrl+Shift+S must not be stopped by the view-only output notice");
    assert!(matches!(simple.state().dialog, Some(Dialog::SaveCopy)), "Ctrl+Shift+S reaches the copy confirmation");
    assert!(!simple_dest.exists(), "choosing a path does not write yet");
    simple.get_by_label("Cancel").click();
    simple.run_steps(2);
    assert!(simple.state().dialog.is_none());
    simple.state_mut().toast = None;
    assert!(!simple.state_mut().save_active(SaveTarget::As));
    assert!(output_refused(&simple), "the ordinary save panel stays refused");
    assert!(!simple_dest.exists());
    simple.state_mut().toast = None;
    simple.get_by_label("名前を付けて保存").click();
    simple.run_steps(4);
    assert!(!output_refused(&simple));
    assert!(matches!(simple.state().dialog, Some(Dialog::SaveCopy)));
    simple.get_by_label("Save").click();
    simple.run_steps(4);
    assert_eq!(std::fs::read(&simple_dest).unwrap(), bytes);
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);

    let detailed_dest = dir.join("detailed.pdf");
    let mut detailed = picker(UiMode::Detailed, original.clone(), detailed_dest.clone(), bytes.clone());
    detailed.get_by_label("Menu").click();
    detailed.run_steps(2);
    detailed.get_by_label("File ⏵").hover();
    detailed.run_steps(3);
    detailed.get_by(|node| node.label().is_some_and(|label| label.starts_with("Save as…"))).click();
    detailed.run_steps(4);
    assert!(!output_refused(&detailed), "the detailed menu must not be stopped by the view-only output notice");
    assert!(matches!(detailed.state().dialog, Some(Dialog::SaveCopy)), "Save as reaches the copy confirmation");
    assert!(!detailed_dest.exists());
    detailed.get_by_label("Save").click();
    detailed.run_steps(4);
    assert_eq!(std::fs::read(&detailed_dest).unwrap(), bytes);
    assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
}
