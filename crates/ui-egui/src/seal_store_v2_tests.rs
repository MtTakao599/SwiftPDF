use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;
use crate::seal_store::{SealEvent, SealStore};

/// A folder under the OS temp dir, never the user's seal folder. Removed on drop.
struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("swiftpdf-sealv2-test-{}-{name}-{}", std::process::id(), &new_id("t").unwrap()[2..10]));
        assert_isolated(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn v2(&self) -> PathBuf {
        self.0.join(V2_FILE)
    }

    fn legacy(&self) -> PathBuf {
        self.0.join(LEGACY_FILE)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_isolated(dir: &Path) {
    assert!(dir.starts_with(std::env::temp_dir()), "test stores live under the temp dir");
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        assert!(!dir.starts_with(PathBuf::from(local).join("SwiftPDF")), "never the user's seal folder");
    }
    assert!(dir.file_name().unwrap().to_string_lossy().starts_with("swiftpdf-sealv2-test-"));
}

fn png(seed: u8) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(4, 4, |x, y| image::Rgba([seed, x as u8 * 40, y as u8 * 40, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn new_seal(name: &str, seed: u8) -> NewSeal {
    NewSeal { name: name.into(), file: format!("{name}.png"), width: 4, height: 4, png: png(seed), scale: 100.0, effects: SealEffects::off() }
}

fn details() -> UseDetails {
    UseDetails {
        file: "合成文書.pdf".into(),
        page: 1,
        image_sha256: crate::stamps_ui::sha256_hex(&png(1)),
        processed: false,
        angle_deg: 0.0,
        fade: 0.0,
        seed: 0,
        rotation_mode: "manual".into(),
    }
}

fn sha(path: &Path) -> String {
    crate::stamps_ui::sha256_hex(&std::fs::read(path).unwrap())
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

fn ready(vault: &SealVault) -> Snapshot {
    match vault.state().unwrap() {
        StoreState::Ready { snapshot, .. } => *snapshot,
        other => panic!("expected a store, got {other:?}"),
    }
}

fn registered(vault: &SealVault, name: &str, seed: u8) -> Seal {
    let applied = vault.register(new_seal(name, seed)).unwrap();
    let id = applied.created.unwrap();
    applied.snapshot.seal(&id).unwrap().clone()
}

fn approve(vault: &SealVault, seal: &Seal) -> ApprovalRef {
    let approval = ApprovalRef::new(seal).unwrap();
    vault.record_approval(&approval).unwrap();
    approval
}

fn stamp(vault: &SealVault, seal: &Seal) -> ApprovalRef {
    let approval = approve(vault, seal);
    vault.prepare_use(&approval, &details()).unwrap();
    vault.commit_use(&approval.op_id).unwrap();
    approval
}

fn conflict(result: Result<Applied, VaultError>) -> (Conflict, Snapshot) {
    match result {
        Err(VaultError::Conflict { reason, latest }) => (reason, *latest),
        other => panic!("expected a conflict, got {other:?}"),
    }
}

/// Write `snapshot` as the store file without the checks a real write makes.
fn write_raw(dir: &TestDir, snapshot: &Snapshot) {
    std::fs::write(dir.v2(), encode(snapshot).unwrap()).unwrap();
}

fn write_raw_json(dir: &TestDir, json: &serde_json::Value) {
    let plain = serde_json::to_vec(json).unwrap();
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&pdfcraft_platform::dpapi::protect_user(&plain).unwrap());
    std::fs::write(dir.v2(), bytes).unwrap();
}

fn legacy_seal(name: &str, seed: u8) -> SealImage {
    SealImage { name: name.into(), file: format!("{name}.png"), width: 4, height: 4, data: png(seed), scale: 100.0, effects: SealEffects::off() }
}

/// A format-1 `seals.bin` written by the format-1 code itself.
fn write_legacy(dir: &TestDir, seals: &[(&str, u8)], lines: usize) -> SealStore {
    let mut store = SealStore::load(dir.legacy());
    for (name, seed) in seals {
        store.add_seal(legacy_seal(name, *seed)).unwrap();
    }
    for line in 0..lines {
        let event = SealEvent::line(
            format!("2026-10-23 00:{:02}:{:02} UTC", line / 60 % 60, line % 60),
            format!("合成{line}.pdf"),
            1,
            seals[0].0,
            "confirmed",
        );
        store.append_event(event).unwrap();
    }
    store
}

/// Pad the store to `count` events with seal_updated lines from one write.
fn pad_events(dir: &TestDir, vault: &SealVault, seal: &Seal, count: usize) -> Snapshot {
    let mut snapshot = ready(vault);
    snapshot.generation += 1;
    while snapshot.events.len() < count {
        let mut event = new_event(kind::SEAL_UPDATED).unwrap();
        event.seq = snapshot.generation;
        event.seal_id = seal.id.clone();
        event.seal_name = seal.name.clone();
        event.seal_revision = seal.revision;
        event.seal_sha256 = seal.sha256.clone();
        snapshot.events.push(event);
    }
    validate(&snapshot).unwrap();
    write_raw(dir, &snapshot);
    snapshot
}

// --- A. Schema --------------------------------------------------------------------------------

#[test]
fn a_store_round_trips_and_is_ciphertext_on_disk() {
    let dir = TestDir::new("roundtrip");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成理事長印", 1);
    let approval = stamp(&vault, &seal);
    let snapshot = ready(&vault);
    assert_eq!(snapshot.schema, 2);
    assert_eq!(snapshot.generation, 4);
    assert_eq!(
        snapshot.events.iter().map(|event| event.kind.as_str()).collect::<Vec<_>>(),
        [kind::SEAL_REGISTERED, kind::USE_APPROVED, kind::USE_PREPARED, kind::USE_COMMITTED]
    );
    assert_eq!(snapshot.events.iter().map(|event| event.seq).collect::<Vec<_>>(), [1, 2, 3, 4]);
    assert_eq!(snapshot.use_state(&approval.op_id), UseState::Committed(Placement::InDocument));
    assert_eq!(snapshot.seal(&seal.id).unwrap().png().unwrap(), png(1));
    let bytes = std::fs::read(dir.v2()).unwrap();
    assert!(bytes.starts_with(b"SWSEAL2\n"));
    let plain_name = "合成理事長印".as_bytes();
    assert!(!bytes.windows(plain_name.len()).any(|window| window == plain_name), "the name is not on disk in plain text");
    assert!(!bytes.windows(4).any(|window| window == b"IHDR"), "the PNG is not on disk in plain text");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE, "a new store also leaves a tombstone for older versions");
    assert_eq!(names(dir.path()), [LEGACY_FILE, LOCK_FILE, V2_FILE]);
    assert_eq!(std::fs::metadata(dir.path().join(LOCK_FILE)).unwrap().len(), 0);
}

#[test]
fn a_bad_file_is_refused_left_as_it_was_and_not_replaced() {
    let dir = TestDir::new("schema");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let other = registered(&vault, "合成B", 2);
    let approval = stamp(&vault, &seal);
    let good = ready(&vault);
    let mut cases: Vec<(&str, Snapshot)> = Vec::new();
    let mut s = good.clone();
    s.schema = 3;
    cases.push(("unknown schema", s));
    let mut s = good.clone();
    s.store_id.clear();
    cases.push(("empty store id", s));
    let mut s = good.clone();
    s.seals[1].id = s.seals[0].id.clone();
    cases.push(("duplicate seal id", s));
    let mut s = good.clone();
    s.seals[1].name = s.seals[0].name.clone();
    cases.push(("duplicate seal name", s));
    let mut s = good.clone();
    s.seals[0].revision = 0;
    cases.push(("revision 0", s));
    let mut s = good.clone();
    s.seals[0].sha256 = other.sha256.clone();
    cases.push(("sha mismatch", s));
    let mut s = good.clone();
    s.deleted.push(DeletedSeal { id: seal.id.clone(), name: seal.name.clone(), sha256: seal.sha256.clone(), generation: 1 });
    cases.push(("deleted id still present", s));
    let mut s = good.clone();
    s.events[2].id = s.events[1].id.clone();
    cases.push(("duplicate event id", s));
    let mut s = good.clone();
    s.events.swap(1, 2);
    cases.push(("seq goes back", s));
    let mut s = good.clone();
    s.events[4].seq = s.generation + 1;
    cases.push(("seq above generation", s));
    let mut s = good.clone();
    s.events[0].kind = "seal_renamed".into();
    cases.push(("unknown kind", s));
    let mut s = good.clone();
    assert_eq!(s.events[4].kind, kind::USE_COMMITTED);
    let mut cancelled = s.events[4].clone();
    cancelled.id = new_id("ev").unwrap();
    cancelled.kind = kind::USE_CANCELLED.into();
    s.events.push(cancelled);
    cases.push(("cancelled after committed", s));
    let mut s = good.clone();
    assert_eq!(s.events[3].kind, kind::USE_PREPARED);
    s.events.remove(3);
    cases.push(("committed without prepared", s));
    let mut s = good.clone();
    for seed in 3..10 {
        let mut extra = s.seals[0].clone();
        extra.id = new_id("seal").unwrap();
        extra.name = format!("合成{seed}");
        s.seals.push(extra);
    }
    cases.push(("more than 8 seals", s));
    for (case, snapshot) in cases {
        write_raw(&dir, &snapshot);
        let before = std::fs::read(dir.v2()).unwrap();
        assert!(matches!(vault.state(), Err(VaultError::Unreadable(_))), "{case}: refused on read");
        assert!(matches!(vault.register(new_seal("合成C", 3)), Err(VaultError::Unreadable(_))), "{case}: no change applied");
        assert!(matches!(vault.commit_use(&approval.op_id), Err(VaultError::Unreadable(_))), "{case}: no change applied");
        assert_eq!(std::fs::read(dir.v2()).unwrap(), before, "{case}: the file is left byte for byte");
    }
    let mut json = serde_json::to_value(&good).unwrap();
    json.as_object_mut().unwrap().insert("future".into(), serde_json::json!(1));
    write_raw_json(&dir, &json);
    assert!(matches!(vault.state(), Err(VaultError::Unreadable(_))), "an unknown field is refused");
    let mut json = serde_json::to_value(&good).unwrap();
    json["events"][0].as_object_mut().unwrap().insert("extra".into(), serde_json::json!("x"));
    write_raw_json(&dir, &json);
    assert!(matches!(vault.state(), Err(VaultError::Unreadable(_))), "an unknown event field is refused");
    write_raw_json(&dir, &serde_json::json!({"seals": []}));
    assert!(matches!(vault.state(), Err(VaultError::Unreadable(_))), "a format-1 body inside SWSEAL2 is refused");
    write_raw(&dir, &good);
    assert_eq!(ready(&vault), good, "the good file still reads");
}

#[test]
fn damaged_or_foreign_ciphertext_is_refused_and_kept() {
    let dir = TestDir::new("cipher");
    let vault = SealVault::new(dir.path());
    registered(&vault, "合成A", 1);
    let good = std::fs::read(dir.v2()).unwrap();
    let mut flipped = good.clone();
    let middle = flipped.len() / 2;
    flipped[middle] ^= 0x01;
    let mut wrong_magic = good.clone();
    wrong_magic[6] = b'1';
    let foreign = {
        // Another key's ciphertext: a format-1 file's DPAPI blob under the format-2 magic is
        // still this user's, so a random blob stands in for another user's data here. The
        // platform tests check another key directly.
        let mut bytes = MAGIC.to_vec();
        bytes.extend((0..400u32).map(|n| (n * 7 + 3) as u8));
        bytes
    };
    let cases: [(&str, Vec<u8>); 5] = [
        ("one bit flipped", flipped),
        ("wrong magic", wrong_magic),
        ("magic only", MAGIC.to_vec()),
        ("truncated", good.get(..good.len() / 3).unwrap().to_vec()),
        ("not this user's ciphertext", foreign),
    ];
    for (case, bytes) in cases {
        std::fs::write(dir.v2(), &bytes).unwrap();
        assert!(matches!(vault.state(), Err(VaultError::Unreadable(_))), "{case}");
        assert!(matches!(vault.register(new_seal("合成B", 2)), Err(VaultError::Unreadable(_))), "{case}");
        assert_eq!(std::fs::read(dir.v2()).unwrap(), bytes, "{case}: not replaced with an empty store");
        assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE);
    }
}

#[test]
fn a_format_1_file_is_read_for_its_counts_and_not_changed() {
    let dir = TestDir::new("v1read");
    write_legacy(&dir, &[("合成A", 1), ("合成B", 2)], 3);
    let before = sha(&dir.legacy());
    let vault = SealVault::new(dir.path());
    assert_eq!(vault.state().unwrap(), StoreState::NeedsMigration { seals: 2, events: 3 });
    assert_eq!(sha(&dir.legacy()), before);
    assert!(!dir.v2().exists());
}

// --- B. Conflicts -----------------------------------------------------------------------------

#[test]
fn two_stale_instances_both_keep_their_registrations() {
    let dir = TestDir::new("register");
    let first = SealVault::new(dir.path());
    let spelled = dir.path().to_string_lossy().replace('\\', "/").to_uppercase();
    let second = SealVault::new(PathBuf::from(spelled));
    first.register(new_seal("合成A", 1)).unwrap();
    second.register(new_seal("合成B", 2)).unwrap();
    first.register(new_seal("合成C", 3)).unwrap();
    let snapshot = ready(&first);
    assert_eq!(snapshot.seals.iter().map(|seal| seal.name.as_str()).collect::<Vec<_>>(), ["合成A", "合成B", "合成C"]);
    assert_eq!(snapshot.generation, 3);
    let (reason, latest) = conflict(second.register(new_seal("合成A", 4)));
    assert_eq!(reason, Conflict::DuplicateName);
    assert_eq!(latest, snapshot, "the conflict carries the newest store");
    assert_eq!(ready(&first), snapshot, "nothing was written");
}

#[test]
fn a_stale_update_or_delete_is_refused_with_the_newest_store() {
    let dir = TestDir::new("update");
    let window_a = SealVault::new(dir.path());
    let window_b = SealVault::new(dir.path());
    let seal = registered(&window_a, "合成A", 1);
    let changed = window_b.update(&seal.id, 1, SealChange::Scale(150.0)).unwrap();
    assert_eq!(changed.snapshot.seal(&seal.id).unwrap().revision, 2);
    let (reason, latest) = conflict(window_a.update(&seal.id, 1, SealChange::Scale(80.0)));
    assert_eq!(reason, Conflict::SealChanged);
    assert_eq!(latest.seal(&seal.id).unwrap().scale, 150.0, "the other window's change is kept");
    let (reason, _) = conflict(window_a.delete(&seal.id, 1));
    assert_eq!(reason, Conflict::SealChanged);
    let unchanged = window_a.update(&seal.id, 2, SealChange::Scale(150.0)).unwrap();
    assert!(!unchanged.changed, "the same value writes nothing");
    window_b.delete(&seal.id, 2).unwrap();
    let (reason, latest) = conflict(window_a.update(&seal.id, 2, SealChange::Scale(80.0)));
    assert_eq!(reason, Conflict::SealDeleted);
    assert!(latest.seal(&seal.id).is_none(), "a stale window does not bring it back");
    assert_eq!(conflict(window_a.delete(&seal.id, 2)).0, Conflict::SealDeleted);
    let again = registered(&window_a, "合成A", 1);
    assert_ne!(again.id, seal.id, "the same name gets a new id");
    let snapshot = ready(&window_a);
    assert!(snapshot.is_deleted(&seal.id));
    assert_eq!(snapshot.seals.len(), 1);
}

#[test]
fn a_seal_deleted_or_changed_after_approval_is_not_prepared() {
    let dir = TestDir::new("approval");
    let vault = SealVault::new(dir.path());
    let other = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let approval = approve(&vault, &seal);
    other.update(&seal.id, 1, SealChange::Effects(SealEffects::enabled_preset())).unwrap();
    assert_eq!(conflict(vault.prepare_use(&approval, &details())).0, Conflict::SealChanged);
    let seal = ready(&vault).seal(&seal.id).unwrap().clone();
    let approval = approve(&vault, &seal);
    other.delete(&seal.id, seal.revision).unwrap();
    assert_eq!(conflict(vault.prepare_use(&approval, &details())).0, Conflict::SealDeleted);
    let renewed = registered(&other, "合成A", 1);
    assert_eq!(renewed.sha256, seal.sha256, "same image, same name");
    assert_eq!(conflict(vault.prepare_use(&approval, &details())).0, Conflict::SealDeleted, "an old approval does not reach the re-registered seal");
    assert_eq!(conflict(vault.record_approval(&ApprovalRef::new(&seal).unwrap())).0, Conflict::SealDeleted);
    let fresh = approve(&vault, &renewed);
    vault.prepare_use(&fresh, &details()).unwrap();
    let snapshot = ready(&vault);
    assert_eq!(snapshot.use_state(&approval.op_id), UseState::Approved, "the refused attempt has no prepared line");
    assert_eq!(snapshot.use_state(&fresh.op_id), UseState::Prepared);
}

#[test]
fn prepare_needs_a_recorded_matching_approval() {
    let dir = TestDir::new("needapproval");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let second = registered(&vault, "合成B", 2);
    let unrecorded = ApprovalRef::new(&seal).unwrap();
    assert_eq!(conflict(vault.prepare_use(&unrecorded, &details())).0, Conflict::NotApproved);
    let approval = approve(&vault, &seal);
    let mut swapped = approval.clone();
    swapped.seal_id = second.id.clone();
    swapped.seal_sha256 = second.sha256.clone();
    assert_eq!(conflict(vault.prepare_use(&swapped, &details())).0, Conflict::ApprovalMismatch, "one approval does not cover another seal");
    assert_eq!(conflict(vault.record_approval(&swapped)).0, Conflict::OpIdInUse);
    let mut bad = details();
    bad.image_sha256 = "zz".into();
    assert!(matches!(vault.prepare_use(&approval, &bad), Err(VaultError::Rejected(_))));
    let generation = ready(&vault).generation;
    assert_eq!(ready(&vault).use_state(&approval.op_id), UseState::Approved);
    assert_eq!(generation, 3);
}

#[test]
fn a_held_lock_times_out_without_applying_and_is_released_after_each_operation() {
    let dir = TestDir::new("lock");
    let limits = Limits { lock_timeout: Duration::from_millis(300), ..Limits::default() };
    let vault = SealVault::with_limits(dir.path(), limits);
    registered(&vault, "合成A", 1);
    let before = std::fs::read(dir.v2()).unwrap();
    let held = pdfcraft_platform::lock::lock_with_timeout(&dir.path().join(LOCK_FILE), Duration::from_secs(1), Duration::from_millis(5)).unwrap();
    let start = Instant::now();
    assert_eq!(vault.register(new_seal("合成B", 2)), Err(VaultError::Busy));
    assert!(start.elapsed() >= Duration::from_millis(300) && start.elapsed() < Duration::from_secs(3));
    assert_eq!(vault.state(), Err(VaultError::Busy));
    assert_eq!(std::fs::read(dir.v2()).unwrap(), before, "nothing was applied");
    drop(held);
    vault.register(new_seal("合成B", 2)).unwrap();
    let free = pdfcraft_platform::lock::lock_with_timeout(&dir.path().join(LOCK_FILE), Duration::ZERO, Duration::from_millis(1));
    assert!(free.is_ok(), "the operation released the lock");
    drop(free);
    assert!(matches!(vault.update("seal-00000000000000000000000000000000", 1, SealChange::Scale(90.0)), Err(VaultError::Conflict { .. })));
    assert!(
        pdfcraft_platform::lock::lock_with_timeout(&dir.path().join(LOCK_FILE), Duration::ZERO, Duration::from_millis(1)).is_ok(),
        "a refused operation released it too"
    );
}

#[cfg(windows)]
#[test]
fn spellings_and_a_junction_reach_the_same_lock() {
    let dir = TestDir::new("junction");
    let real = dir.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    let link = dir.path().join("link");
    let status = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(&real).output().unwrap();
    assert!(status.status.success(), "mklink /J failed");
    let limits = Limits { lock_timeout: Duration::from_millis(150), ..Limits::default() };
    let held = pdfcraft_platform::lock::lock_with_timeout(&real.join(LOCK_FILE), Duration::from_secs(1), Duration::from_millis(5)).unwrap();
    let upper = PathBuf::from(real.to_string_lossy().to_uppercase());
    let slashes = PathBuf::from(real.to_string_lossy().replace('\\', "/"));
    let dotted = real.join(".").join("..").join("real");
    for spelling in [link.clone(), upper, slashes, dotted] {
        let vault = SealVault::with_limits(&spelling, limits);
        assert_eq!(vault.register(new_seal("合成A", 1)), Err(VaultError::Busy), "{}", spelling.display());
    }
    drop(held);
    let through_link = SealVault::with_limits(&link, limits);
    through_link.register(new_seal("合成A", 1)).unwrap();
    assert_eq!(ready(&SealVault::new(&real)).seals.len(), 1, "both names are one store");
    let _ = std::fs::remove_dir(&link);
}

// --- C. Audit ---------------------------------------------------------------------------------

#[test]
fn every_event_is_kept_past_400_and_401_and_10000() {
    let dir = TestDir::new("count");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let first_id = ready(&vault).events[0].id.clone();
    pad_events(&dir, &vault, &seal, 399);
    approve(&vault, &seal);
    let at_400 = ready(&vault);
    assert_eq!(at_400.events.len(), 400);
    approve(&vault, &seal);
    let at_401 = ready(&vault);
    assert_eq!(at_401.events.len(), 401);
    assert_eq!(at_401.events.first().unwrap().id, first_id, "the oldest line is kept");
    assert_eq!(at_401.events.get(..400).unwrap(), at_400.events.as_slice(), "earlier lines are unchanged");
    pad_events(&dir, &vault, &seal, 10_000);
    let applied = vault.record_approval(&ApprovalRef::new(&seal).unwrap()).unwrap();
    assert_eq!(applied.snapshot.events.len(), 10_001);
    assert!(applied.notices.contains(&Notice::ManyEvents { events: 10_001 }), "a warning, not a cap");
    assert_eq!(ready(&vault).events.first().unwrap().id, first_id);
    assert!(matches!(vault.state().unwrap(), StoreState::Ready { notices, .. } if notices.contains(&Notice::ManyEvents { events: 10_001 })));
}

#[test]
fn use_lines_follow_their_states_and_retries_record_once() {
    let dir = TestDir::new("states");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let committed = approve(&vault, &seal);
    assert!(!vault.record_approval(&committed).unwrap().changed, "a retried approval writes nothing");
    vault.prepare_use(&committed, &details()).unwrap();
    assert!(!vault.prepare_use(&committed, &details()).unwrap().changed, "a retried prepare writes nothing");
    let mut other_details = details();
    other_details.page = 2;
    assert_eq!(conflict(vault.prepare_use(&committed, &other_details)).0, Conflict::OpIdInUse);
    assert!(vault.commit_use(&committed.op_id).unwrap().changed);
    assert!(!vault.commit_use(&committed.op_id).unwrap().changed, "a double commit is one line");
    assert_eq!(conflict(vault.cancel_use(&committed.op_id)).0, Conflict::UseState(UseState::Committed(Placement::InDocument)));
    let cancelled = approve(&vault, &seal);
    vault.prepare_use(&cancelled, &details()).unwrap();
    vault.cancel_use(&cancelled.op_id).unwrap();
    assert!(!vault.cancel_use(&cancelled.op_id).unwrap().changed);
    assert_eq!(conflict(vault.commit_use(&cancelled.op_id)).0, Conflict::UseState(UseState::Cancelled), "cancelled is never committed");
    let unprepared = approve(&vault, &seal);
    assert_eq!(conflict(vault.commit_use(&unprepared.op_id)).0, Conflict::UseState(UseState::Approved));
    assert_eq!(conflict(vault.cancel_use("op-ffffffffffffffffffffffffffffffff")).0, Conflict::UseState(UseState::Unknown));
    let step = new_id("step").unwrap();
    vault.note_use_step(&committed.op_id, &step, UseStep::Undone).unwrap();
    assert!(!vault.note_use_step(&committed.op_id, &step, UseStep::Undone).unwrap().changed);
    assert_eq!(conflict(vault.note_use_step(&committed.op_id, &step, UseStep::Redone)).0, Conflict::StepIdReused);
    assert_eq!(
        conflict(vault.note_use_step(&committed.op_id, &new_id("step").unwrap(), UseStep::Removed)).0,
        Conflict::UseState(UseState::Committed(Placement::Undone))
    );
    vault.note_use_step(&committed.op_id, &new_id("step").unwrap(), UseStep::Redone).unwrap();
    vault.note_use_step(&committed.op_id, &new_id("step").unwrap(), UseStep::Removed).unwrap();
    vault.note_use_step(&committed.op_id, &new_id("step").unwrap(), UseStep::RemovalUndone).unwrap();
    assert_eq!(conflict(vault.note_use_step(&cancelled.op_id, &new_id("step").unwrap(), UseStep::Undone)).0, Conflict::UseState(UseState::Cancelled));
    let snapshot = ready(&vault);
    let kinds = |op: &str| snapshot.use_events(op).map(|event| event.kind.clone()).collect::<Vec<_>>();
    assert_eq!(
        kinds(&committed.op_id),
        [
            kind::USE_APPROVED,
            kind::USE_PREPARED,
            kind::USE_COMMITTED,
            kind::USE_UNDONE,
            kind::USE_REDONE,
            kind::USE_REMOVED,
            kind::USE_REMOVAL_UNDONE
        ]
    );
    assert_eq!(kinds(&cancelled.op_id), [kind::USE_APPROVED, kind::USE_PREPARED, kind::USE_CANCELLED]);
    assert_eq!(snapshot.use_state(&committed.op_id), UseState::Committed(Placement::InDocument));
    assert!(snapshot.open_uses().is_empty());
    let committed_line = snapshot.use_events(&committed.op_id).nth(2).unwrap();
    assert_eq!(committed_line.image_sha256, details().image_sha256);
    assert_eq!(committed_line.file, "合成文書.pdf");
    let seqs: Vec<u64> = snapshot.events.iter().map(|event| event.seq).collect();
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]), "one write, one line, in order");
    assert_eq!(*seqs.last().unwrap(), snapshot.generation);
}

#[test]
fn cancelling_one_attempt_keeps_every_other_line() {
    let dir = TestDir::new("keep");
    let vault = SealVault::new(dir.path());
    let other = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let a = stamp(&vault, &seal);
    let b = approve(&other, &seal);
    other.prepare_use(&b, &details()).unwrap();
    let before = ready(&vault);
    other.cancel_use(&b.op_id).unwrap();
    let after = ready(&vault);
    assert_eq!(after.events.get(..before.events.len()).unwrap(), before.events.as_slice(), "nothing earlier is removed or changed");
    assert_eq!(after.events.len(), before.events.len() + 1);
    assert_eq!(after.use_state(&a.op_id), UseState::Committed(Placement::InDocument));
}

#[test]
fn near_the_limit_a_prepare_is_refused_but_every_open_use_can_still_finish() {
    let dir = TestDir::new("limit");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let base = std::fs::metadata(dir.v2()).unwrap().len();
    let limits = Limits { max_file_bytes: base + (24 << 10), step_headroom: 2 << 10, reserve_per_open_use: 4 << 10, ..Limits::default() };
    let tight = SealVault::with_limits(dir.path(), limits);
    let mut open = Vec::new();
    let refused = loop {
        let approval = ApprovalRef::new(&seal).unwrap();
        match tight.record_approval(&approval) {
            Ok(_) => {}
            Err(error) => break error,
        }
        match tight.prepare_use(&approval, &details()) {
            Ok(_) => open.push(approval),
            Err(error) => break error,
        }
        assert!(open.len() < 50, "the limit is reached");
    };
    assert_eq!(refused, VaultError::Full { limit: limits.max_file_bytes });
    assert!(!open.is_empty(), "some stamps were prepared before the limit");
    let file_before = std::fs::read(dir.v2()).unwrap();
    let last = ApprovalRef::new(&seal).unwrap();
    if tight.record_approval(&last).is_ok() {
        assert!(matches!(tight.prepare_use(&last, &details()), Err(VaultError::Full { .. })));
    } else {
        assert_eq!(std::fs::read(dir.v2()).unwrap(), file_before, "a refused request changes nothing");
    }
    let kept = limits.reserve_per_open_use * open.len() as u64 + limits.step_headroom;
    assert!(std::fs::metadata(dir.v2()).unwrap().len() <= limits.max_file_bytes - kept, "general writes leave the room kept for open stamps");
    for (n, approval) in open.iter().enumerate() {
        let result = if n % 2 == 0 { tight.commit_use(&approval.op_id) } else { tight.cancel_use(&approval.op_id) };
        assert!(result.is_ok(), "open stamp {n} could not be finished: {result:?}");
        assert!(std::fs::metadata(dir.v2()).unwrap().len() <= limits.max_file_bytes);
    }
    let snapshot = ready(&tight);
    assert!(snapshot.open_uses().is_empty(), "every prepared stamp has its final line");
    let committed = open.iter().step_by(2).next().unwrap();
    let mut steps = 0;
    loop {
        let step = if steps % 2 == 0 { UseStep::Undone } else { UseStep::Redone };
        match tight.note_use_step(&committed.op_id, &new_id("step").unwrap(), step) {
            Ok(_) => steps += 1,
            Err(VaultError::Full { .. }) => break,
            Err(error) => panic!("{error:?}"),
        }
        assert!(std::fs::metadata(dir.v2()).unwrap().len() <= limits.max_file_bytes);
        assert!(steps < 200);
    }
    assert!(steps > 0, "undo and redo lines may use the headroom new stamps may not");
    assert_eq!(ready(&tight).events.len(), snapshot.events.len() + steps, "nothing was dropped to make room");
}

fn filler(snapshot: &Snapshot, seal: &Seal, file_chars: usize) -> Event {
    let mut event = new_event(kind::SEAL_UPDATED).unwrap();
    event.seq = snapshot.generation;
    event.seal_id = seal.id.clone();
    event.seal_name = seal.name.clone();
    event.seal_revision = seal.revision;
    event.seal_sha256 = seal.sha256.clone();
    event.file = "x".repeat(file_chars);
    event
}

/// Build a store of `seals` large seals and fillers until it has at least `events` events and
/// its file is within 1.5 KB of `target` bytes, in one write.
fn build_store(dir: &TestDir, vault: &SealVault, seals: usize, events: usize, target: u64) -> (Seal, Snapshot) {
    let seal = registered(vault, "合成A", 1);
    let mut snapshot = ready(vault);
    snapshot.generation += 1;
    let mut random = 0x9e37_79b9_7f4a_7c15u64;
    for n in 0..seals {
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        while data.len() < MAX_SEAL_BYTES {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            data.extend_from_slice(&random.to_le_bytes());
        }
        data.truncate(MAX_SEAL_BYTES);
        snapshot.seals.push(Seal {
            id: new_id("seal").unwrap(),
            revision: 1,
            name: format!("合成大{n}"),
            file: "synthetic.png".into(),
            width: 4,
            height: 4,
            data: base64::engine::general_purpose::STANDARD.encode(&data),
            sha256: crate::stamps_ui::sha256_hex(&data),
            scale: 100.0,
            effects: SealEffects::off(),
        });
    }
    while snapshot.events.len() < events {
        snapshot.events.push(filler(&snapshot, &seal, 40));
    }
    let per_event = serde_json::to_vec(&filler(&snapshot, &seal, 240)).unwrap().len() as i64 + 1;
    let mut bytes = encode(&snapshot).unwrap();
    while (target as i64) - (bytes.len() as i64) > 1500 {
        let whole = (((target as i64) - (bytes.len() as i64) - 700) / per_event).max(1);
        for _ in 0..whole {
            snapshot.events.push(filler(&snapshot, &seal, 240));
        }
        bytes = encode(&snapshot).unwrap();
    }
    validate(&snapshot).unwrap();
    std::fs::write(dir.v2(), &bytes).unwrap();
    (seal, snapshot)
}

/// Times of the real store at 400 and 10,000 events and at about 48 and 63 MiB: a load, one
/// recorded operation, and an operation that waits for another holder of the lock. Every
/// event is kept; nothing is shortened to make it faster.
#[test]
#[ignore = "builds stores of up to 63 MiB and prints timings; run with --release --ignored --nocapture"]
fn store_timings_at_400_and_10000_events_and_48_and_63_mib() {
    let limits = Limits::default();
    let cases: [(&str, usize, usize, u64); 4] = [
        ("400 events", 0, 400, 0),
        ("10,000 events", 0, 10_000, 0),
        ("about 48 MiB", 5, 10_000, 48 << 20),
        ("about 63 MiB", 7, 10_000, (63 << 20).min(limits.max_file_bytes - limits.step_headroom - limits.reserve_per_open_use - 8192)),
    ];
    for (label, seals, events, target) in cases {
        let dir = TestDir::new("timings");
        let vault = SealVault::new(dir.path());
        let (seal, built) = build_store(&dir, &vault, seals, events, target);
        let size = std::fs::metadata(dir.v2()).unwrap().len();
        let ms = |start: Instant| start.elapsed().as_millis();
        let mut load = Vec::new();
        for _ in 0..3 {
            let start = Instant::now();
            let snapshot = ready(&vault);
            load.push(ms(start));
            assert_eq!(snapshot.events.len(), built.events.len(), "every event is read");
        }
        let mut operation = Vec::new();
        for _ in 0..3 {
            let approval = ApprovalRef::new(&seal).unwrap();
            let start = Instant::now();
            vault.record_approval(&approval).unwrap();
            operation.push(ms(start));
        }
        let held = pdfcraft_platform::lock::lock_with_timeout(&dir.path().join(LOCK_FILE), Duration::from_secs(1), Duration::from_millis(5)).unwrap();
        let other = SealVault::new(dir.path());
        let approval = ApprovalRef::new(&seal).unwrap();
        let start = Instant::now();
        let waiting = std::thread::spawn(move || other.record_approval(&approval).map(|_| ()));
        std::thread::sleep(Duration::from_millis(1000));
        drop(held);
        waiting.join().unwrap().unwrap();
        let waited = ms(start);
        let finished = ready(&vault);
        assert_eq!(finished.events.len(), built.events.len() + 4, "every line kept");
        println!(
            "TIMING {label}: {} events, {size} bytes ({:.2} MiB); load {load:?} ms; one operation {operation:?} ms; with the lock held 1000 ms: {waited} ms",
            finished.events.len(),
            size as f64 / 1_048_576.0
        );
    }
}

#[test]
#[ignore = "builds a real 64 MiB store; run with --ignored"]
fn a_64_mib_store_refuses_a_stamp_it_could_not_finish_and_finishes_every_open_one() {
    let dir = TestDir::new("64mib");
    let limits = Limits::default();
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let open = approve(&vault, &seal);
    vault.prepare_use(&open, &details()).unwrap();
    let mut snapshot = ready(&vault);
    snapshot.generation += 1;
    let mut random = 0x9e37_79b9_7f4a_7c15u64;
    for n in 0..7 {
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        while data.len() < MAX_SEAL_BYTES {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            data.extend_from_slice(&random.to_le_bytes());
        }
        data.truncate(MAX_SEAL_BYTES);
        snapshot.seals.push(Seal {
            id: new_id("seal").unwrap(),
            revision: 1,
            name: format!("合成大{n}"),
            file: "synthetic.png".into(),
            width: 4,
            height: 4,
            data: base64::engine::general_purpose::STANDARD.encode(&data),
            sha256: crate::stamps_ui::sha256_hex(&data),
            scale: 100.0,
            effects: SealEffects::off(),
        });
    }
    let fixed = snapshot.events.len();
    let per_event = serde_json::to_vec(&filler(&snapshot, &seal, 240)).unwrap().len() as i64 + 1;
    let approval_room = 600;
    let target = (limits.max_file_bytes - limits.step_headroom - limits.reserve_per_open_use) as i64 - 4096 - approval_room;
    let mut bytes = encode(&snapshot).unwrap();
    for _ in 0..12 {
        let gap = target - bytes.len() as i64;
        if (0..=1500).contains(&gap) {
            break;
        }
        if gap > 1500 {
            let whole = ((gap - 700) / per_event).max(0);
            for _ in 0..whole {
                snapshot.events.push(filler(&snapshot, &seal, 240));
            }
            if whole == 0 {
                let chars = (gap - 700 - (per_event - 240)).clamp(0, 240) as usize;
                snapshot.events.push(filler(&snapshot, &seal, chars));
            }
        } else {
            let remove = ((-gap + 700) / per_event + 1) as usize;
            let keep = snapshot.events.len().saturating_sub(remove).max(fixed);
            snapshot.events.truncate(keep);
        }
        bytes = encode(&snapshot).unwrap();
    }
    let gap = target - bytes.len() as i64;
    assert!((0..=1500).contains(&gap), "built within 1.5 KB of the target, gap {gap}");
    validate(&snapshot).unwrap();
    std::fs::write(dir.v2(), &bytes).unwrap();
    let size = |label: &str| {
        let len = std::fs::metadata(dir.v2()).unwrap().len();
        println!("{label}: {len} bytes ({:.3} MiB)", len as f64 / 1_048_576.0);
        assert!(len <= limits.max_file_bytes, "{label}: within 64 MiB");
        len
    };
    let built = size("built");
    assert!(built > 60 << 20, "a real near-64 MiB file");
    let second = ApprovalRef::new(&seal).unwrap();
    let applied = vault.record_approval(&second).unwrap();
    assert!(applied.notices.iter().any(|notice| matches!(notice, Notice::NearFull { .. })));
    size("approved");
    let before_refusal = std::fs::read(dir.v2()).unwrap();
    assert_eq!(
        vault.prepare_use(&second, &details()),
        Err(VaultError::Full { limit: limits.max_file_bytes }),
        "no stamp whose final line might not fit"
    );
    assert_eq!(std::fs::read(dir.v2()).unwrap(), before_refusal, "the refusal changes nothing");
    vault.commit_use(&open.op_id).unwrap();
    size("open stamp committed");
    vault.prepare_use(&second, &details()).unwrap();
    size("second prepared");
    vault.commit_use(&second.op_id).unwrap();
    size("second committed");
    let finished = ready(&vault);
    assert_eq!(finished.use_state(&open.op_id), UseState::Committed(Placement::InDocument));
    assert_eq!(finished.use_state(&second.op_id), UseState::Committed(Placement::InDocument));
    assert_eq!(finished.events.len(), snapshot.events.len() + 4, "every line kept");
    assert!(finished.open_uses().is_empty());
}

#[test]
fn an_unknown_outcome_is_rechecked_by_op_id_and_not_recorded_twice() {
    let dir = TestDir::new("outcome");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let approval = approve(&vault, &seal);
    vault.prepare_use(&approval, &details()).unwrap();
    vault.set_hook(Some(Box::new(|point| point == Point::AfterWrite)));
    assert!(vault.commit_use(&approval.op_id).is_err(), "the caller does not hear success");
    vault.set_hook(None);
    let status = vault.use_status(&approval.op_id).unwrap();
    assert_eq!(status.state, UseState::Committed(Placement::InDocument), "the line did land");
    assert_eq!(status.store_id, ready(&vault).store_id);
    assert!(!status.store_id.is_empty());
    assert!(!vault.commit_use(&approval.op_id).unwrap().changed, "the retry is not a second line");
    assert_eq!(ready(&vault).use_events(&approval.op_id).filter(|event| event.kind == kind::USE_COMMITTED).count(), 1);
}

#[test]
fn a_write_that_does_not_read_back_is_an_error_and_the_store_is_not_reset() {
    let dir = TestDir::new("verify");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let approval = approve(&vault, &seal);
    let v2 = dir.v2();
    vault.set_hook(Some(Box::new(move |point| {
        if point == Point::AfterWrite {
            let mut bytes = std::fs::read(&v2).unwrap();
            let last = bytes.len() - 1;
            bytes[last] ^= 0x40;
            std::fs::write(&v2, bytes).unwrap();
        }
        false
    })));
    assert_eq!(vault.prepare_use(&approval, &details()), Err(VaultError::Verify), "no success before the file reads back");
    vault.set_hook(None);
    let damaged = std::fs::read(dir.v2()).unwrap();
    assert!(matches!(vault.state(), Err(VaultError::Unreadable(_))));
    assert!(matches!(vault.register(new_seal("合成B", 2)), Err(VaultError::Unreadable(_))));
    assert_eq!(std::fs::read(dir.v2()).unwrap(), damaged, "not replaced with an empty store");
}

#[test]
fn a_briefly_held_file_is_retried_a_bounded_number_of_times() {
    let mut calls = 0;
    let result = with_retry(|| {
        calls += 1;
        if calls < 3 { Err(std::io::Error::from_raw_os_error(5)) } else { Ok(calls) }
    });
    assert_eq!(result.unwrap(), 3, "access denied twice, then the replace goes through");
    let mut calls = 0;
    let result: std::io::Result<()> = with_retry(|| {
        calls += 1;
        Err(std::io::Error::from_raw_os_error(32))
    });
    assert!(result.is_err());
    assert_eq!(calls, 9, "a lasting sharing violation is given up after eight retries");
    let mut calls = 0;
    let start = Instant::now();
    let result: std::io::Result<()> = with_retry(|| {
        calls += 1;
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"))
    });
    assert!(result.is_err());
    assert_eq!(calls, 1, "other errors are not retried");
    assert!(start.elapsed() < Duration::from_millis(15));
}

#[test]
fn a_failed_replace_keeps_the_previous_file() {
    let dir = TestDir::new("replace");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let before = std::fs::read(dir.v2()).unwrap();
    let mut permissions = std::fs::metadata(dir.v2()).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(dir.v2(), permissions.clone()).unwrap();
    let result = vault.record_approval(&ApprovalRef::new(&seal).unwrap());
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(dir.v2(), permissions).unwrap();
    assert!(matches!(result, Err(VaultError::Io(_))), "{result:?}");
    assert_eq!(std::fs::read(dir.v2()).unwrap(), before);
    assert_eq!(names(dir.path()), [LEGACY_FILE, LOCK_FILE, V2_FILE], "no staging file is left behind");
    assert_eq!(ready(&vault).generation, 1);
}

// --- D. Migration -----------------------------------------------------------------------------

#[test]
fn migration_keeps_every_seal_and_line_and_moves_the_file_byte_for_byte() {
    let dir = TestDir::new("migrate");
    let old = write_legacy(&dir, &[("合成A", 1), ("合成B", 2)], 400);
    let original = std::fs::read(dir.legacy()).unwrap();
    let source_sha = crate::stamps_ui::sha256_hex(&original);
    let vault = SealVault::new(dir.path());
    let migrated = vault.migrate().unwrap();
    let backup = format!("seals.bin.swseal1-{}.bak", &source_sha[..16]);
    assert_eq!(migrated.outcome, MigrationOutcome::Migrated { backup: backup.clone() });
    let snapshot = migrated.snapshot.unwrap();
    assert_eq!(ready(&vault), snapshot);
    assert_eq!(std::fs::read(dir.path().join(&backup)).unwrap(), original, "the backup is the original file");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE);
    assert_eq!(names(dir.path()), [LEGACY_FILE, LOCK_FILE, backup.as_str(), V2_FILE]);
    let from = snapshot.migrated_from.clone().unwrap();
    assert_eq!((from.source_sha256.as_str(), from.source_seals, from.source_events), (source_sha.as_str(), 2, 400));
    assert_eq!(snapshot.seals.len(), 2);
    for (kept, seal) in snapshot.seals.iter().zip(&old.library.seals) {
        assert_eq!(kept.png().unwrap(), seal.data);
        assert_eq!((kept.name.as_str(), kept.revision, kept.scale), (seal.name.as_str(), 1, seal.scale));
    }
    assert_eq!(snapshot.events.len(), 401);
    for (line, event) in old.library.history.iter().zip(&snapshot.events) {
        assert_eq!(event.kind, "legacy:confirmed");
        assert_eq!(
            (event.at.as_str(), event.file.as_str(), event.seal_name.as_str(), event.seq),
            (line.at.as_str(), line.file.as_str(), line.seal.as_str(), 1)
        );
        assert_eq!(event.seal_id, snapshot.seals[0].id);
    }
    assert_eq!(snapshot.events.last().unwrap().kind, kind::MIGRATED);
    assert!(SealStore::load(dir.legacy()).locked(), "the format-1 code refuses the tombstone");
    assert_eq!(vault.migrate().unwrap().outcome, MigrationOutcome::AlreadyMigrated);
    assert_eq!(std::fs::read(dir.path().join(&backup)).unwrap(), original);
    assert_eq!(ready(&vault), snapshot, "a second migrate changes nothing");
}

#[test]
fn an_unmigrated_store_is_not_touched_until_migrate() {
    let dir = TestDir::new("defer");
    write_legacy(&dir, &[("合成A", 1)], 5);
    let before = std::fs::read(dir.legacy()).unwrap();
    let vault = SealVault::new(dir.path());
    assert_eq!(vault.state().unwrap(), StoreState::NeedsMigration { seals: 1, events: 5 });
    assert_eq!(vault.register(new_seal("合成B", 2)), Err(VaultError::NeedsMigration { seals: 1, events: 5 }));
    assert_eq!(vault.commit_use("op-00000000000000000000000000000000"), Err(VaultError::NeedsMigration { seals: 1, events: 5 }));
    assert_eq!(vault.use_status("op-00000000000000000000000000000000"), Err(VaultError::NeedsMigration { seals: 1, events: 5 }));
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), before);
    assert_eq!(names(dir.path()), [LEGACY_FILE, LOCK_FILE]);
    assert!(!SealStore::load(dir.legacy()).locked(), "the format-1 code still opens it");
}

#[test]
fn nothing_to_migrate_and_an_unreadable_source_write_nothing() {
    let empty = TestDir::new("nothing");
    let vault = SealVault::new(empty.path());
    assert_eq!(vault.state().unwrap(), StoreState::Empty);
    assert_eq!(vault.migrate().unwrap().outcome, MigrationOutcome::NothingToMigrate);
    assert_eq!(names(empty.path()), [LOCK_FILE]);
    let broken = TestDir::new("broken");
    let mut bytes = b"SWSEAL1\n".to_vec();
    bytes.extend_from_slice(b"not a dpapi blob");
    std::fs::write(broken.legacy(), &bytes).unwrap();
    let vault = SealVault::new(broken.path());
    assert!(matches!(vault.migrate(), Err(VaultError::Unreadable(_))));
    assert!(matches!(vault.register(new_seal("合成A", 1)), Err(VaultError::Unreadable(_))));
    assert_eq!(std::fs::read(broken.legacy()).unwrap(), bytes);
    assert!(!broken.v2().exists());
    let unknown = TestDir::new("unknown");
    std::fs::write(unknown.legacy(), b"something else").unwrap();
    let vault = SealVault::new(unknown.path());
    assert_eq!(vault.migrate(), Err(VaultError::UnknownLegacyFile));
    assert_eq!(std::fs::read(unknown.legacy()).unwrap(), b"something else");
}

#[test]
fn a_taken_backup_name_stops_migration_before_anything_is_written() {
    let dir = TestDir::new("taken");
    write_legacy(&dir, &[("合成A", 1)], 1);
    let original = std::fs::read(dir.legacy()).unwrap();
    let backup = dir.path().join(format!("seals.bin.swseal1-{}.bak", &crate::stamps_ui::sha256_hex(&original)[..16]));
    std::fs::write(&backup, b"someone's file").unwrap();
    assert!(matches!(SealVault::new(dir.path()).migrate(), Err(VaultError::Io(_))));
    assert_eq!(std::fs::read(&backup).unwrap(), b"someone's file");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), original);
    assert!(!dir.v2().exists());
}

fn interrupted_at(point: Point) -> (TestDir, Vec<u8>, SealVault) {
    let dir = TestDir::new("interrupt");
    write_legacy(&dir, &[("合成A", 1)], 3);
    let original = std::fs::read(dir.legacy()).unwrap();
    let vault = SealVault::new(dir.path());
    vault.set_hook(Some(Box::new(move |at| at == point)));
    assert!(vault.migrate().is_err(), "stopped at {point:?}");
    vault.set_hook(None);
    (dir, original, vault)
}

fn assert_recovered(dir: &TestDir, original: &[u8], vault: &SealVault) {
    let source_sha = crate::stamps_ui::sha256_hex(original);
    let backup = dir.path().join(format!("seals.bin.swseal1-{}.bak", &source_sha[..16]));
    let applied = vault.register(new_seal("合成B", 2)).unwrap();
    assert_eq!(std::fs::read(&backup).unwrap(), original, "the backup is the original file");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE);
    let snapshot = applied.snapshot;
    assert_eq!(snapshot.events.iter().filter(|event| event.kind == kind::MIGRATED).count(), 1, "migrated once");
    assert_eq!(snapshot.events.iter().filter(|event| event.kind == "legacy:confirmed").count(), 3);
    assert_eq!(snapshot.seals.iter().map(|seal| seal.name.as_str()).collect::<Vec<_>>(), ["合成A", "合成B"]);
    assert_eq!(snapshot.migrated_from.unwrap().source_sha256, source_sha);
    assert_eq!(names(dir.path()).len(), 4, "v2, tombstone, lock, backup and nothing else");
}

#[test]
fn migration_stopped_right_after_the_v2_write_finishes_on_the_next_change() {
    let (dir, original, vault) = interrupted_at(Point::AfterV2Write);
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), original, "seals.bin is untouched");
    assert!(dir.v2().exists());
    assert!(matches!(vault.state().unwrap(), StoreState::Ready { legacy: LegacyFile::MigrationUnfinished, .. }));
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), original, "reading the state does not finish it");
    assert_recovered(&dir, &original, &vault);
}

#[test]
fn migration_stopped_right_after_the_rename_finishes_on_the_next_change() {
    let (dir, original, vault) = interrupted_at(Point::AfterRename);
    assert!(!dir.legacy().exists());
    assert!(matches!(vault.state().unwrap(), StoreState::Ready { legacy: LegacyFile::Absent, .. }));
    assert_recovered(&dir, &original, &vault);
}

#[test]
fn migration_stopped_right_before_the_tombstone_finishes_on_the_next_change() {
    let (dir, original, vault) = interrupted_at(Point::BeforeTombstone);
    assert!(!dir.legacy().exists());
    assert_recovered(&dir, &original, &vault);
}

#[test]
fn migration_stopped_before_the_v2_write_starts_over_cleanly() {
    let (dir, original, vault) = interrupted_at(Point::BeforeV2Write);
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), original);
    assert_eq!(names(dir.path()), [LEGACY_FILE, LOCK_FILE]);
    assert!(matches!(vault.migrate().unwrap().outcome, MigrationOutcome::Migrated { .. }));
    assert_recovered(&dir, &original, &vault);
}

#[test]
fn a_tombstone_without_v2_is_an_error_and_nothing_is_created() {
    let dir = TestDir::new("v2missing");
    write_legacy(&dir, &[("合成A", 1)], 1);
    let vault = SealVault::new(dir.path());
    vault.migrate().unwrap();
    std::fs::remove_file(dir.v2()).unwrap();
    let before = names(dir.path());
    assert_eq!(vault.state(), Err(VaultError::V2Missing));
    assert_eq!(vault.register(new_seal("合成B", 2)), Err(VaultError::V2Missing));
    assert_eq!(vault.migrate(), Err(VaultError::V2Missing));
    assert_eq!(vault.use_status("op-00000000000000000000000000000000"), Err(VaultError::V2Missing));
    assert_eq!(names(dir.path()), before, "no empty store is made");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE);
}

fn set_aside_lines(snapshot: &Snapshot) -> Vec<(String, String)> {
    snapshot
        .events
        .iter()
        .filter(|event| event.kind == kind::LEGACY_WRITE_SET_ASIDE)
        .map(|event| {
            let note = event.note.clone().unwrap_or_default();
            (note.backup, note.source_sha256)
        })
        .collect()
}

#[test]
fn a_late_format_1_write_is_moved_aside_each_time_and_recorded_without_merging() {
    let dir = TestDir::new("legacywrite");
    let mut old = write_legacy(&dir, &[("合成A", 1)], 2);
    let vault = SealVault::new(dir.path());
    vault.migrate().unwrap();
    let seal = ready(&vault).seals[0].clone();
    let approval = approve(&vault, &seal);
    let snapshot_before = ready(&vault);
    old.append_event(SealEvent::line("2026-10-23 01:00:00 UTC", "旧版.pdf", 1, "合成A", "confirmed")).unwrap();
    let first_write = std::fs::read(dir.legacy()).unwrap();
    assert!(first_write.starts_with(b"SWSEAL1\n"), "the older code replaced the tombstone");
    assert!(matches!(vault.state().unwrap(), StoreState::Ready { legacy: LegacyFile::WrittenAfterMigration, .. }));
    let applied = vault.record_approval(&approval).unwrap();
    let first_name = match applied.notices.as_slice() {
        [Notice::LegacyWriteSetAside { backup, sha256 }] => {
            assert_eq!(*sha256, crate::stamps_ui::sha256_hex(&first_write));
            backup.clone()
        }
        other => panic!("{other:?}"),
    };
    assert!(first_name.starts_with("seals.bin.legacy-g"));
    assert_eq!(std::fs::read(dir.path().join(&first_name)).unwrap(), first_write, "kept byte for byte");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE, "the tombstone is back");
    let after = applied.snapshot;
    assert_eq!(after.seals, snapshot_before.seals, "the old seals are not merged");
    assert_eq!(after.events[..snapshot_before.events.len()], snapshot_before.events[..], "every earlier line is kept as it was");
    assert_eq!(after.events.len(), snapshot_before.events.len() + 1, "only the set-aside line is added; the old line is not merged");
    assert_eq!(set_aside_lines(&after), [(first_name.clone(), crate::stamps_ui::sha256_hex(&first_write))]);
    assert_eq!(after.events.last().map(|event| event.seq), Some(after.generation));
    assert_eq!(after, ready(&vault), "the line is in the file");
    assert!(!vault.record_approval(&approval).unwrap().changed, "a retry of the approval writes nothing more");
    old.append_event(SealEvent::line("2026-10-23 01:00:01 UTC", "旧版.pdf", 1, "合成A", "confirmed")).unwrap();
    let second_write = std::fs::read(dir.legacy()).unwrap();
    let applied = vault.prepare_use(&approval, &details()).unwrap();
    let second_name = match applied.notices.as_slice() {
        [Notice::LegacyWriteSetAside { backup, .. }] => backup.clone(),
        other => panic!("{other:?}"),
    };
    assert_ne!(second_name, first_name);
    assert_eq!(std::fs::read(dir.path().join(&first_name)).unwrap(), first_write, "an earlier backup is not overwritten");
    assert_eq!(std::fs::read(dir.path().join(&second_name)).unwrap(), second_write);
    assert_eq!(applied.snapshot.events.iter().filter(|event| event.kind.starts_with("legacy:")).count(), 2, "still only the migrated lines");
    assert_eq!(
        set_aside_lines(&applied.snapshot),
        [(first_name.clone(), crate::stamps_ui::sha256_hex(&first_write)), (second_name.clone(), crate::stamps_ui::sha256_hex(&second_write))]
    );
    assert_eq!(applied.snapshot.use_state(&approval.op_id), UseState::Prepared, "the request itself is applied in the same write");
    let same_sha = format!("seals.bin.legacy-g{}-{}", applied.snapshot.generation, &crate::stamps_ui::sha256_hex(&second_write)[..16]);
    std::fs::write(dir.legacy(), &second_write).unwrap();
    std::fs::write(dir.path().join(format!("{same_sha}.bak")), b"taken").unwrap();
    let applied = vault.commit_use(&approval.op_id).unwrap();
    assert!(matches!(applied.notices.as_slice(), [Notice::LegacyWriteSetAside { backup, .. }] if *backup == format!("{same_sha}-2.bak")));
    let names: Vec<String> = set_aside_lines(&applied.snapshot).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names.len(), 4, "every backup-named file is recorded once: {names:?}");
    assert!(names.contains(&format!("{same_sha}-2.bak")) && names.contains(&first_name) && names.contains(&second_name));
    assert_eq!(std::fs::read(dir.path().join(format!("{same_sha}.bak"))).unwrap(), b"taken");
}

#[test]
fn a_set_aside_whose_line_could_not_be_written_is_an_error_and_is_recorded_once_later() {
    let dir = TestDir::new("legacyrecord");
    let mut old = write_legacy(&dir, &[("合成A", 1)], 1);
    let vault = SealVault::new(dir.path());
    vault.migrate().unwrap();
    let seal = ready(&vault).seals[0].clone();
    let approval = approve(&vault, &seal);
    let v2_before = std::fs::read(dir.v2()).unwrap();
    old.append_event(SealEvent::line("2026-10-23 03:00:00 UTC", "旧版.pdf", 1, "合成A", "confirmed")).unwrap();
    let late = std::fs::read(dir.legacy()).unwrap();
    let tight = SealVault::with_limits(dir.path(), Limits { max_file_bytes: v2_before.len() as u64, step_headroom: 0, ..Limits::default() });
    assert!(matches!(tight.record_approval(&approval), Err(VaultError::Full { .. })), "an unrecorded set-aside is not a success");
    assert_eq!(std::fs::read(dir.v2()).unwrap(), v2_before, "nothing was written");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE);
    let backups: Vec<String> = names(dir.path()).into_iter().filter(|name| name.starts_with("seals.bin.legacy-")).collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(dir.path().join(&backups[0])).unwrap(), late, "the old file is kept");

    vault.set_hook(Some(Box::new(|point| point == Point::AfterWrite)));
    assert!(vault.record_approval(&approval).is_err(), "a write that was not read back is reported");
    vault.set_hook(None);
    let applied = vault.record_approval(&approval).unwrap();
    assert!(!applied.changed, "the line written before the stop is not written twice");
    assert_eq!(set_aside_lines(&applied.snapshot), [(backups[0].clone(), crate::stamps_ui::sha256_hex(&late))]);
    assert!(applied.snapshot.events.iter().all(|event| event.file != "旧版.pdf"), "the old line is not merged");
}

#[test]
fn the_format_1_code_writing_during_migration_is_not_lost() {
    let dir = TestDir::new("race");
    let old = write_legacy(&dir, &[("合成A", 1)], 2);
    let vault = SealVault::new(dir.path());
    let mut racing = old.clone();
    let mut done = false;
    vault.set_hook(Some(Box::new(move |point| {
        if point == Point::LegacyRead && !done {
            done = true;
            racing.append_event(SealEvent::line("2026-10-23 02:00:00 UTC", "途中.pdf", 1, "合成A", "confirmed")).unwrap();
        }
        false
    })));
    let migrated = vault.migrate().unwrap();
    vault.set_hook(None);
    let snapshot = migrated.snapshot.unwrap();
    let lines: Vec<&str> = snapshot.events.iter().filter(|event| event.kind.starts_with("legacy:")).map(|event| event.file.as_str()).collect();
    assert_eq!(lines, ["合成0.pdf", "合成1.pdf", "途中.pdf"], "the line written mid-migration is in v2");
    let from = snapshot.migrated_from.unwrap();
    let backup = std::fs::read(dir.path().join(&from.backup)).unwrap();
    assert_eq!(crate::stamps_ui::sha256_hex(&backup), from.source_sha256, "the backup is what was moved");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE);
    assert_eq!(snapshot.generation, 2, "rebuilt once");
    let mut late = old;
    late.append_event(SealEvent::line("2026-10-23 02:00:01 UTC", "後.pdf", 1, "合成A", "confirmed")).unwrap();
    assert!(SealStore::load(dir.legacy()).library.history.len() == 3, "the stale process replaced the tombstone");
    let applied = vault.register(new_seal("合成B", 2)).unwrap();
    assert!(matches!(applied.notices.as_slice(), [Notice::LegacyWriteSetAside { .. }]), "a stale format-1 process is caught afterwards too");
}

// --- STEP 117H: fault injection (class A / B / C) and two-vault recovery ----------------------

#[test]
fn a_stop_before_write_leaves_the_file_and_the_attempt_unchanged() {
    let dir = TestDir::new("before-write");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let approval = approve(&vault, &seal);
    let before = std::fs::read(dir.v2()).unwrap();
    let generation = ready(&vault).generation;
    vault.set_hook(Some(Box::new(|point| point == Point::BeforeWrite)));
    assert!(matches!(vault.prepare_use(&approval, &details()), Err(VaultError::Io(_))));
    vault.set_hook(None);
    assert_eq!(std::fs::read(dir.v2()).unwrap(), before, "class A/B: the file is not touched");
    assert_eq!(ready(&vault).generation, generation);
    assert_eq!(ready(&vault).use_state(&approval.op_id), UseState::Approved, "no prepared line");
    assert!(!names(dir.path()).iter().any(|name| name.contains("pdfcraft") && name.ends_with(".tmp")));
}

#[test]
fn a_landed_commit_is_unknown_until_use_status_and_cannot_be_cancelled() {
    let dir = TestDir::new("landed-cancel");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let approval = approve(&vault, &seal);
    vault.prepare_use(&approval, &details()).unwrap();
    vault.set_hook(Some(Box::new(|point| point == Point::AfterWrite)));
    assert!(vault.commit_use(&approval.op_id).is_err(), "class C: the caller hears failure");
    vault.set_hook(None);
    assert_eq!(vault.use_status(&approval.op_id).unwrap().state, UseState::Committed(Placement::InDocument));
    assert_eq!(
        conflict(vault.cancel_use(&approval.op_id)).0,
        Conflict::UseState(UseState::Committed(Placement::InDocument)),
        "a landed commit is not an open prepare"
    );
    assert!(!vault.commit_use(&approval.op_id).unwrap().changed);
    assert_eq!(ready(&vault).use_events(&approval.op_id).filter(|event| event.kind == kind::USE_COMMITTED).count(), 1);
}

#[test]
fn a_file_removed_after_replace_is_verify_and_is_not_replaced_with_empty() {
    let dir = TestDir::new("verify-gone");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let approval = approve(&vault, &seal);
    let v2 = dir.v2();
    vault.set_hook(Some(Box::new(move |point| {
        if point == Point::AfterWrite {
            std::fs::remove_file(&v2).unwrap();
        }
        false
    })));
    assert_eq!(vault.prepare_use(&approval, &details()), Err(VaultError::Verify));
    vault.set_hook(None);
    assert!(!dir.v2().exists(), "the replace landed, then the file was gone");
    assert_eq!(std::fs::read(dir.legacy()).unwrap(), TOMBSTONE);
    assert_eq!(vault.state(), Err(VaultError::V2Missing));
    assert_eq!(vault.register(new_seal("合成B", 2)), Err(VaultError::V2Missing), "no empty store is invented");
    assert!(!dir.v2().exists());
}

#[test]
fn two_vaults_a_lost_commit_reply_does_not_drop_the_other_event_or_duplicate() {
    let dir = TestDir::new("two-lost");
    let first = SealVault::new(dir.path());
    let second = SealVault::new(dir.path());
    let seal = registered(&first, "合成A", 1);
    let approval = approve(&first, &seal);
    first.prepare_use(&approval, &details()).unwrap();
    first.set_hook(Some(Box::new(|point| point == Point::AfterWrite)));
    assert!(first.commit_use(&approval.op_id).is_err());
    first.set_hook(None);
    let other = ApprovalRef::new(&seal).unwrap();
    assert!(second.record_approval(&other).unwrap().changed, "B writes after A's lock is released");
    assert_eq!(first.use_status(&approval.op_id).unwrap().state, UseState::Committed(Placement::InDocument));
    assert!(!first.commit_use(&approval.op_id).unwrap().changed, "the same op_id is not a second committed line");
    let snapshot = ready(&first);
    assert_eq!(snapshot.use_events(&approval.op_id).filter(|event| event.kind == kind::USE_COMMITTED).count(), 1);
    assert_eq!(snapshot.use_state(&other.op_id), UseState::Approved, "B's line is kept");
}

#[test]
fn two_vaults_a_stop_before_write_lets_the_other_change_and_retries_once() {
    let dir = TestDir::new("two-before");
    let first = SealVault::new(dir.path());
    let second = SealVault::new(dir.path());
    let seal = registered(&first, "合成A", 1);
    let approval = approve(&first, &seal);
    first.prepare_use(&approval, &details()).unwrap();
    let prepared = std::fs::read(dir.v2()).unwrap();
    first.set_hook(Some(Box::new(|point| point == Point::BeforeWrite)));
    assert!(first.commit_use(&approval.op_id).is_err());
    first.set_hook(None);
    assert_eq!(std::fs::read(dir.v2()).unwrap(), prepared);
    assert_eq!(ready(&first).use_state(&approval.op_id), UseState::Prepared);
    let other = ApprovalRef::new(&seal).unwrap();
    assert!(second.record_approval(&other).unwrap().changed);
    assert!(first.commit_use(&approval.op_id).unwrap().changed, "a later retry of the same op_id is one new line");
    let snapshot = ready(&first);
    assert_eq!(snapshot.use_state(&approval.op_id), UseState::Committed(Placement::InDocument));
    assert_eq!(snapshot.use_state(&other.op_id), UseState::Approved);
    assert_eq!(snapshot.use_events(&approval.op_id).filter(|event| event.kind == kind::USE_COMMITTED).count(), 1);
}

#[test]
fn a_lock_held_by_another_vault_is_busy_and_the_waiter_writes_nothing() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    let dir = TestDir::new("two-busy");
    let holder = SealVault::with_limits(dir.path(), Limits { lock_timeout: Duration::from_secs(3), ..Limits::default() });
    let waiter = SealVault::with_limits(dir.path(), Limits { lock_timeout: Duration::from_millis(200), ..Limits::default() });
    let seal = registered(&holder, "合成A", 1);
    let before = std::fs::read(dir.v2()).unwrap();
    let holding = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let seen = holding.clone();
    let open = release.clone();
    holder.set_hook(Some(Box::new(move |point| {
        if point == Point::BeforeWrite {
            seen.store(true, Ordering::SeqCst);
            while !open.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
        }
        false
    })));
    let approval = ApprovalRef::new(&seal).unwrap();
    let path = dir.path().to_path_buf();
    let worker = thread::spawn(move || holder.record_approval(&approval));
    let start = Instant::now();
    while !holding.load(Ordering::SeqCst) {
        assert!(start.elapsed() < Duration::from_secs(5), "the holder did not take the lock");
        thread::sleep(Duration::from_millis(1));
    }
    let waiting = ApprovalRef::new(&seal).unwrap();
    assert_eq!(waiter.record_approval(&waiting), Err(VaultError::Busy));
    assert_eq!(std::fs::read(dir.v2()).unwrap(), before, "Busy writes nothing");
    release.store(true, Ordering::SeqCst);
    assert!(worker.join().expect("the holder thread").unwrap().changed);
    assert_eq!(ready(&SealVault::new(&path)).use_state(&waiting.op_id), UseState::Unknown);
}

fn notes_are_safe(notes: &str) {
    assert!(!notes.contains("Users\\"), "{notes}");
    assert!(!notes.contains("/Users/"), "{notes}");
    assert!(!notes.contains("iVBORw0"), "{notes}");
    assert!(!notes.contains("合成印"), "{notes}");
    assert!(!notes.contains("password"), "{notes}");
    assert!(notes.contains("file=seals.v2.bin") || notes == "no store I/O notes", "{notes}");
}

/// Not retried by `with_retry`, so one injected failure is one attempt.
#[cfg(windows)]
const NOT_RETRIED_OS: [(pdfcraft_platform::staging::AtomicWriteStage, i32); 4] = {
    use pdfcraft_platform::staging::AtomicWriteStage::*;
    // ERROR_DISK_FULL, ERROR_HANDLE_DISK_FULL, ERROR_WRITE_FAULT, ERROR_NOT_SAME_DEVICE
    [(Create, 112), (Write, 39), (Sync, 29), (Rename, 17)]
};
#[cfg(not(windows))]
const NOT_RETRIED_OS: [(pdfcraft_platform::staging::AtomicWriteStage, i32); 4] = {
    use pdfcraft_platform::staging::AtomicWriteStage::*;
    // ENOSPC, EIO, EIO, EXDEV
    [(Create, 28), (Write, 5), (Sync, 5), (Rename, 18)]
};

/// A replacement that fails at create, write, sync or rename is logged with that step, its OS
/// code and the operation; the file, the audit log and the refusal are as before.
#[test]
fn write_stage_failures_are_named_and_keep_the_history() {
    let dir = TestDir::new("io-stages");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let events_before = ready(&vault).events.len();
    let bytes_before = std::fs::read(dir.v2()).unwrap();
    for (stage, os) in NOT_RETRIED_OS {
        vault.fail_next_write_at(stage, Some(os));
        let approval = ApprovalRef::new(&seal).unwrap();
        assert_eq!(vault.record_approval(&approval), Err(VaultError::Io(WRITE_ERROR)), "{stage:?}");
        let notes = last_store_io_notes();
        let line = format!("stage={} class=Io op=record_approval", stage.as_str());
        assert!(notes.contains(&line), "{stage:?}: {notes}");
        assert!(notes.contains(&format!("os=Some({os})")), "{stage:?}: {notes}");
        notes_are_safe(&notes);
        assert_eq!(std::fs::read(dir.v2()).unwrap(), bytes_before, "{stage:?} must not replace the file");
        let after = ready(&vault);
        assert_eq!(after.events.len(), events_before, "{stage:?} keeps the audit log as it was");
        assert_eq!(after.use_state(&approval.op_id), UseState::Unknown, "{stage:?}: the approval was not recorded");
        // The stamp that approval was for is refused: its preparation has no approval to follow.
        assert_eq!(conflict(vault.prepare_use(&approval, &details())).0, Conflict::NotApproved, "{stage:?}");
        assert_eq!(std::fs::read(dir.v2()).unwrap(), bytes_before, "{stage:?}: the refusal writes nothing");
    }
    // Nothing planted: the next write goes through as before.
    let approval = ApprovalRef::new(&seal).unwrap();
    assert!(vault.record_approval(&approval).unwrap().changed);
}

#[test]
fn verify_and_reread_failures_are_named_and_refuse_the_stamp() {
    let dir = TestDir::new("io-verify");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let events_before = ready(&vault).events.len();
    let v2 = dir.v2();
    vault.set_hook(Some(Box::new(move |point| {
        if point == Point::AfterWrite {
            let mut bytes = std::fs::read(&v2).unwrap();
            let last = bytes.len() - 1;
            bytes[last] ^= 0x40;
            std::fs::write(&v2, bytes).unwrap();
        }
        false
    })));
    let approval = ApprovalRef::new(&seal).unwrap();
    assert_eq!(vault.record_approval(&approval), Err(VaultError::Verify));
    let notes = last_store_io_notes();
    assert!(notes.contains("stage=verify_bytes class=Verify op=record_approval"), "{notes}");
    notes_are_safe(&notes);
    vault.set_hook(None);
    // The mismatched file is not taken as written: the store no longer reads, and nothing stamps.
    assert!(vault.state().is_err() || ready(&vault).use_state(&approval.op_id) == UseState::Unknown);
    assert!(vault.prepare_use(&approval, &details()).is_err(), "no stamp after a failed verify");
    let _ = events_before;

    let dir = TestDir::new("io-reread");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let v2 = dir.v2();
    vault.set_hook(Some(Box::new(move |point| {
        if point == Point::AfterWrite {
            let _ = std::fs::remove_file(&v2);
        }
        false
    })));
    let approval = ApprovalRef::new(&seal).unwrap();
    assert_eq!(vault.record_approval(&approval), Err(VaultError::Verify));
    let notes = last_store_io_notes();
    assert!(notes.contains("stage=reread class=Verify op=record_approval"), "{notes}");
    notes_are_safe(&notes);
}

/// A reread that fails with an I/O error is logged as `Io` with its OS code, not as a mismatch.
#[test]
fn a_reread_io_error_is_logged_as_io() {
    let dir = TestDir::new("io-reread-io");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成A", 1);
    let v2 = dir.v2();
    // A folder where the file was: metadata says "not a file", so the reread refuses it.
    vault.set_hook(Some(Box::new(move |point| {
        if point == Point::AfterWrite {
            let _ = std::fs::remove_file(&v2);
            std::fs::create_dir(&v2).unwrap();
        }
        false
    })));
    let approval = ApprovalRef::new(&seal).unwrap();
    assert!(matches!(vault.record_approval(&approval), Err(VaultError::Unreadable(_))));
    let notes = last_store_io_notes();
    assert!(notes.contains("stage=reread class=Unreadable op=record_approval"), "{notes}");
    notes_are_safe(&notes);
    vault.set_hook(None);
    let _ = std::fs::remove_dir(dir.v2());
}

/// Each failure goes to the log as one line with only the allowed fields.
#[test]
fn the_log_line_names_the_store_file_only() {
    struct Capture(std::sync::Mutex<Vec<String>>);
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            let line = record.args().to_string();
            if line.starts_with("swseal2_io")
                && let Ok(mut lines) = self.0.lock()
            {
                lines.push(line);
            }
        }
        fn flush(&self) {}
    }
    static CAPTURE: Capture = Capture(std::sync::Mutex::new(Vec::new()));
    // No other test in this binary installs a logger; if one ever does, fail loudly here.
    log::set_logger(&CAPTURE).expect("the only logger in this test binary");
    log::set_max_level(log::LevelFilter::Warn);
    let dir = TestDir::new("io-log-line");
    let vault = SealVault::new(dir.path());
    let seal = registered(&vault, "合成印", 1);
    vault.fail_next_write_at(pdfcraft_platform::staging::AtomicWriteStage::Rename, Some(NOT_RETRIED_OS[3].1));
    assert_eq!(vault.record_approval(&ApprovalRef::new(&seal).unwrap()), Err(VaultError::Io(WRITE_ERROR)));
    let lines = CAPTURE.0.lock().unwrap().clone();
    let line = lines.iter().find(|line| line.contains("op=record_approval") && line.contains("stage=rename")).expect("logged");
    assert!(line.starts_with("swseal2_io stage=rename class=Io op=record_approval gen="), "{line}");
    assert!(line.contains("file=seals.v2.bin") && line.contains(" ms="), "{line}");
    let folder = dir.path().to_string_lossy();
    assert!(!line.contains(folder.as_ref()), "no folder path: {line}");
    for secret in ["合成印", "iVBORw0", "SWSEAL2", "seal_id", "sha256"] {
        assert!(!line.contains(secret), "{secret} in {line}");
    }
    assert!(!line.contains(&seal.id) && !line.contains(&seal.sha256), "{line}");
}
