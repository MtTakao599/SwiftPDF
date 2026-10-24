//! Chairman-seal operations as the window runs them: start a store job, and continue from its
//! answer on a later frame.
//!
//! A stamp goes approval → `use_approved` + `use_prepared` → the stamp edit → `use_committed`.
//! Nothing is placed before the store has recorded the prepared line, and a stamp whose committed
//! line cannot be written is taken out of the document again and cancelled. Undo, redo and
//! removal change the document first and are put back when their line cannot be written. While
//! any of this runs, [`crate::PromptLayer::SealWork`] holds the window's input.

use std::sync::Arc;

use crate::seal_service::{Done, Flow, Job, JobResult, Revert, SealRef, SealService, SealState, SealUse, StampOp, StepOp};
use crate::seal_store_v2::{self as v2, Notice, SealVault, Snapshot, UseDetails, UseStep, VaultError};
use crate::{PdfCraftApp, QuickTool};

/// What a document step meant for the seal record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepOutcome {
    /// No chairman stamp was involved, or the step needs no line.
    Kept,
    /// A line is being written. The step is announced, or put back, when the store answers.
    Pending,
    /// The step was put back already and the user was told.
    Reverted,
}

/// Helpers for tests that drive the window's seal operations against a store in a temp folder.
#[cfg(test)]
pub(crate) mod testing {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::PdfCraftApp;
    use crate::seal_store_v2::{Point, SealVault, StoreState};

    /// A store folder under the OS temp dir, never the user's seal folder. Removed on drop.
    pub(crate) struct SealDir(PathBuf);

    impl SealDir {
        pub(crate) fn new(name: &str) -> Self {
            let unique = crate::seal_store_v2::new_id("t").unwrap();
            let dir = std::env::temp_dir().join(format!("swiftpdf-sealui-test-{}-{name}-{}", std::process::id(), unique.get(2..10).unwrap()));
            assert!(dir.starts_with(std::env::temp_dir()));
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                assert!(!dir.starts_with(PathBuf::from(local).join("SwiftPDF")), "never the user's seal folder");
            }
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for SealDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Point `app` at `dir` and wait for the first reading.
    pub(crate) fn connect(app: &mut PdfCraftApp, dir: &SealDir) {
        app.set_seal_dir(dir.path().to_path_buf());
        app.seal_settle();
        assert!(app.seals.usable(), "{:?} {:?}", app.seals.state, app.toast);
    }

    fn use_kind(kind: &str) -> bool {
        kind.starts_with("use_")
    }

    /// The use lines this window last saw, oldest first.
    pub(crate) fn use_kinds(app: &PdfCraftApp) -> Vec<String> {
        app.seals
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.events.iter().filter(|e| use_kind(&e.kind)).map(|e| e.kind.clone()).collect())
            .unwrap_or_default()
    }

    /// The use lines in the file itself, read by a separate store.
    pub(crate) fn use_kinds_on_disk(dir: &SealDir) -> Vec<String> {
        match SealVault::new(dir.path()).state().unwrap() {
            StoreState::Ready { snapshot, .. } => snapshot.events.iter().filter(|e| use_kind(&e.kind)).map(|e| e.kind.clone()).collect(),
            StoreState::Empty => Vec::new(),
            StoreState::NeedsMigration { .. } => panic!("not migrated"),
        }
    }

    pub(crate) const COMMITTED: [&str; 3] = ["use_approved", "use_prepared", "use_committed"];

    /// A placement of the window's first stored seal, as a page click would make it.
    pub(crate) fn chairman_place(app: &PdfCraftApp, page: usize, rect: [f64; 4]) -> crate::stamps_ui::StampPlace {
        let seal = app.seals.seals().first().expect("a registered seal").clone();
        let png = seal.png().unwrap();
        crate::stamps_ui::StampPlace {
            page,
            rect,
            name: seal.name.clone(),
            file: seal.file.clone(),
            data: Arc::new(png.clone()),
            source_page: 0,
            opacity: 1.0,
            chairman: true,
            pixels: crate::seal_store::preview_rgba(&png),
            drag: None,
            scale: 100.0,
            nominal: (60.0, 60.0),
            source: crate::stamps_ui::PlaceSource::Chairman(0),
            circular: false,
            date_overlay: None,
            rotation: 0,
            seal: None,
            seal_ref: Some(crate::seal_service::SealRef::of(&seal)),
            preview_gen: 0,
        }
    }

    /// Let `allowed` more writes of `seals.v2.bin` through, then stop every later one before it
    /// touches the file. `None` lets every write through again.
    pub(crate) fn fail_writes_after(app: &PdfCraftApp, allowed: Option<usize>) {
        let vault = app.seals.vault().expect("a store folder");
        match allowed {
            None => vault.set_hook(None),
            Some(allowed) => {
                let seen = Arc::new(AtomicUsize::new(0));
                vault.set_hook(Some(Box::new(move |point| point == Point::BeforeWrite && seen.fetch_add(1, Ordering::SeqCst) >= allowed)));
            }
        }
    }

    /// Frames until no seal operation runs. The worker answers on its own thread.
    pub(crate) fn settle_frames(h: &mut egui_kittest::Harness<'_, PdfCraftApp>) {
        let start = std::time::Instant::now();
        h.run_steps(1);
        while h.state().seals.running() {
            assert!(start.elapsed() < std::time::Duration::from_secs(60), "the seal operation did not finish");
            std::thread::sleep(std::time::Duration::from_millis(5));
            h.run_steps(1);
        }
        h.run_steps(2);
    }
}

#[cfg(test)]
#[path = "seal_flow_tests.rs"]
mod tests;

fn short_op(op_id: &str) -> &str {
    op_id.get(..11).unwrap_or(op_id)
}

impl PdfCraftApp {
    /// Use `dir` for chairman seals and start reading it in the background. Nothing is written
    /// or migrated until the user asks.
    pub fn set_seal_dir(&mut self, dir: std::path::PathBuf) {
        self.set_seal_vault(SealVault::new(dir));
    }

    pub(crate) fn set_seal_vault(&mut self, vault: SealVault) {
        self.seals = SealService::new(vault);
        self.seal_uses.clear();
        self.seal_orphans.clear();
        self.reload_seals();
    }

    /// Read the store again. A reading that fails leaves the seal list as it was.
    pub(crate) fn reload_seals(&mut self) {
        if self.seals.running() {
            self.notify(crate::seal_service::BUSY_MESSAGE);
            return;
        }
        if matches!(self.seals.state, SealState::NoStore | SealState::Failed(_)) {
            self.seals.state = SealState::Loading;
        }
        if let Err(error) = self.seals.start(Job::Load, Flow::Load) {
            self.seals.state = SealState::Failed(error);
        }
    }

    /// Continue every seal operation that has answered.
    pub(crate) fn poll_seals(&mut self) {
        while let Some((flow, result)) = self.seals.poll() {
            self.finish_seal_job(flow, result);
        }
    }

    /// Run seal operations to the end, including the ones they start. Tests use this instead of frames.
    #[cfg(test)]
    pub(crate) fn seal_settle(&mut self) {
        while let Some((flow, result)) = self.seals.wait() {
            self.finish_seal_job(flow, result);
        }
    }

    fn take_seal_snapshot(&mut self, snapshot: Snapshot) {
        let selected = match self.quick_tool {
            QuickTool::ChairmanSeal(i) => self.seals.seals().get(i).map(|seal| seal.id.clone()),
            _ => None,
        };
        self.seals.snapshot = Some(snapshot);
        self.seals.state = SealState::Ready;
        if let Some(id) = selected {
            match self.seals.seals().iter().position(|seal| seal.id == id) {
                Some(i) => self.quick_tool = QuickTool::ChairmanSeal(i),
                None => {
                    self.quick_tool = QuickTool::Select;
                    self.notify("選んでいた理事長印は削除されました。");
                }
            }
        }
    }

    fn take_seal_done(&mut self, done: &mut Done) {
        if let Some(snapshot) = done.snapshot.take() {
            self.take_seal_snapshot(snapshot);
        }
        let notices = std::mem::take(&mut done.notices);
        self.seal_notices(notices);
    }

    fn seal_notices(&mut self, notices: Vec<Notice>) {
        self.seals.warnings.clear();
        let mut said = Vec::new();
        for notice in notices {
            match notice {
                Notice::ManyEvents { .. } | Notice::NearFull { .. } => self.seals.warnings.push(notice),
                Notice::MigrationFinished { backup } => {
                    said.push(format!("中断していた理事長印の移行を完了しました。元のファイルは {backup} にあります。"))
                }
                Notice::LegacyWriteSetAside { backup, .. } => said.push(format!(
                    "以前の SwiftPDF が理事長印の旧ファイルに書き込んでいました。その内容は取り込まず、{backup} として保管し、記録に残しました。"
                )),
                Notice::TombstoneMissing => {
                    said.push("旧版向けの目印ファイルを作れませんでした。以前の SwiftPDF では理事長印が空に見えることがあります。".into())
                }
                Notice::UnknownLegacyFile => said.push("seals.bin を理事長印のデータとして読めません。ファイルはそのままにしてあります。".into()),
            }
        }
        if !said.is_empty() {
            self.notify(said.join(" "));
        }
    }

    /// Say what failed. A conflict shows the newest list; nothing the user typed is dropped.
    fn seal_failure(&mut self, error: VaultError, prefix: &str) {
        let message = format!("{prefix}{error}");
        match error {
            VaultError::Conflict { latest, .. } => self.take_seal_snapshot(*latest),
            VaultError::NeedsMigration { seals, events } => self.seals.state = SealState::NeedsMigration { seals, events, error: None },
            VaultError::Unreadable(_) | VaultError::V2Missing | VaultError::UnknownLegacyFile => {
                self.seals.state = SealState::Failed(error.to_string());
            }
            VaultError::Busy | VaultError::Full { .. } | VaultError::Rejected(_) | VaultError::Io(_) | VaultError::Verify => {}
        }
        self.notify(message);
    }

    pub(crate) fn finish_seal_job(&mut self, flow: Flow, result: JobResult) {
        match flow {
            Flow::Load => self.finish_seal_load(result),
            Flow::Migrate => self.finish_seal_migration(result),
            Flow::Register { draft } => self.finish_seal_register(draft, result),
            Flow::Update { seal_id, base_revision, done_message } => match result {
                Ok(mut done) => {
                    self.take_seal_done(&mut done);
                    self.rebase_placements(&seal_id, base_revision);
                    self.notify(done_message);
                }
                Err(error) => self.seal_failure(error, "保存しませんでした。"),
            },
            Flow::Delete { seal_id } => match result {
                Ok(mut done) => {
                    self.take_seal_done(&mut done);
                    for view in &mut self.views {
                        if view.stamp_place.as_ref().and_then(|place| place.seal_ref.as_ref()).is_some_and(|seal| seal.id == seal_id) {
                            view.stamp_place = None;
                        }
                    }
                    self.notify("理事長印を削除しました。");
                }
                Err(error @ VaultError::Conflict { .. }) => {
                    self.seal_failure(error, "削除しませんでした。");
                    self.notify("削除しませんでした。最新の内容を表示しました。削除する場合は、もう一度選んで確認してください。");
                }
                Err(error) => self.seal_failure(error, "削除しませんでした。"),
            },
            Flow::Prepare(op) => self.finish_seal_prepare(op, result),
            Flow::Commit(op, refer) => self.finish_seal_commit(op, refer, result),
            Flow::Reconcile(op, refer) => self.finish_seal_reconcile(op, refer, result),
            Flow::Abandon { op_id, message, hold_doc } => match result {
                Ok(mut done) => {
                    self.take_seal_done(&mut done);
                    self.notify(message);
                }
                Err(error) => {
                    let text = format!(
                        "{message} 押印の取り消しを記録できませんでした（{error}）。未完了の押印準備（{}）として残ります。",
                        short_op(&op_id)
                    );
                    if let Some(doc) = hold_doc {
                        self.hold_seal_doc(doc, text);
                    } else {
                        self.notify(text);
                    }
                }
            },
            Flow::Step(step) => self.finish_seal_step(step, result),
        }
    }

    fn finish_seal_load(&mut self, result: JobResult) {
        match result {
            Ok(mut done) => {
                if let Some((seals, events)) = done.needs_migration {
                    let deferred = matches!(self.seals.state, SealState::Deferred { .. });
                    self.seals.snapshot = None;
                    self.seals.state =
                        if deferred { SealState::Deferred { seals, events } } else { SealState::NeedsMigration { seals, events, error: None } };
                    return;
                }
                if done.snapshot.is_none() {
                    self.seals.snapshot = None;
                    self.seals.state = SealState::Ready;
                }
                self.take_seal_done(&mut done);
            }
            Err(VaultError::Busy) => {
                if self.seals.state == SealState::Loading {
                    self.seals.state = SealState::Failed(VaultError::Busy.to_string());
                }
                self.notify(VaultError::Busy.to_string());
            }
            Err(error) => self.seals.state = SealState::Failed(error.to_string()),
        }
    }

    /// Start the migration the user agreed to in [`crate::Dialog::SealMigrate`].
    pub(crate) fn begin_seal_migration(&mut self) {
        if !matches!(self.seals.state, SealState::NeedsMigration { .. } | SealState::Deferred { .. }) {
            return;
        }
        if let Err(error) = self.seals.start(Job::Migrate, Flow::Migrate) {
            self.notify(error);
        }
    }

    pub(crate) fn defer_seal_migration(&mut self) {
        if let SealState::NeedsMigration { seals, events, .. } = self.seals.state {
            self.seals.state = SealState::Deferred { seals, events };
            self.notify("移行を延期しました。理事長印のデータは変更していません。移行するまで理事長印は使えません。");
        }
    }

    fn finish_seal_migration(&mut self, result: JobResult) {
        match result {
            Ok(mut done) => {
                let backup = done.migrated_backup.take();
                if done.snapshot.is_none() {
                    self.seals.snapshot = None;
                    self.seals.state = SealState::Ready;
                }
                self.take_seal_done(&mut done);
                match backup {
                    Some(backup) => {
                        self.notify(format!("理事長印のデータを新しい形式へ移しました。元の暗号化ファイルは {backup} として残してあります。"))
                    }
                    None => self.notify("理事長印のデータは既に新しい形式です。"),
                }
            }
            Err(error) => {
                let (seals, events) = match self.seals.state {
                    SealState::NeedsMigration { seals, events, .. } | SealState::Deferred { seals, events } => (seals, events),
                    _ => (0, 0),
                };
                let message = format!(
                    "移行できませんでした。{error} 元のファイルは削除していません。原因を取り除いてから、もう一度「移行する」を押してください。"
                );
                self.seals.state = SealState::NeedsMigration { seals, events, error: Some(message.clone()) };
                self.notify(message);
            }
        }
    }

    pub(crate) fn begin_seal_register(&mut self, draft: crate::stamps_ui::StampDraft) {
        let seal = v2::NewSeal {
            name: draft.name.trim().to_string(),
            file: draft.file.clone(),
            width: draft.width,
            height: draft.height,
            png: draft.data.to_vec(),
            scale: crate::stamp_preset::scale_or_hundred(draft.scale),
            effects: crate::seal_store::SealEffects::off(),
        };
        if let Err(error) = self.seals.start(Job::Register(seal), Flow::Register { draft: draft.clone() }) {
            self.reopen_seal_registration(draft);
            self.notify(format!("登録しませんでした。{error}"));
        }
    }

    fn reopen_seal_registration(&mut self, draft: crate::stamps_ui::StampDraft) {
        self.stamp_draft = draft;
        self.dialog = Some(crate::Dialog::CreateStamp);
    }

    fn finish_seal_register(&mut self, draft: crate::stamps_ui::StampDraft, result: JobResult) {
        match result {
            Ok(mut done) => {
                let created = done.created.take();
                self.take_seal_done(&mut done);
                if let Some(index) = created.and_then(|id| self.seals.seals().iter().position(|seal| seal.id == id)) {
                    self.quick_tool = QuickTool::ChairmanSeal(index);
                }
                self.note_stamp_try();
            }
            Err(error) => {
                self.seal_failure(error, "登録しませんでした。");
                if self.dialog.is_none() {
                    self.reopen_seal_registration(draft);
                }
            }
        }
    }

    pub(crate) fn begin_seal_update(&mut self, seal: &SealRef, change: v2::SealChange, done_message: &'static str) {
        if !self.seals.usable() {
            self.notify(self.seals.unusable_reason());
            return;
        }
        let flow = Flow::Update { seal_id: seal.id.clone(), base_revision: seal.revision, done_message };
        if let Err(error) = self.seals.start(Job::Update { seal_id: seal.id.clone(), revision: seal.revision, change }, flow) {
            self.notify(format!("保存しませんでした。{error}"));
        }
    }

    /// After this window changed a seal's scale or effects, placements of that seal follow the
    /// new revision. The image is the same, and the placement keeps its own settings.
    fn rebase_placements(&mut self, seal_id: &str, base_revision: u64) {
        let Some(current) = self.seals.seals().iter().find(|seal| seal.id == seal_id).map(SealRef::of) else { return };
        for view in &mut self.views {
            if let Some(seal) = view.stamp_place.as_mut().and_then(|place| place.seal_ref.as_mut())
                && seal.id == current.id
                && seal.revision == base_revision
                && seal.sha256 == current.sha256
            {
                seal.revision = current.revision;
            }
        }
    }

    pub(crate) fn begin_seal_delete(&mut self, seal: SealRef) {
        if let Err(error) = self.seals.start(Job::Delete { seal_id: seal.id.clone(), revision: seal.revision }, Flow::Delete { seal_id: seal.id }) {
            self.notify(format!("削除しませんでした。{error}"));
        }
    }

    // --- Stamping ----------------------------------------------------------------------------

    /// The approved placement in `op`: record the approval and prepare it. The page is changed
    /// only after both lines are in the store.
    pub(crate) fn begin_seal_stamp(&mut self, op: StampOp, details: UseDetails) {
        let approval = v2::ApprovalRef {
            op_id: op.op_id.clone(),
            seal_id: op.seal.id.clone(),
            seal_revision: op.seal.revision,
            seal_sha256: op.seal.sha256.clone(),
        };
        if let Err(error) = self.seals.start(Job::Prepare { approval, details }, Flow::Prepare(Box::new(op))) {
            self.notify(format!("押印しませんでした。{error}"));
        }
    }

    fn finish_seal_prepare(&mut self, op: Box<StampOp>, result: JobResult) {
        let mut done = match result {
            Ok(done) => done,
            // A write that may have landed: close the attempt so it is not left open.
            Err(error @ (VaultError::Verify | VaultError::Io(_))) => {
                let message = format!("使用履歴を記録できないため、押印しませんでした。{error}");
                self.start_seal_abandon(op.op_id, message, None);
                return;
            }
            Err(error @ VaultError::Conflict { .. }) => {
                self.seal_approval = None;
                self.seal_failure(error, "押印しませんでした。");
                return;
            }
            Err(error) => {
                self.seal_failure(error, "使用履歴を記録できないため、押印しませんでした。");
                return;
            }
        };
        self.take_seal_done(&mut done);
        let view = self.views.iter().position(|view| view.id == op.doc);
        let refusal = view.and_then(|index| self.stamp_refusal(index));
        let same_target = self.active_ids().map(|(_, id)| id) == Some(op.doc)
            && view.and_then(|index| self.views.get(index)).and_then(|view| view.stamp_place.as_ref()).is_some_and(|place| {
                place.page == op.place.page
                    && place.preview_gen == op.place.preview_gen
                    && crate::stamps_ui::rect_same(place.rect, op.place.rect)
                    && (place.opacity - op.place.opacity).abs() <= 1.0e-9
                    && place.seal_ref.as_ref() == Some(&op.seal)
            });
        if let Some(why) = refusal {
            self.start_seal_abandon(op.op_id, format!("押印しませんでした。{why}"), None);
            return;
        }
        if !same_target {
            self.start_seal_abandon(op.op_id, "押印する文書か位置が変わったため、押印しませんでした。".into(), None);
            return;
        }
        // Undo frees a stamp's object number for reuse, so "newest" alone may name the new stamp
        // and an undone one alike; only a stamp that was not on a page before is the new one.
        let before = self.session.present_owned_stamps(op.doc);
        let edit = pdfcraft_engine::Edit::AddCustomStamp {
            page: op.place.page,
            rect: op.place.rect,
            name: op.place.name.clone(),
            file: pdfcraft_engine::MarkFile { name: op.place.file.clone(), bytes: Arc::clone(&op.bytes), page: op.place.source_page },
            author: self.comment_prefs.author.clone(),
            opacity: op.place.opacity.clamp(0.0, 1.0),
        };
        if !self.apply_edit(edit) {
            self.start_seal_abandon(op.op_id, "押印は文書に入っていません。".into(), None);
            return;
        }
        let added: Vec<_> = self.session.present_owned_stamps(op.doc).into_iter().filter(|refer| !before.contains(refer)).collect();
        match added.as_slice() {
            [refer] => {
                let refer = *refer;
                let mut op = op;
                op.annot_nm = stamp_annot_name(&self.session, op.doc, refer).unwrap_or_default();
                let job = Job::Commit { op_id: op.op_id.clone() };
                if let Err(error) = self.seals.start(job, Flow::Commit(op.clone(), refer)) {
                    self.take_back_seal(op, refer, format!("押印を記録できないため、押印を取り消しました。{error}"), None);
                }
            }
            _ => {
                // The edit went in but its stamp cannot be named, so its record cannot follow it.
                let undone = self.session.undo(op.doc).is_ok();
                self.refresh_doc_view(op.doc);
                if undone {
                    self.start_seal_abandon(op.op_id, "押印を確認できないため、押印を取り消しました。".into(), None);
                } else {
                    self.hold_seal_doc(op.doc, "押印を確認できず、文書からも取り消せませんでした。".into());
                }
            }
        }
    }

    fn finish_seal_commit(&mut self, op: Box<StampOp>, refer: pdfcraft_cos::ObjRef, result: JobResult) {
        match result {
            Ok(mut done) => {
                self.take_seal_done(&mut done);
                self.accept_seal_commit(*op, refer);
            }
            Err(error @ (VaultError::Verify | VaultError::Io(_))) => {
                let job = Job::Reconcile { op_id: op.op_id.clone() };
                if let Err(start) = self.seals.start(job, Flow::Reconcile(op.clone(), refer)) {
                    self.hold_seal_doc(op.doc, format!("押印の記録を確認できませんでした。{error} {start}"));
                }
            }
            Err(error) => {
                let message = format!("押印を記録できないため、押印を取り消しました。{error}");
                self.take_back_seal(op, refer, message, None);
            }
        }
    }

    fn finish_seal_reconcile(&mut self, op: Box<StampOp>, refer: pdfcraft_cos::ObjRef, result: JobResult) {
        let mut done = match result {
            Ok(done) => done,
            Err(error) => {
                self.hold_seal_doc(op.doc, format!("押印の記録を確認できませんでした。{error}"));
                return;
            }
        };
        self.take_seal_done(&mut done);
        let Some(status) = done.use_status else {
            self.hold_seal_doc(op.doc, "押印の記録を確認できませんでした。".into());
            return;
        };
        if status.store_id.is_empty() || status.store_id != op.store_id {
            self.hold_seal_doc(op.doc, "押印の記録が、この操作の保管先と一致しません。".into());
            return;
        }
        match status.state {
            v2::UseState::Committed(v2::Placement::InDocument) if stamp_status_matches(&status, &op) && self.stamp_annot_still_ours(&op, refer) => {
                self.accept_seal_commit(*op, refer);
            }
            v2::UseState::Prepared if stamp_status_matches(&status, &op) => {
                let message = "押印は記録されていなかったため、取り消しました。".into();
                self.take_back_seal(op, refer, message, None);
            }
            _ => {
                self.hold_seal_doc(op.doc, format!("押印の記録を確定できません（{}）。", short_op(&op.op_id)));
            }
        }
    }

    fn accept_seal_commit(&mut self, op: StampOp, refer: pdfcraft_cos::ObjRef) {
        self.seal_uses.push(SealUse {
            op_id: op.op_id.clone(),
            doc: op.doc,
            num: refer.num,
            generation: refer.generation,
            name: op.place.name.clone(),
            page: op.place.page.saturating_add(1) as u32,
            file_name: op.file_name.clone(),
            active: true,
            undone: false,
        });
        let active = self.active_ids().map(|(_, id)| id) == Some(op.doc);
        if let Some(view) = self.views.iter_mut().find(|view| view.id == op.doc) {
            view.stamp_place = None;
        }
        self.seal_approval = None;
        if active {
            self.quick_tool = QuickTool::Select;
        }
    }

    fn stamp_annot_still_ours(&self, op: &StampOp, refer: pdfcraft_cos::ObjRef) -> bool {
        if op.annot_nm.is_empty() {
            return false;
        }
        let Some(doc) = self.session.get(op.doc) else {
            return false;
        };
        // A duplicate `/NM` is not "the first match wins": reconciliation keeps the stamp only
        // when exactly one Stamp annotation on this document has the captured name, it is still
        // on the intended page, and it is still the object this operation added.
        let named: Vec<_> =
            doc.info.annotations.iter().filter(|annot| annot.subtype == "Stamp" && annot.name.as_deref() == Some(op.annot_nm.as_str())).collect();
        let [annot] = named.as_slice() else {
            return false;
        };
        annot.page == op.place.page
            && self.session.owned_stamp_ref(op.doc, annot.page, annot.index) == Some(refer)
            && self.session.present_owned_stamps(op.doc).contains(&refer)
    }

    /// Take a stamp whose committed line was not written back out of the document, then cancel
    /// its attempt. Redo is refused for it from then on.
    fn take_back_seal(&mut self, op: Box<StampOp>, refer: pdfcraft_cos::ObjRef, message: String, hold_doc: Option<pdfcraft_engine::DocId>) {
        let on_top = self.session.get(op.doc).and_then(|doc| doc.can_undo()) == Some("Add stamp")
            && self.session.present_owned_stamps(op.doc).contains(&refer);
        let removed = on_top && self.session.undo(op.doc).is_ok() && !self.session.present_owned_stamps(op.doc).contains(&refer);
        self.refresh_doc_view(op.doc);
        if removed {
            self.seal_orphans.push((op.doc, refer));
            self.start_seal_abandon(op.op_id, message, hold_doc);
        } else {
            self.hold_seal_doc(
                op.doc,
                format!("押印を記録できず、文書からも取り消せませんでした。記録は未完了（{}）のまま残っています。", short_op(&op.op_id)),
            );
        }
    }

    /// The document holds a seal change its history does not match, and it could not be put
    /// back: nothing more may be saved, printed, exported or edited from it. Only closing it
    /// without saving ends the hold, and the file on disk never had the change.
    fn hold_seal_doc(&mut self, doc: pdfcraft_engine::DocId, cause: String) {
        let reason = format!(
            "要復旧：{cause}この文書は保存・名前を付けて保存・印刷・書き出し・編集・元に戻す・やり直しができません。保存せずに閉じて、元のファイルを開き直してください。"
        );
        if self.session.hold_output(doc, reason.clone()).is_err() {
            // The document is already closed, so nothing of it can leave the app.
            return;
        }
        if let Some(view) = self.views.iter_mut().find(|view| view.id == doc) {
            view.stamp_place = None;
        }
        self.seal_approval = None;
        self.quick_tool = QuickTool::Select;
        self.notify(reason);
    }

    /// Tell the user why and return `true` when document `id` is held ([`Self::hold_seal_doc`]).
    pub(crate) fn refuse_held(&mut self, id: pdfcraft_engine::DocId) -> bool {
        let Some(reason) = self.session.get(id).and_then(|doc| doc.output_hold()).map(str::to_owned) else { return false };
        self.notify(reason);
        true
    }

    /// Whether the active document is held, so only reading commands run for it.
    pub(crate) fn active_held(&self) -> bool {
        self.active_ids().and_then(|(_, id)| self.session.get(id)).is_some_and(|doc| doc.output_hold().is_some())
    }

    fn start_seal_abandon(&mut self, op_id: String, message: String, hold_doc: Option<pdfcraft_engine::DocId>) {
        let job = Job::Abandon { op_id: op_id.clone() };
        if let Err(error) = self.seals.start(job, Flow::Abandon { op_id: op_id.clone(), message: message.clone(), hold_doc }) {
            let text = format!("{message} 押印の取り消しを記録できませんでした（{error}）。未完了の押印準備（{}）として残ります。", short_op(&op_id));
            if let Some(doc) = hold_doc {
                self.hold_seal_doc(doc, text);
            } else {
                self.notify(text);
            }
        }
    }

    fn refresh_doc_view(&mut self, doc: pdfcraft_engine::DocId) {
        if let Some(index) = self.views.iter().position(|view| view.id == doc)
            && let Some(document) = self.session.get(doc)
        {
            self.views[index].document_changed(&document.info);
        }
    }

    // --- Undo, redo and removal ------------------------------------------------------------

    /// Undone chairman stamps come back only by redo. Once `doc` has nothing to redo they never
    /// will, and the engine may give their object numbers to a new stamp, so forget them here
    /// before a new stamp could be mistaken for one. Their lines stay in the store.
    pub(crate) fn retire_unreachable_seal_uses(&mut self, doc: pdfcraft_engine::DocId) {
        if self.session.get(doc).is_some_and(|document| document.can_redo().is_none()) {
            self.seal_uses.retain(|use_| !(use_.doc == doc && use_.undone));
            self.seal_orphans.retain(|(id, _)| *id != doc);
        }
    }

    /// After undo or redo of `label` in `doc`, write the matching line for a chairman stamp.
    /// `done_message` is shown once the line is written.
    pub(crate) fn note_seal_history_step(
        &mut self,
        doc: pdfcraft_engine::DocId,
        undo: bool,
        label: &str,
        done_message: Option<String>,
    ) -> StepOutcome {
        let deletion = match label {
            "Add stamp" => false,
            "Delete comment" => true,
            _ => return StepOutcome::Kept,
        };
        let present = self.session.present_owned_stamps(doc);
        if !deletion && !undo && self.seal_orphans.iter().any(|(id, refer)| *id == doc && present.contains(refer)) {
            let restored = self.session.undo(doc).is_ok();
            self.refresh_doc_view(doc);
            self.notify(if restored {
                "記録できなかった理事長印は、やり直しで戻せません。"
            } else {
                "記録できなかった理事長印が文書に戻りました。保存や印刷の前に内容を確認してください。"
            });
            return StepOutcome::Reverted;
        }
        let Some(use_) = self
            .seal_uses
            .iter()
            .rev()
            .find(|use_| {
                let here = present.iter().any(|refer| refer.num == use_.num && refer.generation == use_.generation);
                use_.doc == doc && if deletion { use_.active != undo && here == undo } else { use_.active == undo && here != undo }
            })
            .cloned()
        else {
            return StepOutcome::Kept;
        };
        let step = match (deletion, undo) {
            (false, true) => UseStep::Undone,
            (false, false) => UseStep::Redone,
            (true, true) => UseStep::RemovalUndone,
            (true, false) => UseStep::Removed,
        };
        let revert = if undo { Revert::Redo } else { Revert::Undo };
        self.begin_seal_step(StepOp { doc, op_id: use_.op_id, step, revert, done_message })
    }

    /// A chairman stamp was just deleted from `doc` by an edit. Record it, or put it back.
    pub(crate) fn note_seal_removal(&mut self, doc: pdfcraft_engine::DocId, op_id: String) -> StepOutcome {
        self.begin_seal_step(StepOp { doc, op_id, step: UseStep::Removed, revert: Revert::Undo, done_message: None })
    }

    fn begin_seal_step(&mut self, step: StepOp) -> StepOutcome {
        let started = v2::new_id("step").map_err(|error| error.to_string()).and_then(|step_id| {
            let job = Job::Step { op_id: step.op_id.clone(), step_id, step: step.step };
            self.seals.start(job, Flow::Step(step.clone()))
        });
        match started {
            Ok(()) => StepOutcome::Pending,
            Err(error) => {
                self.revert_seal_step(&step, &error);
                StepOutcome::Reverted
            }
        }
    }

    fn finish_seal_step(&mut self, step: StepOp, result: JobResult) {
        match result {
            Ok(mut done) => {
                self.take_seal_done(&mut done);
                if let Some(use_) = self.seal_uses.iter_mut().find(|use_| use_.op_id == step.op_id) {
                    use_.active = matches!(step.step, UseStep::Redone | UseStep::RemovalUndone);
                    use_.undone = step.step == UseStep::Undone;
                }
                if let Some(message) = step.done_message {
                    self.notify(message);
                }
            }
            Err(error) => {
                let reason = error.to_string();
                if let VaultError::Conflict { latest, .. } = error {
                    self.take_seal_snapshot(*latest);
                }
                self.revert_seal_step(&step, &reason);
            }
        }
    }

    fn revert_seal_step(&mut self, step: &StepOp, reason: &str) {
        let restored = match step.revert {
            Revert::Undo => self.session.undo(step.doc).is_ok(),
            Revert::Redo => self.session.redo(step.doc).is_ok(),
        };
        self.refresh_doc_view(step.doc);
        if restored {
            self.notify(format!("使用履歴を記録できないため、文書の変更を元に戻しました。{reason}"));
        } else {
            self.hold_seal_doc(step.doc, format!("使用履歴を記録できず、文書の状態も戻せませんでした。{reason}"));
        }
    }
}

fn stamp_annot_name(session: &pdfcraft_engine::Session, doc: pdfcraft_engine::DocId, refer: pdfcraft_cos::ObjRef) -> Option<String> {
    let info = &session.get(doc)?.info;
    let annot =
        info.annotations.iter().find(|annot| annot.subtype == "Stamp" && session.owned_stamp_ref(doc, annot.page, annot.index) == Some(refer))?;
    let name = annot.name.as_deref()?.to_owned();
    if name.is_empty() {
        return None;
    }
    let copies = info.annotations.iter().filter(|other| other.subtype == "Stamp" && other.name.as_deref() == Some(name.as_str())).count();
    (copies == 1).then_some(name)
}

fn stamp_status_matches(status: &v2::UseStatus, op: &StampOp) -> bool {
    let Some(event) = status.events.iter().rev().find(|event| event.kind == v2::kind::USE_PREPARED || event.kind == v2::kind::USE_COMMITTED) else {
        return false;
    };
    event.op_id == op.op_id
        && event.seal_id == op.seal.id
        && event.seal_revision == op.seal.revision
        && event.seal_sha256 == op.seal.sha256
        && event.image_sha256 == op.image_sha256
}
