//! SwiftPDF's 簡単／詳細 interface level: remembered in the settings, chosen with the switch at the
//! right of the tab strip, and changing nothing else. The open tabs, their documents and views
//! stay as they were, and the app stays view-only in both.
//!
//! [`PdfCraftApp::set_view_only`] also sets the process-wide output policy, so these tests live in
//! their own binary and take [`LOCK`] to run one at a time.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfcraft_ui_egui::command_policy::REFUSED;
use pdfcraft_ui_egui::theme::ThemePreference;
use pdfcraft_ui_egui::{PdfCraftApp, UiMode};

static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Two pages that say "Hello mode".
fn pdf() -> Vec<u8> {
    let body = "BT /F1 24 Tf 20 150 Td (Hello mode) Tj ET";
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
        let d = std::env::temp_dir().join(format!("swiftpdf-ui-mode-{test}-{}", std::process::id()));
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

/// A view-only app in `mode`, as the desktop app sets it up before reading the settings.
fn app_in(mode: UiMode) -> PdfCraftApp {
    let mut app = PdfCraftApp::new();
    app.run_inline = true;
    app.set_view_only(true);
    app.set_ui_mode(mode);
    app
}

fn other(mode: UiMode) -> UiMode {
    match mode {
        UiMode::Simple => UiMode::Detailed,
        UiMode::Detailed => UiMode::Simple,
    }
}

fn settings(json: &str) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::from_str(json).unwrap() {
        serde_json::Value::Object(map) => map,
        v => panic!("settings are an object: {v}"),
    }
}

#[test]
fn the_library_starts_detailed_as_pdfcraft_does() {
    let _serial = serial();
    assert_eq!(PdfCraftApp::new().ui_mode(), UiMode::Detailed);
}

#[test]
fn the_mode_is_saved_and_restored() {
    let _serial = serial();
    for mode in [UiMode::Simple, UiMode::Detailed] {
        let saved = app_in(mode).persist();
        assert_eq!(settings(&saved)["ui_mode"], mode.as_str(), "{mode:?}");
        let mut again = app_in(other(mode));
        again.restore(&saved);
        assert_eq!(again.ui_mode(), mode);
    }
    assert_eq!(UiMode::Simple.as_str(), "simple");
    assert_eq!(UiMode::Detailed.as_str(), "detailed");
}

#[test]
fn an_invalid_saved_mode_keeps_the_mode_the_app_started_in() {
    let _serial = serial();
    for start in [UiMode::Simple, UiMode::Detailed] {
        for bad in [r#""edit""#, r#""Simple""#, r#""""#, "5", "true", "null", "[]", r#"{"simple":1}"#] {
            let mut app = app_in(start);
            app.restore(&format!(r#"{{"ui_mode":{bad},"theme":"Dark"}}"#));
            assert_eq!(app.ui_mode(), start, "ui_mode {bad}");
            assert_eq!(app.theme_preference, ThemePreference::Dark, "the other settings are still read beside ui_mode {bad}");
        }
        for broken in ["", "{not json", "[]", r#""simple""#, "null"] {
            let mut app = app_in(start);
            app.restore(broken);
            assert_eq!(app.ui_mode(), start, "settings {broken:?}");
        }
    }
}

#[test]
fn settings_from_before_the_switch_start_simple_and_keep_everything_else() {
    let _serial = serial();
    let dir = Dir::new("migrate");
    let (a, b) = (dir.0.join("a.pdf"), dir.0.join("b.pdf"));
    std::fs::write(&a, pdf()).unwrap();
    std::fs::write(&b, pdf()).unwrap();
    let mut before = app_in(UiMode::Detailed);
    before.reopen_last_session = true;
    before.set_option("theme", "dark").unwrap();
    before.set_option("default-mode", "read").unwrap();
    before.open_path(&a.to_string_lossy());
    before.open_path(&b.to_string_lossy());
    before.set_option("page", "2").unwrap();
    let mut old = settings(&before.persist());
    assert!(old.remove("ui_mode").is_some());
    let old_json = serde_json::Value::Object(old.clone()).to_string();

    // The desktop app's first start with these settings.
    let mut app = app_in(UiMode::Simple);
    app.restore(&old_json);
    assert_eq!(app.ui_mode(), UiMode::Simple, "no saved mode: the app's starting mode");
    app.reopen_last_files(&[]);
    assert_eq!(app.views.len(), 2, "the last session's tabs come back");
    let mut now = settings(&app.persist());
    assert_eq!(now.remove("ui_mode"), Some("simple".into()));
    // Reading the comment styles back changes FreeText's width on its own, with or without a
    // mode (PdfCraft's round trip): compared with a plain round trip instead.
    let mut plain = PdfCraftApp::new();
    plain.restore(&old_json);
    assert_eq!(now.get("comment_styles"), settings(&plain.persist()).get("comment_styles"));
    for (key, value) in old.iter().filter(|(key, _)| *key != "comment_styles") {
        assert_eq!(now.get(key), Some(value), "{key} is unchanged");
    }
    assert_eq!(now.len(), old.len(), "ui_mode is the only new setting");
}

/// A view-only app showing two documents, the second one active, as the desktop app starts.
fn harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_view_only(true);
        app.set_ui_mode(UiMode::Simple);
        app.open_bytes("first.pdf", None, pdf()).unwrap();
        app.open_bytes("second.pdf", None, pdf()).unwrap();
        app
    });
    h.run_steps(4);
    h
}

/// Whether the switch says this segment is the current one (a selected button is "pressed").
fn selected(h: &Harness<'static, PdfCraftApp>, label: &str) -> bool {
    let node = h.get_by_label(label);
    let node = node.accesskit_node();
    node.is_selected() == Some(true) || node.toggled() == Some(egui::accesskit::Toggled::True)
}

/// A tab's page, zoom, rotation, find text and how far down the view its page starts (the two
/// interfaces put different bars round the view, so where it is on screen moves with them).
type ViewState = (usize, f32, u16, Option<String>, Option<i32>);
/// The open documents in tab order, the active tab, and each tab's [`ViewState`].
type AppState = (Vec<pdfcraft_engine::DocId>, Option<usize>, Vec<ViewState>);

/// Everything a mode switch must leave alone.
fn state(h: &Harness<'static, PdfCraftApp>) -> AppState {
    let app = h.state();
    let ids: Vec<_> = app.views.iter().map(|v| v.id).collect();
    for id in &ids {
        assert!(app.session.get(*id).is_some(), "document {id:?} is still open");
    }
    let scrolled = |v: &pdfcraft_ui_egui::canvas::DocView| v.page_screen_rect(v.current).map(|r| (r.top() - v.viewport_rect().top()).round() as i32);
    let views = app.views.iter().map(|v| (v.current, v.zoom, v.rotation, v.find.as_ref().map(|f| f.query.clone()), scrolled(v))).collect();
    (ids, app.active, views)
}

/// [`state`] once the views have finished laying out and the background work (text for find,
/// renders) has landed: unchanged over ten frames, with time between them for the workers.
fn settled(h: &mut Harness<'static, PdfCraftApp>) -> AppState {
    let mut last = state(h);
    let mut same = 0;
    for _ in 0..300 {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let now = state(h);
        same = if now == last { same + 1 } else { 0 };
        if same == 10 {
            return now;
        }
        last = now;
    }
    panic!("the views never settled: {last:?}");
}

#[test]
fn the_switch_changes_only_the_mode() {
    let _serial = serial();
    let mut h = harness();
    // The active tab searching, then at a zoom, turned and on page 2; the other tab as it opened.
    // Find jumps to its first match once the text has loaded, so it has its matches before the jump.
    assert!(h.state_mut().execute("edit.find"));
    h.state_mut().views[1].find.as_mut().expect("the find bar is open").query = "Hello".into();
    let found = |h: &Harness<'static, PdfCraftApp>| h.state().views[1].find.as_ref().is_some_and(|f| f.matches.len() == 2);
    for _ in 0..1000 {
        if found(&h) {
            break;
        }
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(found(&h), "find matched both pages");
    settled(&mut h);
    {
        let view = &mut h.state_mut().views[1];
        view.set_zoom(1.5);
        view.rotate_view(true);
        view.go_to_page(1);
    }
    let before = settled(&mut h);
    assert_eq!(before.2[1].0, 1, "on page 2");
    assert_eq!(before.1, Some(1));
    assert!(selected(&h, "簡単") && !selected(&h, "詳細"));
    h.get_by_label("詳細").click();
    h.run_steps(4);
    assert_eq!(h.state().ui_mode(), UiMode::Detailed);
    assert!(selected(&h, "詳細") && !selected(&h, "簡単"));
    assert_eq!(state(&h), before, "the tabs, documents and views are as they were");

    h.get_by_label("簡単").click();
    h.run_steps(4);
    assert_eq!(h.state().ui_mode(), UiMode::Simple);
    assert_eq!(state(&h), before);
    // Clicking the current mode keeps it.
    h.get_by_label("簡単").click();
    h.run_steps(2);
    assert_eq!(h.state().ui_mode(), UiMode::Simple);
}

#[test]
fn the_switch_works_from_home_and_hides_in_full_screen() {
    let _serial = serial();
    let mut h = harness();
    h.get_by_label("Home").click();
    h.run_steps(3);
    assert_eq!(h.state().active, None);
    h.get_by_label("詳細").click();
    h.run_steps(3);
    assert_eq!(h.state().ui_mode(), UiMode::Detailed);
    assert_eq!(h.state().active, Some(1), "back to the tab that was open before Home");
    assert_eq!(h.state().views.len(), 2, "no tab closed");
    h.get_by_label("Home").click();
    h.run_steps(3);
    assert_eq!(h.state().active, None, "Home in the detailed interface is PdfCraft's");

    h.state_mut().active = Some(0);
    h.state_mut().full_screen = true;
    h.run_steps(3);
    assert!(h.query_by_label("簡単").is_none() && h.query_by_label("詳細").is_none(), "full screen shows only the document");
    h.state_mut().full_screen = false;
    h.run_steps(3);
    assert!(h.query_by_label("簡単").is_some());
}

#[test]
fn a_narrow_window_keeps_the_switch_and_room_for_a_tab() {
    let _serial = serial();
    for width in [600.0, 900.0] {
        let mut h = Harness::builder().with_size(egui::vec2(width, 700.0)).build_eframe(|_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.set_view_only(true);
            for i in 0..12 {
                app.open_bytes(&format!("narrow-{i:02}.pdf"), None, pdf()).unwrap();
            }
            app
        });
        h.run_steps(4);
        let (simple, detailed) = (h.get_by_label("簡単").rect(), h.get_by_label("詳細").rect());
        assert!(simple.right() <= detailed.left() + 0.5, "{width}: 簡単 {simple:?} sits left of 詳細 {detailed:?}");
        assert!(detailed.right() <= width, "{width}: the switch is in the window: {detailed:?}");
        assert!(h.get_by_label_contains("Display theme:").rect().right() <= simple.left(), "{width}: the switch is the rightmost button");
        assert!(h.get_by_label("Open").rect().right() <= simple.left(), "{width}: Open stays left of the switch");
        assert!(h.get_by_label("Discord").rect().right() <= simple.left(), "{width}: Discord stays, as an icon if need be");
        let active = h.get_by_label("narrow-11.pdf").rect();
        let (home, open) = (h.get_by_label("Home").rect(), h.get_by_label("Open").rect());
        assert!(active.left() > home.right() && active.right() <= open.left(), "{width}: the active tab {active:?} is in full view");
        assert!(active.width() > 120.0, "{width}: and not squeezed: {active:?}");
        // The narrowest tab (180) and a scroll arrow either side (24 and a gap of 4 each).
        assert!(open.left() - home.right() >= 236.0, "{width}: the switch leaves the tabs room for one: {home:?} {open:?}");
        h.get_by_label("簡単").click();
        h.run_steps(3);
        assert_eq!(h.state().ui_mode(), UiMode::Simple, "{width}");
    }
}

#[test]
fn both_modes_stay_view_only() {
    let _serial = serial();
    let mut h = harness();
    for label in ["詳細", "簡単", "詳細"] {
        h.get_by_label(label).click();
        h.run_steps(3);
        let app = h.state();
        assert!(app.session.view_only(), "{label}: the engine refuses edits");
        assert!(pdfcraft_ui_egui::output_policy::blocked(), "{label}: no file is written");
        for id in ["file.save", "page.rotate", "print.dialog", "edit.edit_text", "comment.note"] {
            assert!(!app.command_allowed(id), "{label}: {id} is not allowed");
        }
        for id in ["file.save", "page.rotate", "edit.undo"] {
            h.state_mut().toast = None;
            assert!(!h.state_mut().execute(id), "{label}: {id} ran");
            assert_eq!(h.state().toast.as_ref().map(|t| t.0.as_str()), Some(REFUSED), "{label}: {id}");
        }
        let doc = h.state().session.get(h.state().views[1].id).unwrap();
        assert!(!doc.dirty && doc.info.pages.iter().all(|p| p.rotation == 0), "{label}: the document is unchanged");
        let refused = h.state_mut().set_option("dialog", "sign");
        assert!(refused.is_err_and(|e| e.contains("view-only")), "{label}: --dialog sign");
    }
}
