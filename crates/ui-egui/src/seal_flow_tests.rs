//! The window's chairman-seal operations against stores in temp folders: state changes,
//! stamping and its failures, undo and redo records, work off the window's thread, and two
//! windows on one folder. Every store, PDF and seal here is synthetic.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::testing::{COMMITTED, SealDir, chairman_place, connect, fail_writes_after, settle_frames, use_kinds_on_disk};
use crate::seal_service::{SealRef, SealState};
use crate::seal_store_v2::{self as v2, Limits, Point, SealVault, StoreState, UseState};
use crate::{Dialog, PdfCraftApp, QuickTool};

const RECT: [f64; 4] = [20.0, 200.0, 80.0, 260.0];

/// A PDF of `pages` empty pages with a correct cross-reference table.
fn pdf(pages: usize) -> Vec<u8> {
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 3 + i)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {pages} /MediaBox [0 0 200 300] >>", kids.join(" ")),
    ];
    for _ in 0..pages {
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents {} 0 R >>", 3 + pages));
    }
    objects.push("<< /Length 0 >>\nstream\n\nendstream".into());
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

fn png(seed: u8) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(8, 8, |x, y| image::Rgba([200, seed, (x * 20) as u8, if (x + y) % 3 == 0 { 0 } else { 255 }]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn new_app() -> PdfCraftApp {
    let mut app = PdfCraftApp::new();
    app.set_view_only(true);
    app.grant_stamp();
    app
}

/// A window on `dir` with one document of `pages` pages open.
fn app_on(dir: &SealDir, pages: usize) -> PdfCraftApp {
    let mut app = new_app();
    connect(&mut app, dir);
    app.open_bytes("合成文書.pdf", None, pdf(pages)).unwrap();
    app
}

fn toast(app: &PdfCraftApp) -> String {
    app.toast.as_ref().map(|(text, _)| text.clone()).unwrap_or_default()
}

/// Register a seal named `name` through the registration dialog and wait for the store.
fn register(app: &mut PdfCraftApp, name: &str, seed: u8) {
    app.start_chairman_seal(format!("{name}.png"), png(seed));
    app.stamp_draft.name = name.into();
    app.accept_stamp_registration();
    app.seal_settle();
}

fn seal_index(app: &PdfCraftApp, name: &str) -> usize {
    app.seals.seals().iter().position(|seal| seal.name == name).expect("registered")
}

/// Place seal `index` on `page` of the active document and approve it, as the dialog does.
fn place_and_approve(app: &mut PdfCraftApp, index: usize, page: usize) {
    app.quick_tool = QuickTool::ChairmanSeal(index);
    let mut place = chairman_place(app, page, RECT);
    let seal = app.seals.seals()[index].clone();
    place.name = seal.name.clone();
    place.file = seal.file.clone();
    place.data = Arc::new(seal.png().unwrap());
    place.pixels = crate::seal_store::preview_rgba(place.data.as_ref());
    place.source = crate::stamps_ui::PlaceSource::Chairman(index);
    place.seal_ref = Some(SealRef::of(&seal));
    let active = app.active.unwrap();
    app.views[active].stamp_place = Some(place);
    app.ask_stamp_confirm();
    assert!(matches!(app.dialog, Some(Dialog::ConfirmSeal)), "{}", toast(app));
    crate::stamps_ui::approve_current_seal(app).unwrap();
}

/// Press 確定 in the approval dialog. The store answers later.
fn confirm(app: &mut PdfCraftApp) {
    app.commit_stamp_place();
    app.dialog = None;
}

fn stamps(app: &PdfCraftApp, view: usize) -> usize {
    app.session.get(app.views[view].id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count()
}

fn snapshot_on_disk(dir: &SealDir) -> v2::Snapshot {
    match SealVault::new(dir.path()).state().unwrap() {
        StoreState::Ready { snapshot, .. } => *snapshot,
        other => panic!("expected a store, got {other:?}"),
    }
}

/// The op id of the newest use line on disk.
fn last_op(dir: &SealDir) -> String {
    snapshot_on_disk(dir).events.iter().rev().find(|event| event.kind.starts_with("use_")).map(|event| event.op_id.clone()).unwrap()
}

/// Stop only write number `which` (0-based) of `seals.v2.bin`; every other write goes through.
fn fail_write(app: &PdfCraftApp, which: usize) {
    let seen = Arc::new(AtomicUsize::new(0));
    app.seals.vault().unwrap().set_hook(Some(Box::new(move |point| point == Point::BeforeWrite && seen.fetch_add(1, Ordering::SeqCst) == which)));
}

fn general_stamp(page: usize) -> pdfcraft_engine::Edit {
    pdfcraft_engine::Edit::AddCustomStamp {
        page,
        rect: [100.0, 40.0, 160.0, 90.0],
        name: "一般".into(),
        file: pdfcraft_engine::MarkFile { name: "mark.png".into(), bytes: Arc::new(png(9)), page: 0 },
        author: "合成".into(),
        opacity: 1.0,
    }
}

/// A format-1 `seals.bin` with one seal and `lines` history lines, written by the format-1 code.
fn write_format_1(dir: &SealDir, lines: usize) -> crate::seal_store::SealStore {
    let mut store = crate::seal_store::SealStore::load(dir.path().join(v2::LEGACY_FILE));
    store
        .add_seal(crate::seal_store::SealImage {
            name: "合成旧印".into(),
            file: "old.png".into(),
            width: 8,
            height: 8,
            data: png(3),
            scale: 100.0,
            effects: crate::seal_store::SealEffects::off(),
        })
        .unwrap();
    for line in 0..lines {
        store
            .append_event(crate::seal_store::SealEvent::line(format!("2026-10-23 00:00:{line:02} UTC"), "合成.pdf", 1, "合成旧印", "confirmed"))
            .unwrap();
    }
    store
}

/// The data files in `dir`. The lock file, which every reading may create, is left out.
fn names(dir: &SealDir) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != v2::LOCK_FILE)
        .collect();
    names.sort();
    names
}

// --- A. States -------------------------------------------------------------------------------

/// A format-1 store waits for the user. Postponing writes nothing; a failed migration keeps the
/// original file and says why; only "移行する" in the dialog migrates.
#[test]
#[cfg(windows)]
fn a_format_1_store_is_migrated_only_from_the_dialog_and_a_failure_keeps_the_original() {
    use egui_kittest::kittest::Queryable;
    let dir = SealDir::new("migrate-ui");
    drop(write_format_1(&dir, 2));
    let legacy = dir.path().join(v2::LEGACY_FILE);
    let original = std::fs::read(&legacy).unwrap();
    let before = names(&dir);
    let seal_dir = dir.path().to_path_buf();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_cc| {
        let mut app = new_app();
        app.set_option("language", "ja").unwrap();
        app.set_seal_dir(seal_dir);
        app
    });
    settle_frames(&mut h);
    assert_eq!(h.state().seals.state, SealState::NeedsMigration { seals: 1, events: 2, error: None });
    assert!(!h.state().seals.usable());
    assert_eq!(names(&dir), before, "reading the store creates nothing, not even an empty v2 file");
    assert!(!dir.path().join(v2::V2_FILE).exists());
    h.state_mut().start_chairman_seal("x.png".into(), png(1));
    assert!(h.state().dialog.is_none(), "no registration before migration");
    assert!(toast(h.state()).contains("新しい形式へ移すまで"), "{}", toast(h.state()));

    h.state_mut().dialog = Some(Dialog::SealMigrate);
    h.run_steps(3);
    for line in crate::stamps_ui::seal_migration_disclosures(1, 2) {
        h.get_by_label(&line);
    }
    h.get_by_label("延期").click();
    h.run_steps(3);
    assert_eq!(h.state().seals.state, SealState::Deferred { seals: 1, events: 2 });
    assert!(h.state().dialog.is_none());
    h.state_mut().reload_seals();
    settle_frames(&mut h);
    assert_eq!(h.state().seals.state, SealState::Deferred { seals: 1, events: 2 }, "a reload keeps the postponement");
    assert_eq!(names(&dir), before);
    assert_eq!(std::fs::read(&legacy).unwrap(), original, "postponing changes nothing");

    h.state().seals.vault().unwrap().set_hook(Some(Box::new(|point| point == Point::BeforeV2Write)));
    h.state_mut().dialog = Some(Dialog::SealMigrate);
    h.run_steps(3);
    h.get_by_label("移行する").click();
    settle_frames(&mut h);
    match &h.state().seals.state {
        SealState::NeedsMigration { seals: 1, events: 2, error: Some(error) } => {
            assert!(error.contains("移行できませんでした") && error.contains("test: stopped") && error.contains("削除していません"), "{error}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read(&legacy).unwrap(), original, "the original is kept after a failure");
    assert!(!dir.path().join(v2::V2_FILE).exists(), "no store is left half made");

    h.state().seals.vault().unwrap().set_hook(None);
    h.state_mut().dialog = Some(Dialog::SealMigrate);
    h.run_steps(3);
    h.get_by_label("移行する").click();
    settle_frames(&mut h);
    assert_eq!(h.state().seals.state, SealState::Ready, "{}", toast(h.state()));
    assert_eq!(h.state().seals.seals().len(), 1);
    assert!(toast(h.state()).contains("残してあります"), "{}", toast(h.state()));
    let snapshot = snapshot_on_disk(&dir);
    let backup = snapshot.migrated_from.as_ref().unwrap().backup.clone();
    assert_eq!(std::fs::read(dir.path().join(&backup)).unwrap(), original, "the backup is the original, byte for byte");
    assert_eq!(std::fs::read(&legacy).unwrap(), v2::TOMBSTONE);
}

#[test]
fn an_unreadable_store_is_not_used_or_replaced() {
    let dir = SealDir::new("unreadable");
    let path = dir.path().join(v2::V2_FILE);
    std::fs::write(&path, b"SWSEAL2\nnot a store").unwrap();
    let mut app = new_app();
    app.set_seal_dir(dir.path().to_path_buf());
    app.seal_settle();
    assert!(matches!(app.seals.state, SealState::Failed(_)), "{:?}", app.seals.state);
    app.start_chairman_seal("x.png".into(), png(1));
    assert!(app.dialog.is_none());
    app.reload_seals();
    app.seal_settle();
    assert!(matches!(app.seals.state, SealState::Failed(_)));
    assert_eq!(std::fs::read(&path).unwrap(), b"SWSEAL2\nnot a store", "a bad file is left as it was");
}

// --- B. Stamping -----------------------------------------------------------------------------

/// The page changes only after the prepared line is in the store, and every line names one op.
/// While the store works, edits, commands, other seal operations and quitting are refused.
#[test]
fn a_stamp_waits_for_its_prepared_line_and_holds_the_window_until_committed() {
    let dir = SealDir::new("order");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    assert!(app.seals.blocking(), "the approval and preparation are being written");
    assert_eq!(stamps(&app, 0), 0, "nothing is placed before the prepared line");
    assert!(!app.apply_edit(general_stamp(0)), "edits wait");
    assert_eq!(toast(&app), crate::seal_service::BUSY_MESSAGE);
    assert!(!app.execute("edit.undo"));
    app.toast = None;
    assert!(!app.request_quit(&egui::Context::default()), "quitting waits");
    assert!(toast(&app).contains("終了できません"), "{}", toast(&app));
    assert!(!app.allow_quit);
    app.reload_seals();
    assert_eq!(toast(&app), crate::seal_service::BUSY_MESSAGE, "a second seal operation is refused");
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 1, "{}", toast(&app));
    assert!(!app.seals.running());
    let snapshot = snapshot_on_disk(&dir);
    let op = last_op(&dir);
    assert_eq!(snapshot.use_events(&op).map(|event| event.kind.as_str()).collect::<Vec<_>>(), COMMITTED);
    assert_eq!(snapshot.use_state(&op), UseState::Committed(v2::Placement::InDocument));
    let seal = &app.seals.seals()[0];
    for event in snapshot.use_events(&op) {
        assert_eq!(
            (event.seal_id.as_str(), event.seal_revision, event.seal_sha256.as_str()),
            (seal.id.as_str(), seal.revision, seal.sha256.as_str())
        );
    }
    assert_eq!(app.seal_uses.len(), 1);
    assert_eq!(app.seal_uses[0].op_id, op);
    assert!(app.seal_approval.is_none(), "the approval is used once");
    assert!(app.views[0].stamp_place.is_none());
}

/// The answer arrives after the user moved to another tab or page: nothing is placed anywhere,
/// and the prepared attempt is cancelled by its op id.
#[test]
fn a_stamp_whose_tab_or_page_changed_while_preparing_is_cancelled() {
    let dir = SealDir::new("target");
    let mut app = app_on(&dir, 2);
    register(&mut app, "合成印", 1);
    app.open_bytes("別の文書.pdf", None, pdf(2)).unwrap();
    app.active = Some(0);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.active = Some(1);
    app.seal_settle();
    assert_eq!((stamps(&app, 0), stamps(&app, 1)), (0, 0), "{} {}", toast(&app), crate::seal_store_v2::last_store_io_notes());
    assert!(toast(&app).contains("文書か位置が変わった"), "{}", toast(&app));
    let op = last_op(&dir);
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Cancelled);

    app.active = Some(0);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.views[0].stamp_place.as_mut().unwrap().page = 1;
    app.seal_settle();
    assert_eq!((stamps(&app, 0), stamps(&app, 1)), (0, 0));
    let second = last_op(&dir);
    assert_ne!(second, op, "every attempt has its own op id");
    assert_eq!(snapshot_on_disk(&dir).use_state(&second), UseState::Cancelled);
    assert!(snapshot_on_disk(&dir).open_uses().is_empty());
    assert!(app.seal_uses.is_empty());
}

/// A preparation that cannot be written leaves the page alone and no attempt open.
#[test]
fn a_failed_preparation_places_nothing() {
    let dir = SealDir::new("prepare-fail");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    for allowed in [0, 1] {
        fail_writes_after(&app, Some(allowed));
        place_and_approve(&mut app, 0, 0);
        confirm(&mut app);
        app.seal_settle();
        assert_eq!(stamps(&app, 0), 0, "{}", toast(&app));
        assert!(toast(&app).contains("押印しませんでした"), "{}", toast(&app));
        fail_writes_after(&app, None);
        let snapshot = snapshot_on_disk(&dir);
        assert!(snapshot.open_uses().is_empty());
        assert!(!snapshot.events.iter().any(|event| event.kind == "use_committed"));
    }
    assert!(app.seal_uses.is_empty());
}

/// The edit itself fails after the preparation: the attempt is cancelled, not left open.
#[test]
fn a_failed_edit_cancels_the_prepared_attempt() {
    let dir = SealDir::new("edit-fail");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    // A page the document does not have: the engine refuses the edit after preparation.
    place_and_approve(&mut app, 0, 7);
    confirm(&mut app);
    assert!(app.seals.blocking(), "{}", toast(&app));
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 0, "{}", toast(&app));
    assert!(toast(&app).contains("押印は文書に入っていません"), "{}", toast(&app));
    let op = last_op(&dir);
    assert_eq!(
        snapshot_on_disk(&dir).events.iter().filter(|e| e.op_id == op).map(|e| e.kind.as_str()).collect::<Vec<_>>(),
        ["use_approved", "use_prepared", "use_cancelled"]
    );
}

/// The committed line cannot be written: the stamp is taken out again, its attempt cancelled,
/// and redo cannot bring it back. A later edit that reuses its object number is not refused.
#[test]
fn a_failed_commit_takes_the_stamp_back_and_redo_cannot_restore_it() {
    let dir = SealDir::new("commit-fail");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    fail_write(&app, 2);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    fail_writes_after(&app, None);
    assert_eq!(stamps(&app, 0), 0, "{}", toast(&app));
    assert!(toast(&app).contains("押印は記録されていなかったため、取り消しました"), "{}", toast(&app));
    let op = last_op(&dir);
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Cancelled);
    assert!(app.seal_uses.is_empty(), "no committed use is remembered");
    let doc = app.views[0].id;
    assert_eq!(app.session.get(doc).unwrap().can_redo(), Some("Add stamp"));
    let lines = snapshot_on_disk(&dir).events.len();
    app.execute("edit.redo");
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 0, "redo does not restore an unrecorded seal");
    assert!(toast(&app).contains("やり直しで戻せません"), "{}", toast(&app));
    assert_eq!(snapshot_on_disk(&dir).events.len(), lines, "nothing is recorded for it");

    let gone = app.seal_orphans.first().map(|(_, refer)| *refer).unwrap();
    assert!(app.apply_edit(general_stamp(0)), "{}", toast(&app));
    let general = app.session.present_owned_stamps(doc);
    assert_eq!(general, [gone], "precondition: the engine gives the new stamp the freed number");
    assert!(app.execute("edit.undo"));
    assert!(app.execute("edit.redo"), "{}", toast(&app));
    assert_eq!(stamps(&app, 0), 1, "a general stamp is not mistaken for the unrecorded seal: {}", toast(&app));
    assert!(!app.seals.running());
    assert_eq!(snapshot_on_disk(&dir).events.len(), lines);
}

/// Class C (STEP 117J): the replace of `use_committed` can land, and the caller still hears
/// an error (`AfterWrite` stop stands in for a lost verify). Reconciliation reads the same
/// store and `/NM`, keeps the stamp, and does not add a second committed line.
#[test]
fn a_landed_commit_reported_as_error_is_kept_after_reconciliation() {
    let dir = SealDir::new("landed-commit");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    let seen = Arc::new(AtomicUsize::new(0));
    app.seals.vault().unwrap().set_hook(Some(Box::new(move |point| point == Point::AfterWrite && seen.fetch_add(1, Ordering::SeqCst) == 2)));
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    fail_writes_after(&app, None);
    let op = last_op(&dir);
    let snapshot = snapshot_on_disk(&dir);
    assert_eq!(snapshot.use_state(&op), UseState::Committed(v2::Placement::InDocument));
    assert_eq!(stamps(&app, 0), 1, "the stamp stays: {}", toast(&app));
    let kinds: Vec<&str> = snapshot.use_events(&op).map(|event| event.kind.as_str()).collect();
    assert_eq!(kinds, ["use_approved", "use_prepared", "use_committed"]);
    assert!(app.session.get(app.views[0].id).unwrap().output_hold().is_none());
    assert_eq!(app.seal_uses.len(), 1);
    assert!(app.execute("edit.undo"));
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 0);
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Committed(v2::Placement::Undone));
    assert!(app.execute("edit.redo"));
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 1, "{}", toast(&app));
    let after_redo = snapshot_on_disk(&dir);
    let kinds: Vec<&str> = after_redo.use_events(&op).map(|event| event.kind.as_str()).collect();
    assert_eq!(kinds, ["use_approved", "use_prepared", "use_committed", "use_undone", "use_redone"]);
    assert_eq!(kinds.iter().filter(|kind| **kind == "use_undone").count(), 1);
    assert_eq!(kinds.iter().filter(|kind| **kind == "use_redone").count(), 1);
}

/// The committed replace landed, then the file was gone before verify. Reconciliation cannot
/// read the store, so the document is held and the working stamp is not treated as success.
#[test]
fn a_store_that_vanishes_after_commit_holds_the_document() {
    let dir = SealDir::new("reconcile-gone");
    let mut app = app_on(&dir, 1);
    app.grant_save_as();
    register(&mut app, "合成印", 1);
    let v2_path = dir.path().join(v2::V2_FILE);
    let seen = Arc::new(AtomicUsize::new(0));
    app.seals.vault().unwrap().set_hook(Some(Box::new(move |point| {
        if point == Point::AfterWrite && seen.fetch_add(1, Ordering::SeqCst) == 2 {
            let _ = std::fs::remove_file(&v2_path);
        }
        false
    })));
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    fail_writes_after(&app, None);
    let doc = app.views[0].id;
    let reason = app.session.get(doc).unwrap().output_hold().map(str::to_owned).expect("held");
    assert!(reason.starts_with("要復旧") && reason.contains("確認できませんでした"), "{reason}");
    assert_eq!(stamps(&app, 0), 1, "the page is not undone when the outcome is unknown");
    assert!(!app.execute("file.save_as"), "{}", toast(&app));
}

fn stamp_names(app: &PdfCraftApp, view: usize) -> Vec<String> {
    app.session
        .get(app.views[view].id)
        .unwrap()
        .info
        .annotations
        .iter()
        .filter(|annot| annot.subtype == "Stamp")
        .filter_map(|annot| annot.name.clone())
        .collect()
}

/// Each chairman stamp receives its own ISO 32000 `/NM`. The same page, another page, and
/// another document never share that name, and an object number reused after undo gets a new one.
#[test]
fn chairman_stamps_each_receive_a_unique_annotation_name() {
    let dir = SealDir::new("nm-unique");
    let mut app = app_on(&dir, 2);
    register(&mut app, "合成印", 1);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    let first = stamp_names(&app, 0);
    assert_eq!(first.len(), 1, "{}", toast(&app));
    assert!(!first[0].is_empty());

    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    let same_page = stamp_names(&app, 0);
    assert_eq!(same_page.len(), 2, "{}", toast(&app));
    assert_ne!(same_page[0], same_page[1], "two stamps on one page must not share /NM");
    assert_eq!(same_page.iter().collect::<std::collections::BTreeSet<_>>().len(), 2);

    place_and_approve(&mut app, 0, 1);
    confirm(&mut app);
    app.seal_settle();
    let two_pages = stamp_names(&app, 0);
    assert_eq!(two_pages.len(), 3, "{}", toast(&app));
    assert_eq!(two_pages.iter().collect::<std::collections::BTreeSet<_>>().len(), 3, "a stamp on another page must not reuse /NM");

    app.open_bytes("別の文書.pdf", None, pdf(1)).unwrap();
    app.active = Some(1);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    let other_doc = stamp_names(&app, 1);
    assert_eq!(other_doc.len(), 1, "{}", toast(&app));
    assert!(!two_pages.contains(&other_doc[0]), "another document must not reuse this document's /NM");

    let doc = app.views[0].id;
    app.active = Some(0);
    let before_undo = stamp_names(&app, 0);
    let page1 = app.session.get(doc).unwrap().info.annotations.iter().find(|annot| annot.subtype == "Stamp" && annot.page == 1).unwrap();
    let reused = app.session.owned_stamp_ref(doc, 1, page1.index).unwrap();
    assert!(app.execute("edit.undo"));
    app.seal_settle();
    assert!(app.apply_edit(general_stamp(1)), "{}", toast(&app));
    assert!(app.session.present_owned_stamps(doc).contains(&reused), "precondition: the engine reused the object number");
    let general = stamp_names(&app, 0);
    let new_names: Vec<&String> = general.iter().filter(|name| !before_undo.contains(name)).collect();
    assert_eq!(new_names.len(), 1, "a reused object number must carry a new /NM: {general:?} vs {before_undo:?}");
}

/// The committed line is on disk, but the working stamp was taken out before verify. That is
/// not success: the document is held so the store and the page cannot silently disagree.
#[test]
fn a_landed_commit_without_its_stamp_holds_the_document() {
    let dir = SealDir::new("committed-missing-stamp");
    let mut app = app_on(&dir, 1);
    app.grant_save_as();
    register(&mut app, "合成印", 1);
    let release = Arc::new(AtomicBool::new(false));
    let seen_before = Arc::new(AtomicUsize::new(0));
    let seen_after = Arc::new(AtomicUsize::new(0));
    let open = release.clone();
    app.seals.vault().unwrap().set_hook(Some(Box::new(move |point| match point {
        Point::BeforeWrite if seen_before.fetch_add(1, Ordering::SeqCst) == 2 => {
            while !open.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1));
            }
            false
        }
        Point::AfterWrite if seen_after.fetch_add(1, Ordering::SeqCst) == 2 => true,
        _ => false,
    })));
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    until_stamped(&mut app, 0);
    let doc = app.views[0].id;
    app.session.undo(doc).unwrap();
    assert_eq!(stamps(&app, 0), 0, "precondition: the working stamp is gone");
    release.store(true, Ordering::SeqCst);
    app.seal_settle();
    fail_writes_after(&app, None);
    let reason = app.session.get(doc).unwrap().output_hold().map(str::to_owned).expect("held");
    assert!(reason.starts_with("要復旧"), "{reason}");
    assert!(app.seal_uses.is_empty(), "a store line without its annotation is not success");
    assert_eq!(snapshot_on_disk(&dir).use_state(&last_op(&dir)), UseState::Committed(v2::Placement::InDocument));
    assert!(!app.execute("file.save_as"), "{}", toast(&app));
}

/// Reconciliation reads a different store than the one that prepared this stamp: hold, do not
/// treat the working annotation as committed.
#[test]
fn a_store_id_mismatch_after_commit_holds_the_document() {
    let other = SealDir::new("store-id-other");
    let mut other_app = app_on(&other, 1);
    register(&mut other_app, "別印", 2);
    let foreign = std::fs::read(other.path().join(v2::V2_FILE)).unwrap();
    let foreign_id = snapshot_on_disk(&other).store_id;
    drop(other_app);

    let dir = SealDir::new("store-id-mismatch");
    let mut app = app_on(&dir, 1);
    app.grant_save_as();
    register(&mut app, "合成印", 1);
    let ours = snapshot_on_disk(&dir).store_id;
    assert_ne!(ours, foreign_id);
    let v2_path = dir.path().join(v2::V2_FILE);
    let seen = Arc::new(AtomicUsize::new(0));
    app.seals.vault().unwrap().set_hook(Some(Box::new(move |point| {
        if point == Point::AfterWrite && seen.fetch_add(1, Ordering::SeqCst) == 2 {
            std::fs::write(&v2_path, &foreign).unwrap();
        }
        false
    })));
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    fail_writes_after(&app, None);
    let doc = app.views[0].id;
    let reason = app.session.get(doc).unwrap().output_hold().map(str::to_owned).expect("held");
    assert!(reason.starts_with("要復旧") && reason.contains("保管先"), "{reason}");
    assert_eq!(stamps(&app, 0), 1, "the page is not undone when the store is not ours");
    assert!(app.seal_uses.is_empty());
    assert!(!app.execute("file.save_as"), "{}", toast(&app));
}

/// Reconciliation cannot take the store lock: the outcome is unknown, so the document is held.
#[test]
fn a_busy_store_during_reconciliation_holds_the_document() {
    let dir = SealDir::new("reconcile-busy");
    let mut app = app_on(&dir, 1);
    app.grant_save_as();
    register(&mut app, "合成印", 1);
    let limits = Limits { lock_timeout: Duration::from_millis(250), ..Limits::default() };
    app.set_seal_vault(SealVault::with_limits(dir.path(), limits));
    app.seal_settle();
    let landed = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(AtomicUsize::new(0));
    let flag = landed.clone();
    app.seals.vault().unwrap().set_hook(Some(Box::new(move |point| {
        if point == Point::AfterWrite && seen.fetch_add(1, Ordering::SeqCst) == 2 {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    })));
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    let start = Instant::now();
    while !landed.load(Ordering::SeqCst) {
        assert!(start.elapsed() < Duration::from_secs(30), "{}", toast(&app));
        std::thread::sleep(Duration::from_millis(2));
        app.poll_seals();
    }
    // The commit worker still holds the store lock until `store` returns. Take it from this
    // thread before polling the Verify/Io result, so reconciliation is the step that is Busy.
    let held = pdfcraft_platform::lock::lock_with_timeout(&dir.path().join(v2::LOCK_FILE), Duration::from_secs(5), Duration::from_millis(1))
        .expect("the test lock after the landed write");
    app.seal_settle();
    fail_writes_after(&app, None);
    drop(held);
    let doc = app.views[0].id;
    let reason = app.session.get(doc).unwrap().output_hold().map(str::to_owned).expect("held");
    assert!(reason.starts_with("要復旧") && reason.contains("確認できませんでした"), "{reason}");
    assert_eq!(stamps(&app, 0), 1);
    assert!(app.seal_uses.is_empty());
    assert!(!app.execute("file.save_as"), "{}", toast(&app));
}

/// Hold write number `which` (0-based) of `seals.v2.bin` until `release` is set, then stop it
/// when `fail`; every other write goes through. The window sees the step before it as done.
fn hold_write(app: &PdfCraftApp, which: usize, fail: bool) -> Arc<AtomicBool> {
    let release = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(AtomicUsize::new(0));
    let open = release.clone();
    app.seals.vault().unwrap().set_hook(Some(Box::new(move |point| {
        if point != Point::BeforeWrite || seen.fetch_add(1, Ordering::SeqCst) != which {
            return false;
        }
        while !open.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        fail
    })));
    release
}

/// Frames until the stamp is in the document of tab `view` and its commit is still waiting.
fn until_stamped(app: &mut PdfCraftApp, view: usize) {
    let start = Instant::now();
    while stamps(app, view) == 0 {
        assert!(start.elapsed() < Duration::from_secs(30), "{}", toast(app));
        std::thread::sleep(Duration::from_millis(2));
        app.poll_seals();
    }
    assert!(app.seals.blocking(), "the commit is still waiting");
}

/// The committed line fails and the stamp cannot be taken back either, because another change
/// is now on top of it. The document is held: every way to save, copy, print, export or change
/// it is refused with the reason, autosave skips it, and only closing without saving remains.
#[test]
fn a_stamp_that_can_neither_be_recorded_nor_taken_back_holds_the_document() {
    let dir = SealDir::new("hold");
    let mut app = app_on(&dir, 1);
    app.grant_save_as();
    app.grant_print();
    app.grant_edit_text();
    app.grant_page_rotate();
    register(&mut app, "合成印", 1);
    let release = hold_write(&app, 2, true);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    until_stamped(&mut app, 0);
    let doc = app.views[0].id;
    // Past the window's gate, as no user can: the seal is no longer the step undo takes back.
    app.session.apply(doc, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    release.store(true, Ordering::SeqCst);
    app.seal_settle();
    fail_writes_after(&app, None);

    let reason = app.session.get(doc).unwrap().output_hold().map(str::to_owned).expect("the document is held");
    assert!(reason.starts_with("要復旧") && reason.contains("押印を記録できず"), "{reason}");
    assert_eq!(toast(&app), reason, "the user is told why");
    let present = app.session.present_owned_stamps(doc);
    assert_eq!(present.len(), 1, "precondition: the unrecorded seal is still in the working document");
    assert_eq!(app.session.get(doc).unwrap().can_undo(), Some("Rotate page"), "precondition: the rotation is on top");
    let op = last_op(&dir);
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Prepared, "the record stays unfinished, not committed");
    assert!(app.seal_uses.is_empty(), "nothing is remembered as committed");
    let lines = snapshot_on_disk(&dir).events.len();
    let files = names(&dir);

    for command in ["file.save_as", "print.dialog", "edit.undo", "edit.redo", "edit.text", "comment.stamp", "page.rotate", "page.rotate_ccw"] {
        app.toast = None;
        assert!(!app.execute(command), "{command}");
        assert_eq!(toast(&app), reason, "{command}");
        app.run_command(command);
        let spec = pdfcraft_engine::commands::command(command).unwrap();
        assert!(!app.command_enabled(spec), "{command} is greyed out");
    }
    assert!(!app.execute("file.save"), "view-only refuses saving in place on its own");
    assert!(app.dialog.is_none(), "no save or print dialog opened");

    app.toast = None;
    assert!(!app.begin_save_copy());
    assert_eq!(toast(&app), reason);
    assert!(!app.save_active(crate::editing::SaveTarget::As));
    assert!(!app.save_active(crate::editing::SaveTarget::InPlace));
    app.open_print();
    assert_ne!(app.dialog, Some(Dialog::Print));
    assert!(!app.print_now());
    assert!(!app.apply_edit(general_stamp(0)));
    assert!(toast(&app).contains("要復旧"), "{}", toast(&app));

    let settings = pdfcraft_engine::print::Settings::default();
    let refused = [
        app.session.save_copy_bytes(doc).err(),
        app.session.save_edited_copy_bytes(doc).err(),
        app.session.save_bytes(doc).err(),
        app.session.save_full_bytes(doc).err(),
        app.session.print_pdf(doc, &settings).err(),
        app.session.export_data(doc, pdfcraft_engine::DataFormat::Xfdf, true, false).err(),
        app.session.extract(doc, &[0]).err(),
        app.session.reduced_bytes(doc).err(),
        app.session.summarize_comments(doc, pdfcraft_engine::SummarySort::Page).err(),
        app.session.undo(doc).err(),
        app.session.redo(doc).err(),
        app.session.apply(doc, general_stamp(0)).err(),
    ];
    for (i, error) in refused.iter().enumerate() {
        assert_eq!(error.as_ref().map(ToString::to_string).as_deref(), Some(reason.as_str()), "engine call {i}");
    }
    assert!(app.session.autosave_snapshots().iter().all(|snap| snap.doc != doc), "autosave skips it");

    let ctx = egui::Context::default();
    assert!(!app.request_quit(&ctx), "unsaved work is asked about");
    app.resolve_close(&ctx, Some(true));
    assert!(!app.allow_quit, "saving is refused, so the app does not quit");
    assert_eq!(app.views.len(), 1, "the document stays open");
    assert_eq!(toast(&app), reason);
    assert!(app.close_request.is_some(), "the prompt asks again");
    assert_eq!(app.session.present_owned_stamps(doc), present, "no refusal changed the document");
    assert_eq!(snapshot_on_disk(&dir).events.len(), lines, "nothing was recorded as saved or printed");
    assert_eq!(names(&dir), files, "no file was written");

    app.resolve_close(&ctx, Some(false));
    assert!(app.views.is_empty(), "closing without saving is what remains");
    assert!(app.allow_quit);
}

/// A held document says why above its pages, for as long as it is open.
#[test]
fn a_held_document_shows_why_above_its_pages() {
    use egui_kittest::kittest::Queryable;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(|_cc| {
        let mut app = new_app();
        app.set_option("language", "ja").unwrap();
        app.open_bytes("合成文書.pdf", None, pdf(1)).unwrap();
        app
    });
    h.run_steps(3);
    assert!(h.query_by_label_contains("要復旧").is_none());
    let doc = h.state().views[0].id;
    h.state_mut().session.hold_output(doc, "要復旧：合成の理由。".into()).unwrap();
    h.run_steps(3);
    h.get_by_label_contains("要復旧：合成の理由。");
    let spec = pdfcraft_engine::commands::command("file.save_as").unwrap();
    assert!(!h.state().command_enabled(spec));
}

/// The stamp is taken back but its cancellation cannot be written: the document has no seal
/// and is not held, and the attempt stays open on disk, named by its op id.
#[test]
fn a_failed_cancellation_leaves_the_attempt_open_and_says_so() {
    let dir = SealDir::new("cancel-fail");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    fail_writes_after(&app, Some(2));
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    fail_writes_after(&app, None);
    assert_eq!(stamps(&app, 0), 0, "{}", toast(&app));
    assert!(toast(&app).contains("押印の取り消しを記録できませんでした"), "{}", toast(&app));
    let op = last_op(&dir);
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Prepared);
    assert_eq!(snapshot_on_disk(&dir).open_uses(), std::slice::from_ref(&op));
    assert!(toast(&app).contains("未完了の押印準備"), "{}", toast(&app));
    assert!(app.session.get(app.views[0].id).unwrap().output_hold().is_none(), "the document has no seal, so it is not held");
    assert!(app.seal_uses.is_empty());
}

/// The user moves to another tab while the commit is being written: the stamp stays with the
/// document it went into and is recorded for it alone.
#[test]
fn a_tab_switch_while_committing_keeps_the_stamp_with_its_document() {
    let dir = SealDir::new("commit-switch");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    app.open_bytes("別の文書.pdf", None, pdf(1)).unwrap();
    app.active = Some(0);
    let release = hold_write(&app, 2, false);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    until_stamped(&mut app, 0);
    app.active = Some(1);
    release.store(true, Ordering::SeqCst);
    app.seal_settle();
    fail_writes_after(&app, None);
    assert_eq!((stamps(&app, 0), stamps(&app, 1)), (1, 0), "{}", toast(&app));
    let op = last_op(&dir);
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Committed(v2::Placement::InDocument));
    assert_eq!(app.seal_uses.len(), 1);
    assert_eq!(app.seal_uses[0].doc, app.views[0].id);
}

/// A store at its size limit refuses the stamp before the page changes.
#[test]
fn a_full_store_refuses_the_stamp_before_the_page_changes() {
    let dir = SealDir::new("full");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    let size = std::fs::metadata(dir.path().join(v2::V2_FILE)).unwrap().len();
    let limits = Limits { max_file_bytes: size + 64, ..Limits::default() };
    app.set_seal_vault(SealVault::with_limits(dir.path(), limits));
    app.seal_settle();
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 0);
    assert!(toast(&app).contains("上限に近い"), "{}", toast(&app));
    assert!(use_kinds_on_disk(&dir).is_empty());
}

// --- C. Undo, redo and removal ---------------------------------------------------------------

/// Every document step of a chairman stamp adds a line for the same op id, and nothing is
/// removed from the record. A step whose line fails is put back.
#[test]
fn undo_redo_and_removal_add_lines_by_op_id_and_a_failed_line_puts_the_step_back() {
    let dir = SealDir::new("steps");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    let op = last_op(&dir);
    assert!(app.apply_edit(pdfcraft_engine::Edit::DeleteAnnotation { page: 0, index: 0 }), "{}", toast(&app));
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 0);
    for (command, expected) in [("edit.undo", 1), ("edit.redo", 0), ("edit.undo", 1), ("edit.undo", 0), ("edit.redo", 1)] {
        assert!(app.execute(command), "{command}: {}", toast(&app));
        app.seal_settle();
        assert_eq!(stamps(&app, 0), expected, "{command}: {}", toast(&app));
    }
    let snapshot = snapshot_on_disk(&dir);
    let kinds: Vec<&str> = snapshot.use_events(&op).map(|event| event.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "use_approved",
            "use_prepared",
            "use_committed",
            "use_removed",
            "use_removal_undone",
            "use_removed",
            "use_removal_undone",
            "use_undone",
            "use_redone"
        ]
    );
    let steps: Vec<&str> = snapshot.use_events(&op).filter(|event| !event.step_id.is_empty()).map(|event| event.step_id.as_str()).collect();
    let mut unique = steps.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), steps.len(), "each step has its own id");
    assert!(!toast(&app).contains("記録できない"), "{}", toast(&app));

    let lines = snapshot.events.len();
    fail_writes_after(&app, Some(0));
    assert!(app.apply_edit(pdfcraft_engine::Edit::DeleteAnnotation { page: 0, index: 0 }), "{}", toast(&app));
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 1, "a removal whose line failed is put back: {}", toast(&app));
    assert!(toast(&app).contains("使用履歴を記録できないため、文書の変更を元に戻しました"), "{}", toast(&app));
    app.toast = None;
    assert!(app.execute("edit.undo"));
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 1, "an undo whose line failed is put back: {}", toast(&app));
    assert!(toast(&app).contains("使用履歴を記録できないため"), "the reverted step is reported, not announced as done: {}", toast(&app));
    fail_writes_after(&app, None);
    assert_eq!(snapshot_on_disk(&dir).events.len(), lines, "nothing was recorded and nothing removed");
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Committed(v2::Placement::InDocument));
}

/// After an undo the engine may give the seal's object number to the next stamp. A general
/// stamp with that number is not recorded as the seal, and a new chairman stamp gets its own op.
#[test]
fn a_number_freed_by_an_undone_seal_is_not_mistaken_for_it() {
    let dir = SealDir::new("reuse");
    let mut app = app_on(&dir, 1);
    register(&mut app, "合成印", 1);
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    let doc = app.views[0].id;
    let first = last_op(&dir);
    let seal_ref = app.session.present_owned_stamps(doc)[0];
    assert!(app.execute("edit.undo"));
    app.seal_settle();
    assert_eq!(use_kinds_on_disk(&dir).last().map(String::as_str), Some("use_undone"));
    let lines = snapshot_on_disk(&dir).events.len();
    assert!(app.apply_edit(general_stamp(0)), "{}", toast(&app));
    assert_eq!(app.session.present_owned_stamps(doc), [seal_ref], "precondition: the number is reused");
    assert!(app.execute("edit.undo"));
    app.seal_settle();
    assert!(app.execute("edit.redo"));
    app.seal_settle();
    assert_eq!(stamps(&app, 0), 1);
    assert_eq!(snapshot_on_disk(&dir).events.len(), lines, "the general stamp's steps are not seal lines");
    assert_eq!(snapshot_on_disk(&dir).use_state(&first), UseState::Committed(v2::Placement::Undone));

    assert!(app.execute("edit.undo"));
    place_and_approve(&mut app, 0, 0);
    confirm(&mut app);
    app.seal_settle();
    let second = last_op(&dir);
    assert_ne!(second, first);
    assert_eq!(stamps(&app, 0), 1, "{}", toast(&app));
    assert!(app.execute("edit.undo"));
    app.seal_settle();
    let snapshot = snapshot_on_disk(&dir);
    assert_eq!(snapshot.use_state(&second), UseState::Committed(v2::Placement::Undone));
    assert_eq!(snapshot.use_state(&first), UseState::Committed(v2::Placement::Undone), "the earlier attempt is not touched");
}

// --- D. Off the window's thread --------------------------------------------------------------

/// Another process holds the store for longer than the wait. The window keeps drawing, shows
/// the operation, refuses to quit, then reports Busy and keeps what the user typed.
#[test]
fn the_window_keeps_drawing_while_the_store_is_held_and_busy_keeps_the_input() {
    use egui_kittest::kittest::Queryable;
    let dir = SealDir::new("busy");
    let seal_dir = dir.path().to_path_buf();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_cc| {
        let mut app = new_app();
        app.set_option("language", "ja").unwrap();
        app.set_seal_dir(seal_dir);
        app
    });
    settle_frames(&mut h);
    assert!(h.state().seals.usable());
    let held = pdfcraft_platform::lock::lock_with_timeout(&dir.path().join(v2::LOCK_FILE), Duration::from_secs(1), Duration::from_millis(5)).unwrap();
    h.state_mut().start_chairman_seal("合成.png".into(), png(1));
    h.state_mut().stamp_draft.name = "入力した名前".into();
    h.state_mut().accept_stamp_registration();
    let start = Instant::now();
    let mut frames = 0;
    while start.elapsed() < Duration::from_millis(1500) {
        let frame = Instant::now();
        h.run_steps(1);
        assert!(frame.elapsed() < Duration::from_millis(1000), "a frame waited for the store");
        frames += 1;
    }
    assert!(frames >= 5, "{frames} frames");
    assert!(h.state().seals.running(), "the lock is still held");
    h.get_by_label("理事長印を登録しています…");
    let ctx = h.ctx.clone();
    assert!(!h.state_mut().request_quit(&ctx));
    assert!(!h.state().allow_quit);
    settle_frames(&mut h);
    assert!(start.elapsed() >= Duration::from_secs(3), "the store waited its full timeout");
    assert!(toast(h.state()).contains("ほかの SwiftPDF が理事長印のデータを使用中"), "{}", toast(h.state()));
    assert!(matches!(h.state().dialog, Some(Dialog::CreateStamp)), "the registration is offered again");
    assert_eq!(h.state().stamp_draft.name, "入力した名前", "what the user typed is kept");
    assert!(h.state().seals.seals().is_empty());
    drop(held);
    h.state_mut().accept_stamp_registration();
    settle_frames(&mut h);
    assert_eq!(h.state().seals.seals().len(), 1, "an explicit retry succeeds: {}", toast(h.state()));
    assert_eq!(h.state().seals.seals()[0].name, "入力した名前");
}

/// Press the window's × once. Returns whether the window cancelled the close.
fn press_close(h: &mut egui_kittest::Harness<'_, PdfCraftApp>) -> bool {
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
    h.step();
    let cancelled =
        h.output().viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|out| out.commands.contains(&egui::ViewportCommand::CancelClose));
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.clear();
    cancelled
}

fn hold_lock(dir: &SealDir) -> pdfcraft_platform::lock::FileLock {
    pdfcraft_platform::lock::lock_with_timeout(&dir.path().join(v2::LOCK_FILE), Duration::from_secs(1), Duration::from_millis(5)).unwrap()
}

/// The window's × while a seal operation runs, while a stamp waits for its record, with unsaved
/// work, and on a held document: the close is cancelled with the reason, the operation finishes
/// as it would have, and nothing is lost. Reading the store does not hold the window.
#[test]
fn the_close_button_waits_for_seal_work_and_asks_about_unsaved_or_held_documents() {
    let dir = SealDir::new("close-button");
    let seal_dir = dir.path().to_path_buf();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_cc| {
        let mut app = new_app();
        app.grant_save_as();
        app.set_option("language", "ja").unwrap();
        app.set_seal_dir(seal_dir);
        app.open_bytes("合成文書.pdf", None, pdf(1)).unwrap();
        app
    });
    settle_frames(&mut h);

    // Registration waiting for another holder of the store.
    let held = hold_lock(&dir);
    h.state_mut().start_chairman_seal("合成.png".into(), png(1));
    h.state_mut().stamp_draft.name = "合成印".into();
    h.state_mut().accept_stamp_registration();
    h.run_steps(1);
    assert!(h.state().seals.blocking());
    assert!(press_close(&mut h), "closing waits for the registration");
    assert!(toast(h.state()).contains("終了できません"), "{}", toast(h.state()));
    assert!(!h.state().allow_quit);
    drop(held);
    settle_frames(&mut h);
    assert_eq!(h.state().seals.seals().len(), 1, "the registration finished: {}", toast(h.state()));

    // Approval and preparation waiting for the store.
    let held = hold_lock(&dir);
    place_and_approve(h.state_mut(), 0, 0);
    confirm(h.state_mut());
    h.run_steps(1);
    assert!(press_close(&mut h), "closing waits for the preparation");
    assert!(toast(h.state()).contains("終了できません"));
    // Writes from here: the approval, the preparation, then the committed line.
    let release = hold_write(h.state(), 2, false);
    drop(held);

    // The stamp is in the document and its committed line is being written.
    let start = Instant::now();
    while stamps(h.state(), 0) == 0 {
        assert!(start.elapsed() < Duration::from_secs(30), "{}", toast(h.state()));
        std::thread::sleep(Duration::from_millis(2));
        h.run_steps(1);
    }
    assert!(press_close(&mut h), "closing waits for the commit");
    assert!(toast(h.state()).contains("終了できません"));
    release.store(true, Ordering::SeqCst);
    settle_frames(&mut h);
    fail_writes_after(h.state(), None);
    let op = last_op(&dir);
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Committed(v2::Placement::InDocument), "{}", toast(h.state()));
    assert_eq!(stamps(h.state(), 0), 1);

    // The stamped document is unsaved: × asks instead of closing.
    h.state_mut().toast = None;
    assert!(press_close(&mut h), "unsaved work is not dropped");
    assert!(h.state().close_request.is_some(), "the save prompt is up");
    assert!(!h.state().allow_quit);
    let ctx = h.ctx.clone();
    h.state_mut().resolve_close(&ctx, None);
    assert_eq!(h.state().views.len(), 1, "cancelling keeps the document");

    // A held document: × asks, and saving from the prompt is refused with the reason.
    let doc = h.state().views[0].id;
    h.state_mut().session.hold_output(doc, "要復旧：合成の理由。".into()).unwrap();
    assert!(press_close(&mut h));
    h.state_mut().resolve_close(&ctx, Some(true));
    assert_eq!(toast(h.state()), "要復旧：合成の理由。");
    assert!(!h.state().allow_quit, "nothing was saved, so the app does not quit");
    assert_eq!(h.state().views.len(), 1);
    assert_eq!(snapshot_on_disk(&dir).use_state(&op), UseState::Committed(v2::Placement::InDocument), "the record is untouched");
}

/// × while another window holds the store and this one is only reading it: reading changes
/// nothing, so the window may close.
#[test]
fn the_close_button_does_not_wait_for_a_read() {
    let dir = SealDir::new("close-reading");
    drop(write_format_1(&dir, 1));
    let held = hold_lock(&dir);
    let seal_dir = dir.path().to_path_buf();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_cc| {
        let mut app = new_app();
        app.set_seal_dir(seal_dir);
        app
    });
    h.run_steps(1);
    assert!(h.state().seals.running(), "the store is still being read");
    assert!(!h.state().seals.blocking());
    assert!(!press_close(&mut h), "a read does not hold the window");
    drop(held);
}

/// Many events and a nearly full file are shown in the panel; unfinished preparations from
/// another window are listed by op id and left open.
#[test]
#[cfg(windows)]
fn the_panel_shows_size_warnings_and_open_preparations_without_closing_them() {
    use egui_kittest::kittest::Queryable;
    let dir = SealDir::new("panel");
    let other = SealVault::new(dir.path());
    let applied = other.register(v2::NewSeal {
        name: "合成印".into(),
        file: "a.png".into(),
        width: 8,
        height: 8,
        png: png(1),
        scale: 100.0,
        effects: crate::seal_store::SealEffects::off(),
    });
    let seal = applied.unwrap().snapshot.seals[0].clone();
    let approval = v2::ApprovalRef::new(&seal).unwrap();
    other.record_approval(&approval).unwrap();
    let details = v2::UseDetails {
        file: "合成.pdf".into(),
        page: 1,
        image_sha256: seal.sha256.clone(),
        processed: false,
        angle_deg: 0.0,
        fade: 0.0,
        seed: 0,
        rotation_mode: String::new(),
    };
    other.prepare_use(&approval, &details).unwrap();
    let seal_dir = dir.path().to_path_buf();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = new_app();
        app.set_option("language", "ja").unwrap();
        app.set_seal_vault(SealVault::with_limits(seal_dir, Limits { warn_events: 2, warn_file_bytes: 1, ..Limits::default() }));
        app.open_bytes("合成文書.pdf", None, pdf(1)).unwrap();
        app
    });
    settle_frames(&mut h);
    assert!(h.state().seals.usable());
    assert_eq!(h.state().seals.open_uses(), std::slice::from_ref(&approval.op_id));
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(4);
    h.get_by_label(&format!("操作 ID: {}", approval.op_id));
    h.get_by_label_contains("確定も取り消しもされていない押印準備が 1 件");
    h.get_by_label_contains("自動では取り消しません");
    h.get_by_label_contains("使用履歴が");
    h.get_by_label_contains("上限に近づいています");
    h.state_mut().reload_seals();
    settle_frames(&mut h);
    assert_eq!(snapshot_on_disk(&dir).use_state(&approval.op_id), UseState::Prepared, "nothing closes it automatically");
}

// --- E. Two windows on one folder ------------------------------------------------------------

/// Two windows register, change and delete seals in one folder. Neither loses the other's
/// registration; a stale change or deletion is refused with the newest list and the input kept.
#[test]
fn two_windows_keep_each_others_seals_and_a_stale_change_is_refused() {
    let dir = SealDir::new("two");
    let mut a = app_on(&dir, 1);
    let mut b = app_on(&dir, 1);
    register(&mut a, "合成A", 1);
    register(&mut b, "合成B", 2);
    assert_eq!(b.seals.seals().len(), 2, "B's write kept A's seal");
    assert_eq!(snapshot_on_disk(&dir).seals.len(), 2);
    register(&mut a, "合成B", 3);
    assert!(toast(&a).contains("同じ名前"), "{}", toast(&a));
    assert!(matches!(a.dialog, Some(Dialog::CreateStamp)));
    assert_eq!(a.stamp_draft.name, "合成B", "the typed name is kept");
    a.dialog = None;
    assert_eq!(a.seals.seals().len(), 2, "the conflict showed the newest list");

    let index = seal_index(&b, "合成A");
    place_and_approve(&mut b, index, 0);
    b.dialog = None;
    b.views[0].stamp_place.as_mut().unwrap().scale = 80.0;
    let stale = b.views[0].stamp_place.as_ref().unwrap().seal_ref.clone().unwrap();
    let fresh = SealRef::of(&a.seals.seals()[seal_index(&a, "合成A")]);
    a.begin_seal_update(&fresh, v2::SealChange::Scale(120.0), "保存しました");
    a.seal_settle();
    b.save_place_scale();
    b.seal_settle();
    assert!(toast(&b).contains("ほかのウィンドウで変更されました"), "{}", toast(&b));
    assert_eq!(b.views[0].stamp_place.as_ref().unwrap().scale, 80.0, "the placement keeps its input");
    assert_eq!(b.seals.seals()[seal_index(&b, "合成A")].scale, 120.0, "B shows A's change and did not overwrite it");
    assert_eq!(snapshot_on_disk(&dir).seal(&stale.id).unwrap().scale, 120.0);

    b.seal_delete = Some(stale.clone());
    b.delete_selected_seal();
    b.seal_settle();
    assert!(toast(&b).contains("削除しませんでした。最新の内容を表示しました"), "{}", toast(&b));
    assert!(snapshot_on_disk(&dir).seal(&stale.id).is_some(), "a stale deletion deletes nothing");
}

/// An approval made in one window does not stamp a seal another window changed or replaced
/// since. A seal deleted and registered again under the same name is a new seal.
#[test]
fn an_approval_from_before_another_windows_change_does_not_stamp() {
    let dir = SealDir::new("stale-approval");
    let mut a = app_on(&dir, 1);
    let mut b = app_on(&dir, 1);
    register(&mut a, "合成印", 1);
    b.reload_seals();
    b.seal_settle();
    place_and_approve(&mut b, 0, 0);
    let seen = b.views[0].stamp_place.as_ref().unwrap().seal_ref.clone().unwrap();
    a.begin_seal_update(&seen, v2::SealChange::Scale(90.0), "保存しました");
    a.seal_settle();
    confirm(&mut b);
    b.seal_settle();
    assert_eq!(stamps(&b, 0), 0, "{}", toast(&b));
    assert!(toast(&b).contains("押印しませんでした"), "{}", toast(&b));
    assert!(b.seal_approval.is_none(), "a new approval is needed");
    assert!(use_kinds_on_disk(&dir).iter().all(|kind| kind != "use_prepared"), "{:?}", use_kinds_on_disk(&dir));
    crate::stamps_ui::approve_current_seal(&mut b).unwrap();
    confirm(&mut b);
    assert!(!b.seals.running(), "the placement names the old revision and is refused at once");
    assert!(toast(&b).contains("仮置きをやり直してください"), "{}", toast(&b));
    assert_eq!(stamps(&b, 0), 0);

    let current = SealRef::of(&a.seals.seals()[0]);
    a.begin_seal_delete(current.clone());
    a.seal_settle();
    register(&mut a, "合成印", 1);
    let again = a.seals.seals()[0].clone();
    assert_ne!(again.id, current.id, "a seal registered again under the same name is a new seal");
    b.dialog = None;
    b.views[0].stamp_place = None;
    b.quick_tool = QuickTool::ChairmanSeal(0);
    b.views[0].stamp_place = Some(chairman_place(&b, 0, RECT));
    b.ask_stamp_confirm();
    crate::stamps_ui::approve_current_seal(&mut b).unwrap();
    confirm(&mut b);
    b.seal_settle();
    assert_eq!(stamps(&b, 0), 0, "B's list still names the deleted seal: {}", toast(&b));
    assert!(snapshot_on_disk(&dir).open_uses().is_empty());
    assert!(!use_kinds_on_disk(&dir).iter().any(|kind| kind == "use_committed"));
}

/// An older SwiftPDF writes the format-1 file after migration. The next operation sets it
/// aside, records that, and the window says so; nothing from it is merged.
#[test]
#[cfg(windows)]
fn a_late_format_1_write_is_set_aside_recorded_and_announced() {
    let dir = SealDir::new("late-legacy");
    let mut old = write_format_1(&dir, 1);
    let mut app = new_app();
    app.set_seal_dir(dir.path().to_path_buf());
    app.seal_settle();
    app.begin_seal_migration();
    app.seal_settle();
    assert!(app.seals.usable(), "{}", toast(&app));
    let seals_before = snapshot_on_disk(&dir).seals;
    old.add_seal(crate::seal_store::SealImage {
        name: "旧版で追加".into(),
        file: "late.png".into(),
        width: 8,
        height: 8,
        data: png(5),
        scale: 100.0,
        effects: crate::seal_store::SealEffects::off(),
    })
    .unwrap();
    let late = std::fs::read(dir.path().join(v2::LEGACY_FILE)).unwrap();
    register(&mut app, "合成印", 1);
    assert!(toast(&app).contains("以前の SwiftPDF が理事長印の旧ファイルに書き込んでいました"), "{}", toast(&app));
    let snapshot = snapshot_on_disk(&dir);
    let line = snapshot.events.iter().find(|event| event.kind == "legacy_write_set_aside").expect("recorded");
    let backup = line.note.as_ref().unwrap().backup.clone();
    assert_eq!(std::fs::read(dir.path().join(&backup)).unwrap(), late, "kept byte for byte");
    assert_eq!(snapshot.seals.len(), seals_before.len() + 1, "only the new registration; the late seal is not merged");
    assert!(!snapshot.seals.iter().any(|seal| seal.name == "旧版で追加"));
    assert_eq!(std::fs::read(dir.path().join(v2::LEGACY_FILE)).unwrap(), v2::TOMBSTONE);
}

/// A PDF whose pages each show one line of Helvetica text. `rotate` is the page's `/Rotate`.
fn acceptance_pdf(pages: &[(&str, i32)]) -> Vec<u8> {
    let n = pages.len();
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    for (i, (text, rotate)) in pages.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Rotate {rotate} /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * i
        ));
        let mut content = String::from("0.2 0.2 0.6 RG 4 w 40 40 515 762 re S\n");
        content.push_str("BT /F1 28 Tf 60 760 Td (TOP - SwiftPDF synthetic test) Tj ET\n");
        content.push_str(&format!("BT /F1 22 Tf 60 420 Td ({text}) Tj ET\n"));
        content.push_str(&format!("BT /F1 16 Tf 60 70 Td (Page {} of {n} - synthetic data, not a real document) Tj ET\n", i + 1));
        objects.push(format!("<< /Length {} >>\nstream\n{content}endstream", content.len()));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

/// A transparent round seal drawn by the product's own stamp renderer, captioned `top` / `bottom`.
fn acceptance_seal(top: &str, bottom: &str, color: [u8; 3], rules: bool) -> (u32, u32, Vec<u8>) {
    let date = crate::stamp_preset::StampDate { year: 2000, month: 1, day: 1 };
    let overlay = crate::stamp_preset::DateOverlay {
        date,
        entry: date.format(),
        region: crate::stamp_preset::default_region(),
        font: crate::stamp_preset::default_font(),
        color,
        base: Vec::new(),
        drawn: Some(crate::stamp_preset::DrawnDate { top: top.into(), bottom: bottom.into(), rules, ..Default::default() }),
        show_calendar: false,
        calendar_year: 2000,
        calendar_month: 1,
    };
    let (w, h, rgba) = crate::stamp_preset::render_drawn_stamp(&overlay, 400).expect("the Japanese face is in this build (CRAFT_FONTS_DIR)");
    assert!(rgba.chunks_exact(4).any(|px| px[3] == 0), "transparent corners");
    (w, h, crate::stamp_preset::encode_rgba_png(w, h, &rgba).expect("png"))
}

/// Writes the synthetic files for the manual GUI acceptance run (STEP 117E) into
/// `SWIFTPDF_ACCEPTANCE_DIR`, which must be a `step117e` folder under `SwiftPDF-seals-validate`.
/// Existing files are never replaced. Run on its own:
/// `cargo test -p pdfcraft-ui-egui --lib write_gui_acceptance_fixtures -- --ignored`.
#[test]
#[ignore = "writes the manual acceptance fixtures; needs SWIFTPDF_ACCEPTANCE_DIR"]
#[cfg(windows)]
fn write_gui_acceptance_fixtures() {
    let root = std::path::PathBuf::from(std::env::var("SWIFTPDF_ACCEPTANCE_DIR").expect("SWIFTPDF_ACCEPTANCE_DIR"));
    let root = std::fs::canonicalize(&root).expect("the acceptance folder exists");
    let shown = root.to_string_lossy().to_lowercase();
    assert!(
        shown.contains("\\swiftpdf-seals-validate\\") && shown.ends_with("\\step117e"),
        "refusing to write outside a step117e validation folder: {shown}"
    );
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default().to_lowercase();
    assert!(local.is_empty() || !shown.starts_with(&format!("{local}\\swiftpdf")), "refusing the normal app data folder");
    let write_new = |path: std::path::PathBuf, bytes: &[u8]| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        std::io::Write::write_all(&mut file, bytes).unwrap();
    };

    let samples = root.join("samples");
    let pdfs: [(&str, Vec<u8>); 5] = [
        ("normal.pdf", acceptance_pdf(&[("normal.pdf - open, zoom, text, stamps", 0)])),
        ("multi-page.pdf", acceptance_pdf(&[("Page ONE", 0), ("Page TWO", 0), ("Page THREE", 0), ("Page FOUR", 0), ("Page FIVE", 0)])),
        (
            "rotate-test.pdf",
            acceptance_pdf(&[("rotate-test page 1 - the word TOP is at the top", 0), ("rotate-test page 2 - already /Rotate 90", 90)]),
        ),
        ("stamp-test.pdf", acceptance_pdf(&[("stamp-test.pdf - place the test seal below", 0), ("stamp-test page 2", 0)])),
        ("unsaved-test.pdf", acceptance_pdf(&[("unsaved-test.pdf - edit, then close without saving", 0)])),
    ];
    for (name, bytes) in &pdfs {
        let mut app = PdfCraftApp::new();
        app.open_bytes(name, None, bytes.clone()).unwrap();
        let (_, id) = app.active_ids().unwrap();
        assert_eq!(app.session.page_rotate_availability(id), pdfcraft_engine::PageRotateAvailability::Available, "{name}");
        write_new(samples.join(name), bytes);
    }
    let good = acceptance_pdf(&[("repaired-xref.pdf - its xref is wrong, so it opens repaired", 0)]);
    let text = String::from_utf8(good).unwrap();
    let at = text.rfind("startxref\n").unwrap() + "startxref\n".len();
    let end = at + text[at..].find('\n').unwrap();
    let wrong: usize = text[at..end].parse::<usize>().unwrap() + 7;
    let repaired = format!("{}{wrong}{}", &text[..at], &text[end..]).into_bytes();
    let mut app = PdfCraftApp::new();
    app.open_bytes("repaired-xref.pdf", None, repaired.clone()).unwrap();
    let (_, id) = app.active_ids().unwrap();
    assert_eq!(
        app.session.page_rotate_availability(id),
        pdfcraft_engine::PageRotateAvailability::Signature,
        "opens repaired, so rotation is refused"
    );
    write_new(samples.join("repaired-xref.pdf"), &repaired);

    let seals = samples.join("seals");
    let (aw, ah, a) = acceptance_seal("テスト印A", "合成データ", [200, 30, 30], true);
    let (bw, bh, b) = acceptance_seal("テスト印B", "検証用", [20, 90, 170], false);
    let (_, _, c) = acceptance_seal("テスト印C", "削除再登録用", [120, 50, 160], true);
    write_new(seals.join("test-seal-A.png"), &a);
    write_new(seals.join("test-seal-B.png"), &b);
    write_new(seals.join("test-seal-C.png"), &c);
    let (ow, oh, old) = acceptance_seal("旧形式テスト印", "移行試験", [190, 110, 20], true);
    write_new(seals.join("old-format-seal.png"), &old);

    let migration = root.join("migration-test").join("Data");
    std::fs::create_dir_all(&migration).unwrap();
    assert!(!migration.join(v2::LEGACY_FILE).exists() && !migration.join(v2::V2_FILE).exists(), "migration data already prepared");
    let old_seal = crate::seal_store::SealImage {
        name: "旧形式テスト印".into(),
        file: "old-format-seal.png".into(),
        width: ow,
        height: oh,
        data: old.clone(),
        scale: 100.0,
        effects: crate::seal_store::SealEffects::off(),
    };
    let mut store = crate::seal_store::SealStore::load(migration.join(v2::LEGACY_FILE));
    store.add_seal(old_seal.clone()).unwrap();
    for line in 0..3 {
        store
            .append_event(crate::seal_store::SealEvent::line(
                format!("2000-01-01 00:00:0{line} UTC"),
                "合成試験文書.pdf",
                1,
                "旧形式テスト印",
                "confirmed",
            ))
            .unwrap();
    }

    let late_dir = samples.join("late-legacy-write");
    std::fs::create_dir_all(&late_dir).unwrap();
    assert!(!late_dir.join(v2::LEGACY_FILE).exists(), "late write already prepared");
    let mut late = crate::seal_store::SealStore::load(late_dir.join(v2::LEGACY_FILE));
    late.add_seal(old_seal).unwrap();
    late.add_seal(crate::seal_store::SealImage {
        name: "旧版で追加したテスト印".into(),
        file: "test-seal-A.png".into(),
        width: aw,
        height: ah,
        data: a,
        scale: 100.0,
        effects: crate::seal_store::SealEffects::off(),
    })
    .unwrap();
    assert!(bw > 0 && bh > 0);

    let readonly = root.join("output").join("save-fail").join("readonly-target.pdf");
    write_new(readonly.clone(), &pdfs[0].1);
    let mut permissions = std::fs::metadata(&readonly).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&readonly, permissions).unwrap();
}
