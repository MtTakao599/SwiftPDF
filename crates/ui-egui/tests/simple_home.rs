//! SwiftPDF's simple Home and viewer: the eight buttons (開く opens a PDF, the rest only say they
//! are coming), going Home and back without closing anything, returning to the last tab in the
//! detailed interface, and the recent files, which keep the ten newest and keep files that went
//! missing until they are found again or removed.
//!
//! [`PdfCraftApp::set_view_only`] also sets the process-wide output policy, so these tests live in
//! their own binary and take [`LOCK`] to run one at a time.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::command_policy::REFUSED;
use pdfcraft_ui_egui::{PdfCraftApp, QuickTool, RECENT_MAX, RecentFile, UiMode};

static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

const BUTTONS: [&str; 8] = ["開く", "結合", "分割", "編集", "変換", "圧縮", "印刷", "スタンプ"];
const NOT_READY: [&str; 7] = ["結合", "分割", "編集", "変換", "圧縮", "印刷", "スタンプ"];

/// Two pages that say "Hello home".
fn pdf() -> Vec<u8> {
    let body = "BT /F1 24 Tf 20 150 Td (Hello home) Tj ET";
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
        let d = std::env::temp_dir().join(format!("swiftpdf-simple-home-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }

    /// A PDF called `name` in this folder, as a path string.
    fn pdf(&self, name: &str) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, pdf()).unwrap();
        path.to_string_lossy().into_owned()
    }

    /// A path in this folder with nothing there.
    fn gone(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The desktop app in the simple interface, view-only, with `setup` done before the first frame.
fn harness_sized(size: egui::Vec2, setup: impl FnOnce(&mut PdfCraftApp) + Send + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(size).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.run_inline = true;
        app.set_view_only(true);
        app.set_ui_mode(UiMode::Simple);
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

fn harness(setup: impl FnOnce(&mut PdfCraftApp) + Send + 'static) -> Harness<'static, PdfCraftApp> {
    harness_sized(egui::vec2(1400.0, 900.0), setup)
}

fn click(h: &mut Harness<'static, PdfCraftApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(4);
}

/// Click the tab called `name` (highest on screen: a recent row of the same name is below it).
fn click_tab(h: &mut Harness<'static, PdfCraftApp>, name: &str) {
    let tab = h.query_all_by_label(name).min_by(|a, b| a.rect().top().total_cmp(&b.rect().top())).expect("the tab");
    assert!(tab.rect().top() < 60.0, "{name}: a tab: {:?}", tab.rect());
    tab.click();
    h.run_steps(4);
}

/// Click the recent row called `name` (lowest on screen: a tab of the same name is above it).
fn click_row(h: &mut Harness<'static, PdfCraftApp>, name: &str) {
    let row = h.query_all_by_label(name).max_by(|a, b| a.rect().top().total_cmp(&b.rect().top())).expect("the recent row");
    assert!(row.rect().top() > 120.0, "{name}: a recent row, not a tab: {:?}", row.rect());
    row.click();
    h.run_steps(4);
}

fn recent(name: &str, path: &str) -> RecentFile {
    RecentFile { name: name.into(), path: path.into(), pages: 2, size: 100 }
}

fn recent_paths(app: &PdfCraftApp) -> Vec<String> {
    app.recent.iter().map(|r| r.path.clone()).collect()
}

/// The simple Home is showing: its buttons, and none of PdfCraft's Home.
fn at_simple_home(h: &Harness<'static, PdfCraftApp>) -> bool {
    h.state().active.is_none()
        && BUTTONS.iter().all(|b| h.query_by_label(b).is_some())
        && h.query_by_label_contains("ArtCraft community").is_none()
        && h.query_by_label_contains("Recommended tools").is_none()
}

#[test]
fn home_has_the_eight_buttons_in_any_window_and_no_pdfcraft_home() {
    let _serial = serial();
    for size in [egui::vec2(1400.0, 900.0), egui::vec2(760.0, 700.0), egui::vec2(420.0, 640.0)] {
        let h = harness_sized(size, |_| {});
        assert!(at_simple_home(&h), "{size:?}");
        assert!(h.query_by_label("最近使ったPDF").is_some(), "{size:?}: the recent list");
        let cards: Vec<egui::Rect> = BUTTONS.iter().map(|b| h.get_by_label(b).rect()).collect();
        for (i, a) in cards.iter().enumerate() {
            assert!(a.left() >= 0.0 && a.right() <= size.x, "{size:?}: {} fits across: {a:?}", BUTTONS[i]);
            assert!(a.width() >= 80.0 && a.height() >= 80.0, "{size:?}: {} is a real button: {a:?}", BUTTONS[i]);
            for (j, b) in cards.iter().enumerate().skip(i + 1) {
                assert!(!a.shrink(0.5).intersects(b.shrink(0.5)), "{size:?}: {} and {} overlap", BUTTONS[i], BUTTONS[j]);
            }
        }
        // In reading order: left to right, then down.
        for pair in cards.windows(2) {
            assert!(pair[1].top() > pair[0].top() + 1.0 || pair[1].left() > pair[0].left(), "{size:?}: {pair:?}");
        }
    }
}

#[test]
fn buttons_that_are_not_ready_only_say_so() {
    let _serial = serial();
    let dir = Dir::new("not-ready");
    let file = dir.pdf("a.pdf");
    let mut h = harness(move |app| app.open_path(&file));
    click(&mut h, "Home");
    assert!(at_simple_home(&h));
    for label in NOT_READY {
        h.state_mut().toast = None;
        click(&mut h, label);
        let app = h.state();
        assert_eq!(app.toast.as_ref().map(|t| t.0.as_str()), Some(format!("「{label}」は準備中です。").as_str()), "{label}");
        assert_eq!(app.views.len(), 1, "{label}: no tab opened or closed");
        assert_eq!(app.active, None, "{label}: still Home");
        assert!(app.dialog.is_none(), "{label}: no dialog");
        assert_eq!(app.quick_tool, QuickTool::Select, "{label}: no tool picked");
        assert_eq!(app.ui_mode(), UiMode::Simple, "{label}");
        let doc = app.session.get(app.views[0].id).unwrap();
        assert!(!doc.dirty && doc.info.pages.iter().all(|p| p.rotation == 0), "{label}: the document is unchanged");
    }
}

#[test]
fn open_shows_the_picker_and_opening_again_reuses_the_tab() {
    let _serial = serial();
    let dir = Dir::new("open");
    let file = dir.pdf("a.pdf");
    let mut h = harness(|_| {});
    h.state_mut().pick_override = Some(vec![file.clone()]);
    click(&mut h, "開く");
    assert_eq!(h.state().views.len(), 1, "the chosen PDF opened");
    assert_eq!(h.state().active, Some(0));
    assert_eq!(recent_paths(h.state()).first(), Some(&file));
    assert!(h.query_by_label("検索").is_some() && h.query_by_label("文書のプロパティ").is_some(), "the viewer's bar");
    assert!(h.query_by_label("開く").is_none(), "Home is not drawn over the document");

    click(&mut h, "Home");
    assert!(at_simple_home(&h));
    click(&mut h, "開く");
    assert_eq!(h.state().views.len(), 1, "the same PDF comes back in its tab");
    assert_eq!(h.state().active, Some(0));
    click(&mut h, "Home");
    h.state_mut().pick_override = None;
    click_row(&mut h, "a.pdf");
    assert_eq!(h.state().views.len(), 1, "the recent row reuses the tab too");
    assert_eq!(h.state().active, Some(0));
}

#[test]
fn home_and_back_keeps_every_tab_and_its_view() {
    let _serial = serial();
    let dir = Dir::new("home-back");
    let (a, b) = (dir.pdf("a.pdf"), dir.pdf("b.pdf"));
    let mut h = harness(move |app| {
        app.open_path(&a);
        app.open_path(&b);
    });
    let ids: Vec<_> = h.state().views.iter().map(|v| v.id).collect();
    {
        let view = &mut h.state_mut().views[1];
        view.set_zoom(1.5);
        view.rotate_view(true);
        view.go_to_page(1);
    }
    h.run_steps(4);
    click(&mut h, "Home");
    assert!(at_simple_home(&h));
    assert_eq!(h.state().views.iter().map(|v| v.id).collect::<Vec<_>>(), ids, "no tab closed");
    assert!(ids.iter().all(|id| h.state().session.get(*id).is_some()), "the documents stay open");
    // Back through the tab strip.
    click_tab(&mut h, "b.pdf");
    let view = &h.state().views[1];
    assert_eq!(h.state().active, Some(1));
    assert_eq!((view.zoom, view.rotation, view.current), (1.5, 90, 1), "the view is as it was");
    assert_eq!(h.state().views.iter().map(|v| v.id).collect::<Vec<_>>(), ids, "nothing reloaded");
}

#[test]
fn closing_the_last_tab_shows_the_simple_home() {
    let _serial = serial();
    let dir = Dir::new("close-last");
    let (a, b) = (dir.pdf("a.pdf"), dir.pdf("b.pdf"));
    let mut h = harness(move |app| {
        app.open_path(&a);
        app.open_path(&b);
    });
    assert!(h.state_mut().execute("file.close"));
    h.run_steps(4);
    assert_eq!(h.state().active, Some(0), "another tab is left");
    assert!(h.state_mut().execute("file.close"));
    h.run_steps(4);
    assert!(h.state().views.is_empty());
    assert!(at_simple_home(&h), "the simple Home, not PdfCraft's");
    // The detailed interface then has no tab to return to.
    click(&mut h, "詳細");
    assert_eq!(h.state().active, None);
}

#[test]
fn the_detailed_interface_returns_to_the_last_tab_unless_it_closed() {
    let _serial = serial();
    let dir = Dir::new("return");
    let (a, b, c) = (dir.pdf("a.pdf"), dir.pdf("b.pdf"), dir.pdf("c.pdf"));
    let mut h = harness(move |app| {
        app.open_path(&a);
        app.open_path(&b);
        app.open_path(&c);
    });
    click_tab(&mut h, "a.pdf");
    h.state_mut().views[0].set_zoom(2.0);
    h.state_mut().views[0].go_to_page(1);
    h.run_steps(6);
    assert_eq!(h.state().views[0].current, 1);
    let a_id = h.state().views[0].id;
    click(&mut h, "Home");
    // Reordering the tabs at Home doesn't lose it.
    h.state_mut().views.rotate_left(1);
    h.run_steps(2);
    click(&mut h, "詳細");
    let app = h.state();
    let shown = app.active.map(|i| app.views[i].id);
    assert_eq!(shown, Some(a_id), "back to a.pdf, wherever its tab is now");
    let view = app.views.iter().find(|v| v.id == a_id).unwrap();
    assert_eq!((view.zoom, view.current), (2.0, 1), "as it was, not reloaded");
    assert_eq!(app.views.len(), 3);
    h.run_steps(6);
    let view = h.state().views.iter().find(|v| v.id == a_id).unwrap();
    assert_eq!(view.current, 1, "still on page 2 once drawn in the detailed layout");

    // Back to the simple Home, then a.pdf closes there: nothing to return to.
    click(&mut h, "簡単");
    click(&mut h, "Home");
    let at = h.state().views.iter().position(|v| v.id == a_id).unwrap();
    h.state_mut().close_tab(at);
    h.run_steps(3);
    assert_eq!(h.state().active, None, "closing a tab at Home stays at Home");
    click(&mut h, "詳細");
    assert_eq!(h.state().active, None, "PdfCraft's start screen, not another tab");
    assert_eq!(h.state().views.len(), 2);
}

#[test]
fn recent_files_keep_the_ten_newest_and_the_tabs_still_come_back() {
    let _serial = serial();
    let dir = Dir::new("ten");
    let files: Vec<String> = (0..12).map(|i| dir.pdf(&format!("f{i:02}.pdf"))).collect();
    let mut app = PdfCraftApp::new();
    app.run_inline = true;
    app.set_view_only(true);
    app.reopen_last_session = true;
    for f in &files {
        app.open_path(f);
    }
    assert_eq!(app.views.len(), 12);
    let newest: Vec<String> = files.iter().rev().take(RECENT_MAX).cloned().collect();
    assert_eq!(recent_paths(&app), newest, "the ten newest, newest first");
    // Opening one again moves it up without adding it twice.
    app.close_tab(5);
    app.open_recent(&files[5]);
    assert_eq!(recent_paths(&app).first(), Some(&files[5]));
    assert_eq!(app.recent.len(), RECENT_MAX);

    // Twelve saved (an older version, or edited by hand): the ten newest are read.
    app.recent = files.iter().rev().map(|f| recent("x.pdf", f)).collect();
    let saved = app.persist();
    let mut again = PdfCraftApp::new();
    again.run_inline = true;
    again.set_view_only(true);
    again.restore(&saved);
    assert_eq!(recent_paths(&again), newest);
    again.reopen_last_files(&[]);
    assert_eq!(again.views.len(), 12, "every tab of the last session comes back");
}

#[test]
fn missing_files_stay_listed_and_say_so() {
    let _serial = serial();
    let dir = Dir::new("missing");
    let (here, gone) = (dir.pdf("here.pdf"), dir.gone("gone.pdf"));
    let mut saver = PdfCraftApp::new();
    saver.set_ui_mode(UiMode::Simple);
    saver.recent = vec![recent("gone.pdf", &gone), recent("here.pdf", &here)];
    let saved = saver.persist();
    let mut h = harness(move |app| app.restore(&saved));
    assert_eq!(recent_paths(h.state()), vec![gone.clone(), here.clone()], "nothing dropped on start");
    assert!(h.query_by_label("ファイルが見つかりません").is_some());
    assert_eq!(h.query_all_by_label("ファイルが見つかりません").count(), 1, "only the missing one says so");
    assert_eq!(h.query_all_by_label("場所の再指定").count(), 1, "only the missing one offers it");
    assert!(h.query_by_label(&gone).is_some() && h.query_by_label(&here).is_some(), "each row shows its path");
    // A missing file's row opens nothing.
    click_row(&mut h, "gone.pdf");
    assert!(h.state().views.is_empty());
    assert_eq!(recent_paths(h.state()).len(), 2, "and isn't removed by trying");
    click_row(&mut h, "here.pdf");
    assert_eq!(h.state().views.len(), 1);
    assert_eq!(recent_paths(h.state()), vec![here.clone(), gone.clone()], "still listed after another file opens");
}

#[test]
fn relocating_replaces_the_entry_only_once_the_new_file_opens() {
    let _serial = serial();
    let dir = Dir::new("relocate");
    let (gone, other) = (dir.gone("old/report.pdf"), dir.pdf("other.pdf"));
    let broken = dir.0.join("broken.pdf");
    std::fs::write(&broken, b"not a pdf").unwrap();
    let broken = broken.to_string_lossy().into_owned();
    let moved = dir.pdf("report.pdf");
    let mut h = harness({
        let (gone, other) = (gone.clone(), other.clone());
        move |app| app.recent = vec![recent("report.pdf", &gone), recent("other.pdf", &other)]
    });

    // Cancelled: nothing changes.
    h.state_mut().pick_override = Some(vec![]);
    click(&mut h, "場所の再指定");
    assert_eq!(recent_paths(h.state()), vec![gone.clone(), other.clone()]);
    // A file that doesn't open: the entry stays, and is not replaced later either.
    h.state_mut().pick_override = Some(vec![broken.clone()]);
    click(&mut h, "場所の再指定");
    h.run_steps(4);
    assert!(h.state().views.is_empty());
    assert_eq!(recent_paths(h.state()), vec![gone.clone(), other.clone()], "a failed open replaces nothing");
    h.state_mut().pick_override = None;
    click_row(&mut h, "other.pdf");
    click(&mut h, "Home");
    assert_eq!(recent_paths(h.state()), vec![other.clone(), gone.clone()], "the failed choice is forgotten");

    h.state_mut().pick_override = Some(vec![moved.clone()]);
    click(&mut h, "場所の再指定");
    h.run_steps(4);
    assert_eq!(h.state().views.len(), 2, "the file opened");
    assert_eq!(recent_paths(h.state()), vec![moved.clone(), other.clone()], "in place of the missing entry, at the top");
    click(&mut h, "Home");
    assert!(h.query_by_label("ファイルが見つかりません").is_none());
}

#[test]
fn relocating_to_a_file_already_open_shows_its_tab() {
    let _serial = serial();
    let dir = Dir::new("relocate-open");
    let (gone, moved) = (dir.gone("old.pdf"), dir.pdf("moved.pdf"));
    let mut h = harness({
        let (gone, moved) = (gone.clone(), moved.clone());
        move |app| {
            app.open_path(&moved);
            app.recent = vec![recent("old.pdf", &gone), recent("moved.pdf", &moved)];
        }
    });
    click(&mut h, "Home");
    h.state_mut().pick_override = Some(vec![moved.clone()]);
    click(&mut h, "場所の再指定");
    assert_eq!(h.state().views.len(), 1, "not opened twice");
    assert_eq!(h.state().active, Some(0));
    assert_eq!(recent_paths(h.state()), vec![moved]);
}

#[test]
fn removing_an_entry_takes_only_that_one_off_the_list() {
    let _serial = serial();
    let dir = Dir::new("remove");
    let (a, b, gone) = (dir.pdf("a.pdf"), dir.pdf("b.pdf"), dir.gone("gone.pdf"));
    let mut h = harness({
        let (a, b, gone) = (a.clone(), b.clone(), gone.clone());
        move |app| app.recent = vec![recent("a.pdf", &a), recent("gone.pdf", &gone), recent("b.pdf", &b)]
    });
    let remove_in = |h: &mut Harness<'static, PdfCraftApp>, row: &str| {
        let row = h.get_by_label(row).rect();
        let button = h.query_all_by_label("一覧から削除").find(|n| row.contains(n.rect().center())).expect("the row's remove button");
        button.click();
        h.run_steps(4);
    };
    remove_in(&mut h, "b.pdf");
    assert_eq!(recent_paths(h.state()), vec![a.clone(), gone.clone()]);
    assert!(Path::new(&b).is_file(), "the file itself stays");
    remove_in(&mut h, "gone.pdf");
    assert_eq!(recent_paths(h.state()), vec![a.clone()]);
    assert!(h.state().views.is_empty(), "removing opens nothing");
    // The list is saved without them.
    let saved = h.state().persist();
    let mut again = PdfCraftApp::new();
    again.restore(&saved);
    assert_eq!(recent_paths(&again), vec![a]);
}

#[test]
fn the_simple_interface_stays_view_only() {
    let _serial = serial();
    let dir = Dir::new("view-only");
    let file = dir.pdf("a.pdf");
    let mut h = harness(move |app| app.open_path(&file));
    // The viewer's own buttons are viewing ones.
    click(&mut h, "検索");
    assert!(h.state().views[0].find.is_some(), "find opens");
    click(&mut h, "文書のプロパティ");
    assert!(h.state().dialog.is_some(), "properties open");
    h.state_mut().dialog = None;
    h.run_steps(2);
    for at_home in [false, true] {
        if at_home {
            click(&mut h, "Home");
        }
        let app = h.state();
        assert!(app.session.view_only(), "the engine refuses edits");
        assert!(pdfcraft_ui_egui::output_policy::blocked(), "no file is written");
        for id in ["file.save", "file.save_as", "page.rotate", "print.dialog", "edit.edit_text", "comment.note"] {
            assert!(!app.command_allowed(id), "{id} is not allowed");
        }
        for id in ["file.save", "page.rotate", "edit.undo"] {
            h.state_mut().toast = None;
            assert!(!h.state_mut().execute(id), "{id} ran");
            assert_eq!(h.state().toast.as_ref().map(|t| t.0.as_str()), Some(REFUSED), "{id}");
        }
    }
    for label in NOT_READY {
        click(&mut h, label);
        assert_ne!(h.state().toast.as_ref().map(|t| t.0.as_str()), Some(REFUSED), "{label} tried no command");
    }
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    assert!(!doc.dirty && doc.info.pages.iter().all(|p| p.rotation == 0), "the document is unchanged");
}
