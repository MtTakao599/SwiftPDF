//! The window's side of the chairman-seal store ([`crate::seal_store_v2`]).
//!
//! Every store call runs on its own worker thread, one at a time, so a lock wait, a large
//! decryption or a migration never stops the window from drawing. The window polls each frame
//! and continues the operation from the [`Flow`] it started it with. While a [`Flow::blocking`]
//! operation runs, the window shows a prompt that takes all input, so nothing can change the
//! document or start a second seal operation until the store has answered.

use std::sync::{Arc, mpsc};

use crate::seal_store_v2::{self as v2, ApprovalRef, NewSeal, Notice, SealChange, SealVault, Snapshot, StoreState, UseDetails, UseStep, VaultError};

/// One store request. It runs on the worker exactly as given.
#[derive(Clone, Debug)]
pub(crate) enum Job {
    Load,
    Migrate,
    Register(NewSeal),
    Update {
        seal_id: String,
        revision: u64,
        change: SealChange,
    },
    Delete {
        seal_id: String,
        revision: u64,
    },
    /// Record the approval the window made, then prepare the stamp. Nothing is placed yet.
    Prepare {
        approval: ApprovalRef,
        details: UseDetails,
    },
    Commit {
        op_id: String,
    },
    /// Re-read one attempt after a commit whose write-back was not confirmed.
    Reconcile {
        op_id: String,
    },
    /// The stamp did not stay in the document. Cancels a prepared attempt; an attempt that turns
    /// out to be committed already is recorded as undone instead, since the window took it out.
    Abandon {
        op_id: String,
    },
    Step {
        op_id: String,
        step_id: String,
        step: UseStep,
    },
}

/// What a finished job reports. Only the parts its job sets are filled in.
#[derive(Clone, Debug, Default)]
pub(crate) struct Done {
    pub snapshot: Option<Snapshot>,
    pub notices: Vec<Notice>,
    pub created: Option<String>,
    pub needs_migration: Option<(usize, usize)>,
    pub migrated_backup: Option<String>,
    /// [`Job::Abandon`] found the attempt committed and recorded it as undone.
    pub undone_after_commit: bool,
    /// [`Job::Reconcile`] re-read this attempt.
    pub use_status: Option<v2::UseStatus>,
}

pub(crate) type JobResult = Result<Done, VaultError>;

fn run(vault: &SealVault, job: Job) -> JobResult {
    let applied =
        |applied: v2::Applied| Done { snapshot: Some(applied.snapshot), notices: applied.notices, created: applied.created, ..Done::default() };
    match job {
        Job::Load => match vault.state()? {
            StoreState::Empty => Ok(Done::default()),
            StoreState::NeedsMigration { seals, events } => Ok(Done { needs_migration: Some((seals, events)), ..Done::default() }),
            StoreState::Ready { snapshot, notices, .. } => Ok(Done { snapshot: Some(*snapshot), notices, ..Done::default() }),
        },
        Job::Migrate => {
            let migrated = vault.migrate()?;
            let backup = match migrated.outcome {
                v2::MigrationOutcome::Migrated { backup } => Some(backup),
                v2::MigrationOutcome::AlreadyMigrated | v2::MigrationOutcome::NothingToMigrate => None,
            };
            Ok(Done { snapshot: migrated.snapshot, notices: migrated.notices, migrated_backup: backup, ..Done::default() })
        }
        Job::Register(seal) => vault.register(seal).map(applied),
        Job::Update { seal_id, revision, change } => vault.update(&seal_id, revision, change).map(applied),
        Job::Delete { seal_id, revision } => vault.delete(&seal_id, revision).map(applied),
        Job::Prepare { approval, details } => {
            vault.record_approval(&approval)?;
            vault.prepare_use(&approval, &details).map(applied)
        }
        Job::Commit { op_id } => vault.commit_use(&op_id).map(applied),
        Job::Reconcile { op_id } => match vault.state()? {
            StoreState::Ready { snapshot, notices, .. } => Ok(Done {
                use_status: Some(v2::UseStatus {
                    state: snapshot.use_state(&op_id),
                    events: snapshot.use_events(&op_id).cloned().collect(),
                    generation: snapshot.generation,
                    store_id: snapshot.store_id.clone(),
                }),
                snapshot: Some(*snapshot),
                notices,
                ..Done::default()
            }),
            StoreState::Empty => Ok(Done {
                use_status: Some(v2::UseStatus { state: v2::UseState::Unknown, events: Vec::new(), generation: 0, store_id: String::new() }),
                ..Done::default()
            }),
            StoreState::NeedsMigration { seals, events } => Err(VaultError::NeedsMigration { seals, events }),
        },
        Job::Abandon { op_id } => match vault.cancel_use(&op_id) {
            Ok(done) => Ok(applied(done)),
            Err(VaultError::Conflict { reason: v2::Conflict::UseState(v2::UseState::Committed(v2::Placement::InDocument)), .. }) => {
                let step_id = v2::new_id("step")?;
                let mut done = applied(vault.note_use_step(&op_id, &step_id, UseStep::Undone)?);
                done.undone_after_commit = true;
                Ok(done)
            }
            // Never prepared, or already closed: nothing is open for this attempt.
            Err(VaultError::Conflict {
                reason: v2::Conflict::UseState(v2::UseState::Unknown | v2::UseState::Approved | v2::UseState::Cancelled | v2::UseState::Committed(_)),
                latest,
            }) => Ok(Done { snapshot: Some(*latest), ..Done::default() }),
            Err(error) => Err(error),
        },
        Job::Step { op_id, step_id, step } => vault.note_use_step(&op_id, &step_id, step).map(applied),
    }
}

/// Where the seal list stands for this window.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SealState {
    /// No folder was given (the web build, or a test that did not set one).
    NoStore,
    Loading,
    Ready,
    /// Only a format-1 file exists. Nothing is used or written until the user agrees to migrate.
    NeedsMigration {
        seals: usize,
        events: usize,
        error: Option<String>,
    },
    /// The user chose to migrate later. Chairman seals are unavailable until then.
    Deferred {
        seals: usize,
        events: usize,
    },
    /// The store could not be read. The message says why; nothing was written over it.
    Failed(String),
}

/// A chairman seal as one placement saw it. Approval and preparation name exactly this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealRef {
    pub id: String,
    pub revision: u64,
    pub sha256: String,
}

impl SealRef {
    pub(crate) fn of(seal: &v2::Seal) -> Self {
        Self { id: seal.id.clone(), revision: seal.revision, sha256: seal.sha256.clone() }
    }
}

/// A chairman stamp this session placed, so undo, redo and removal name the same attempt.
#[derive(Clone, Debug)]
pub struct SealUse {
    pub op_id: String,
    pub doc: pdfcraft_engine::DocId,
    pub num: u32,
    pub generation: u16,
    pub name: String,
    /// 1-based, as recorded.
    pub page: u32,
    pub file_name: String,
    /// The stamp is in the working document as far as the record says.
    pub active: bool,
    /// Its placement was undone, so only redo can bring it back.
    pub undone: bool,
}

/// A stamp attempt between its approval and its committed or cancelled line.
#[derive(Clone, Debug)]
pub(crate) struct StampOp {
    pub doc: pdfcraft_engine::DocId,
    pub op_id: String,
    pub seal: SealRef,
    pub place: crate::stamps_ui::StampPlace,
    pub bytes: Arc<Vec<u8>>,
    pub file_name: String,
    /// `Snapshot.store_id` when this attempt started. Empty if the window had not read a store.
    pub store_id: String,
    /// SHA-256 of [`Self::bytes`], the PNG given to the stamp.
    pub image_sha256: String,
    /// `/NM` of the annotation this attempt placed. Empty until the edit has run.
    pub annot_nm: String,
}

/// How to take a document step back when its record could not be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Revert {
    Undo,
    Redo,
}

#[derive(Clone, Debug)]
pub(crate) struct StepOp {
    pub doc: pdfcraft_engine::DocId,
    pub op_id: String,
    pub step: UseStep,
    pub revert: Revert,
    /// Shown once the line is written, e.g. "Undid Add stamp".
    pub done_message: Option<String>,
}

/// What the window does with the job's answer.
#[derive(Clone, Debug)]
pub(crate) enum Flow {
    Load,
    Migrate,
    Register {
        draft: crate::stamps_ui::StampDraft,
    },
    Update {
        seal_id: String,
        base_revision: u64,
        done_message: &'static str,
    },
    Delete {
        seal_id: String,
    },
    Prepare(Box<StampOp>),
    Commit(Box<StampOp>, pdfcraft_cos::ObjRef),
    Reconcile(Box<StampOp>, pdfcraft_cos::ObjRef),
    /// `message` explains why the stamp is not in the document. It is shown with the outcome.
    Abandon {
        op_id: String,
        message: String,
        /// When set, a failed cancel holds this document instead of leaving the attempt open.
        hold_doc: Option<pdfcraft_engine::DocId>,
    },
    Step(StepOp),
}

impl Flow {
    /// Operations that change the document or the seal list hold every other input until the
    /// store has answered. Reading the store does not.
    pub(crate) fn blocking(&self) -> bool {
        !matches!(self, Self::Load)
    }

    pub(crate) fn caption(&self) -> &'static str {
        match self {
            Self::Load => "理事長印のデータを読み込んでいます…",
            Self::Migrate => "理事長印のデータを新しい形式へ移しています…",
            Self::Register { .. } => "理事長印を登録しています…",
            Self::Update { .. } => "理事長印の設定を保存しています…",
            Self::Delete { .. } => "理事長印を削除しています…",
            Self::Prepare(_) => "押印の承認を記録しています…",
            Self::Commit(..) => "押印を記録しています…",
            Self::Reconcile(..) => "押印の記録を確認しています…",
            Self::Abandon { .. } => "押印の取り消しを記録しています…",
            Self::Step(_) => "理事長印の使用履歴を記録しています…",
        }
    }
}

struct Running {
    flow: Flow,
    rx: mpsc::Receiver<JobResult>,
}

pub(crate) const BUSY_MESSAGE: &str = "理事長印の処理が終わるまでお待ちください。";

pub struct SealService {
    vault: Option<Arc<SealVault>>,
    pub(crate) state: SealState,
    /// The newest store this window has read or written. `None` until one exists.
    pub(crate) snapshot: Option<Snapshot>,
    /// Size and count warnings from the last answer.
    pub(crate) warnings: Vec<Notice>,
    running: Option<Running>,
    /// Shared with the worker, which wakes the window when it answers. A job started before the
    /// first frame still finds the context once the window has one.
    ctx: SharedContext,
}

impl std::fmt::Debug for SealService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealService").field("vault", &self.vault).field("state", &self.state).finish()
    }
}

impl Default for SealService {
    fn default() -> Self {
        Self { vault: None, state: SealState::NoStore, snapshot: None, warnings: Vec::new(), running: None, ctx: Arc::default() }
    }
}

impl SealService {
    pub(crate) fn new(vault: SealVault) -> Self {
        Self { vault: Some(Arc::new(vault)), ..Self::default() }
    }

    #[cfg(test)]
    pub(crate) fn vault(&self) -> Option<&SealVault> {
        self.vault.as_deref()
    }

    /// Folder the store was opened on. Same path `set_seal_dir` received.
    pub fn store_dir(&self) -> Option<&std::path::Path> {
        self.vault.as_ref().map(|vault| vault.dir())
    }

    pub(crate) fn set_context(&mut self, ctx: &egui::Context) {
        if let Ok(mut slot) = self.ctx.lock()
            && slot.is_none()
        {
            *slot = Some(ctx.clone());
        }
    }

    pub(crate) fn running(&self) -> bool {
        self.running.is_some()
    }

    /// An operation is running that holds the window's input.
    pub(crate) fn blocking(&self) -> bool {
        self.running.as_ref().is_some_and(|running| running.flow.blocking())
    }

    /// The running operation's caption.
    pub(crate) fn progress(&self) -> Option<&'static str> {
        self.running.as_ref().map(|running| running.flow.caption())
    }

    /// Seals may be listed, placed and changed.
    pub(crate) fn usable(&self) -> bool {
        self.state == SealState::Ready
    }

    pub(crate) fn unusable_reason(&self) -> String {
        match &self.state {
            SealState::Ready => String::new(),
            SealState::NoStore => "理事長印の保管先がありません。".into(),
            SealState::Loading => "理事長印のデータを読み込んでいます。少し待ってから、もう一度お試しください。".into(),
            SealState::NeedsMigration { .. } | SealState::Deferred { .. } => "理事長印のデータを新しい形式へ移すまで、理事長印は使えません。".into(),
            SealState::Failed(message) => format!("理事長印は使えません。{message}"),
        }
    }

    pub(crate) fn seals(&self) -> &[v2::Seal] {
        self.snapshot.as_ref().map(|snapshot| snapshot.seals.as_slice()).unwrap_or(&[])
    }

    /// The newest known state of `seal` is the one the placement saw.
    pub(crate) fn matches(&self, seal: &SealRef) -> bool {
        self.snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.seal(&seal.id))
            .is_some_and(|current| current.revision == seal.revision && current.sha256 == seal.sha256)
    }

    /// Prepared attempts that no operation of this window is finishing.
    pub(crate) fn open_uses(&self) -> Vec<String> {
        let mine = match self.running.as_ref().map(|running| &running.flow) {
            Some(Flow::Prepare(op) | Flow::Commit(op, _) | Flow::Reconcile(op, _)) => Some(op.op_id.as_str()),
            Some(Flow::Abandon { op_id, .. }) => Some(op_id.as_str()),
            _ => None,
        };
        self.snapshot.as_ref().map(|snapshot| snapshot.open_uses().into_iter().filter(|op| Some(op.as_str()) != mine).collect()).unwrap_or_default()
    }

    /// Run `job` on a worker. Refused while another seal operation runs.
    pub(crate) fn start(&mut self, job: Job, flow: Flow) -> Result<(), String> {
        if self.running.is_some() {
            return Err(BUSY_MESSAGE.into());
        }
        let Some(vault) = self.vault.clone() else { return Err("理事長印の保管先がありません。".into()) };
        let (tx, rx) = mpsc::channel();
        spawn(vault, job, tx, Arc::clone(&self.ctx))?;
        self.running = Some(Running { flow, rx });
        Ok(())
    }

    /// The finished operation, if it has finished. A worker that stopped without answering is
    /// reported as an error; whatever it wrote is read again by the next operation.
    pub(crate) fn poll(&mut self) -> Option<(Flow, JobResult)> {
        let result = match self.running.as_ref()?.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(VaultError::Io("理事長印の処理が途中で止まりました。")),
        };
        self.running.take().map(|running| (running.flow, result))
    }

    /// Wait for the running operation. Tests use this; the window polls instead.
    #[cfg(test)]
    pub(crate) fn wait(&mut self) -> Option<(Flow, JobResult)> {
        let result = match self.running.as_ref()?.rx.recv_timeout(std::time::Duration::from_secs(120)) {
            Ok(result) => result,
            Err(_) => Err(VaultError::Io("理事長印の処理が途中で止まりました。")),
        };
        self.running.take().map(|running| (running.flow, result))
    }
}

type SharedContext = Arc<std::sync::Mutex<Option<egui::Context>>>;

#[cfg(not(target_arch = "wasm32"))]
fn spawn(vault: Arc<SealVault>, job: Job, tx: mpsc::Sender<JobResult>, ctx: SharedContext) -> Result<(), String> {
    std::thread::Builder::new()
        .name("seal-store".into())
        .spawn(move || {
            let result = run(&vault, job);
            // The window may have closed; then nobody is waiting for the answer.
            let _ = tx.send(result);
            if let Some(ctx) = ctx.lock().ok().and_then(|slot| slot.clone()) {
                ctx.request_repaint();
            }
        })
        .map(|_| ())
        .map_err(|_| "理事長印の処理を開始できませんでした。".to_string())
}

/// The web build has no store folder, so [`SealService::start`] never gets this far there.
#[cfg(target_arch = "wasm32")]
fn spawn(_vault: Arc<SealVault>, _job: Job, _tx: mpsc::Sender<JobResult>, _ctx: SharedContext) -> Result<(), String> {
    Err("この環境では理事長印を使えません。".into())
}
