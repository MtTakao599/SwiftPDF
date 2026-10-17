//! What a view-only app does at start: crash recovery stays off and leaves an earlier session's
//! snapshots alone, launch options that edit or write are refused, and the recent files and last
//! session come back as before.
//!
//! [`PdfCraftApp::set_view_only`] also sets the process-wide output policy, so these tests live in
//! their own binary and take [`LOCK`] to run one at a time.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use pdfcraft_ui_egui::{Dialog, PdfCraftApp, RecoveryMeta, RecoveryStore};

static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn pdf() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 300 300] >>".into(),
        "<< /Type /Page /Parent 2 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R >>".into(),
    ];
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

/// A folder of its own for one test, removed afterwards.
struct Dir(PathBuf);

impl Dir {
    fn new(test: &str) -> Self {
        let d = std::env::temp_dir().join(format!("swiftpdf-view-only-start-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every file in `dir` with its bytes and modification time, sorted by name.
fn contents(dir: &Path) -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap(), e.metadata().unwrap().modified().unwrap()))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn view_only_app() -> PdfCraftApp {
    let mut app = PdfCraftApp::new();
    app.run_inline = true;
    app.set_view_only(true);
    app
}

#[test]
fn recovery_stays_off_and_leaves_earlier_snapshots_alone() {
    let _serial = serial();
    let dir = Dir::new("recovery");
    let store_dir = dir.0.join("Recovery");
    // A snapshot a crashed (editing) session left behind, written before view-only is on.
    pdfcraft_ui_egui::output_policy::set_blocked(false);
    let meta = RecoveryMeta { key: "1-2-3".into(), name: "crashed.pdf".into(), path: None, saved_at: 1, encrypted: false, instance: None };
    RecoveryStore::new(&store_dir).write(&meta, &pdf()).unwrap();
    let before = contents(&store_dir);
    assert_eq!(before.len(), 2, "the snapshot and its metadata");

    let mut app = view_only_app();
    app.enable_recovery(RecoveryStore::new(&store_dir));
    assert!(app.recovery.is_none(), "autosave is off");
    assert!(app.recoverable.is_empty(), "nothing is offered");
    assert!(!matches!(app.dialog, Some(Dialog::Recovery)), "no recovery dialog");
    // Documents open, time passes, the app shuts down: still nothing written or removed.
    app.open_bytes("doc.pdf", None, pdf()).unwrap();
    app.autosave_now();
    app.discard_recovered(&["1-2-3".to_string()]);
    app.shutdown_recovery();
    assert_eq!(contents(&store_dir), before, "no lock file, no new snapshot, nothing changed or removed");
}

#[test]
fn a_store_that_does_not_exist_is_not_created() {
    let _serial = serial();
    let dir = Dir::new("no-store");
    let store_dir = dir.0.join("Recovery");
    let mut app = view_only_app();
    app.enable_recovery(RecoveryStore::new(&store_dir));
    assert!(!store_dir.exists(), "the recovery folder isn't made");
}

#[test]
fn launch_options_that_edit_or_write_are_refused() {
    let _serial = serial();
    let mut app = view_only_app();
    app.open_bytes("doc.pdf", None, pdf()).unwrap();
    for (key, value) in [
        ("dialog", "sign"),
        ("dialog", "certify"),
        ("dialog", "export-image"),
        ("dialog", "protect"),
        ("dialog", "split"),
        ("dialog", "js-console"),
        ("dialog", "number-pages"),
        ("quick", "note"),
        ("quick", "snapshot"),
        ("organize", "on"),
    ] {
        let refused = app.set_option(key, value);
        assert!(refused.as_ref().is_err_and(|e| e.contains("view-only")), "--{key} {value}: {refused:?}");
        assert!(app.dialog.is_none(), "--{key} {value} opened {:?}", app.dialog);
    }
    assert!(matches!(app.quick_tool, pdfcraft_ui_egui::QuickTool::Select));
    assert!(!app.views[0].organize);
    // Reading options still apply.
    app.set_option("page", "2").unwrap();
    assert_eq!(app.views[0].current, 1);
    app.set_option("zoom", "150").unwrap();
    app.set_option("quick", "hand").unwrap();
    app.set_option("dialog", "properties").unwrap();
    assert!(matches!(app.dialog, Some(Dialog::Properties(_))));
}

#[test]
fn recent_files_and_the_last_session_come_back() {
    let _serial = serial();
    let dir = Dir::new("session");
    let (a, b) = (dir.0.join("a.pdf"), dir.0.join("b.pdf"));
    std::fs::write(&a, pdf()).unwrap();
    std::fs::write(&b, pdf()).unwrap();
    let mut app = view_only_app();
    app.reopen_last_session = true;
    app.open_path(&a.to_string_lossy());
    app.open_path(&b.to_string_lossy());
    assert_eq!(app.views.len(), 2);
    app.set_option("page", "2").unwrap();
    let saved = app.persist();

    let mut again = view_only_app();
    again.restore(&saved);
    let recent: Vec<&str> = again.recent.iter().map(|r| r.name.as_str()).collect();
    assert!(recent.contains(&"a.pdf") && recent.contains(&"b.pdf"), "{recent:?}");
    again.reopen_last_files(&[]);
    assert_eq!(again.views.len(), 2, "both tabs came back");
    let active = again.active.and_then(|i| again.views.get(i)).expect("an active tab");
    let path = again.session.get(active.id).and_then(|d| d.path.clone()).unwrap_or_default();
    assert!(path.ends_with("b.pdf"), "the active tab is b.pdf: {path}");
    assert_eq!(active.current, 1, "at the page it showed");
    assert_eq!(std::fs::read(&a).unwrap(), pdf(), "the files are untouched");
    assert_eq!(std::fs::read(&b).unwrap(), pdf());
}
