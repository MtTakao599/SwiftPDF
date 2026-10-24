//! Chairman seals, format 2: `seals.v2.bin`, next to the format-1 `seals.bin`.
//!
//! The file is `SWSEAL2\n` followed by current-user DPAPI ciphertext of one JSON document: the
//! seals, the ids of deleted seals, and every audit event. Nothing is written back from memory.
//! Each change takes the folder's lock (`seals.bin.lock`), reads and checks the newest file,
//! applies one request to it, writes, reads the file back and compares it, and only then returns.
//! A caller that saw an older state gets [`VaultError::Conflict`] with the newest snapshot, and
//! its own request is left with the caller.
//!
//! `seals.bin` is migrated only by [`SealVault::migrate`]. Afterwards it is a plain-text
//! tombstone that format-1 readers refuse to decrypt. If an older SwiftPDF writes it again, the
//! next operation moves that file aside under a new name, puts the tombstone back, reports it,
//! and adds one `legacy_write_set_aside` line naming the backup. Its seals and lines are not
//! merged, and nothing else in `seals.v2.bin` changes because of it.
//!
//! Approval stays the window's job. The store records an approval the window has made, and refuses
//! a placement whose seal was changed or deleted after it. It never decides that a seal may be
//! placed on its own.

use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

pub use crate::seal_store::SealEffects;
use crate::seal_store::{MAX_SEAL_BYTES, MAX_SEALS, SealImage};

pub const V2_FILE: &str = "seals.v2.bin";
pub const LEGACY_FILE: &str = "seals.bin";
pub const LOCK_FILE: &str = "seals.bin.lock";
/// What `seals.bin` holds after migration. Format-1 readers reject it because the magic differs.
pub const TOMBSTONE: &[u8] = b"SWSEAL-MOVED\nSwiftPDF 1.0 moved the chairman seals to seals.v2.bin. Older SwiftPDF versions cannot use them.\n";
const TOMBSTONE_MAGIC: &[u8] = b"SWSEAL-MOVED\n";
const MAGIC: &[u8] = b"SWSEAL2\n";
const LEGACY_MAGIC: &[u8] = b"SWSEAL1\n";
const SCHEMA: u32 = 2;
/// Files above this are not read at all, whatever the configured limit.
const HARD_READ_CAP: u64 = 80 << 20;
const MAX_NAME_CHARS: usize = 40;
const MAX_FILE_CHARS: usize = 240;
const MAX_AT_CHARS: usize = 40;
const MAX_BACKUP_CHARS: usize = 120;
/// A use event's JSON must fit well inside [`Limits::reserve_per_open_use`], so the terminal event
/// that copies it always fits in the room kept for it.
const MAX_USE_EVENT_JSON: usize = 2 << 10;
const MAX_BACKUP_TRIES: u32 = 1000;
/// Backups of late format-1 writes recorded by one operation. Any others are recorded by the next.
const MAX_LEGACY_RECORDS: usize = 16;
const LEGACY_BACKUP_PREFIX: &str = "seals.bin.legacy-";

const READ_ERROR: &str = "理事長印のファイルを読めませんでした。ファイルはそのままにしてあります。";
const DECRYPT_ERROR: &str = "理事長印のファイルを復号できませんでした。ファイルはそのままにしてあります。";
const CONTENT_ERROR: &str = "理事長印のファイルの内容を確認できませんでした。ファイルはそのままにしてあります。";
const LEGACY_ERROR: &str = "旧形式の理事長印ファイルを復号できませんでした。ファイルはそのままにしてあります。";
const WRITE_ERROR: &str = "理事長印のファイルを書き出せませんでした。";
const FOLDER_ERROR: &str = "理事長印の保管フォルダを開けませんでした。";
const BACKUP_EXISTS: &str = "バックアップと同じ名前のファイルが既にあるため、止めました。ファイルはそのままです。";
const RECORD_ERROR: &str = "理事長印の記録に書けない内容です。";

/// Event kinds. `legacy:<event>` lines come from a migrated format-1 file.
pub mod kind {
    pub const SEAL_REGISTERED: &str = "seal_registered";
    pub const SEAL_UPDATED: &str = "seal_updated";
    pub const SEAL_DELETED: &str = "seal_deleted";
    pub const USE_APPROVED: &str = "use_approved";
    pub const USE_PREPARED: &str = "use_prepared";
    pub const USE_COMMITTED: &str = "use_committed";
    pub const USE_CANCELLED: &str = "use_cancelled";
    pub const USE_UNDONE: &str = "use_undone";
    pub const USE_REDONE: &str = "use_redone";
    pub const USE_REMOVED: &str = "use_removed";
    pub const USE_REMOVAL_UNDONE: &str = "use_removal_undone";
    pub const MIGRATED: &str = "migrated";
    /// An older SwiftPDF wrote `seals.bin` after migration. The note names the backup it was
    /// moved to and that file's SHA-256. Its content was not merged.
    pub const LEGACY_WRITE_SET_ASIDE: &str = "legacy_write_set_aside";
    pub const LEGACY_PREFIX: &str = "legacy:";
}

const LEGACY_EVENTS: [&str; 5] = ["confirmed", "undone", "redone", "removed", "removal-undone"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// Largest `seals.v2.bin`, ciphertext and header included.
    pub max_file_bytes: u64,
    /// Room kept below [`Self::max_file_bytes`] for undo, redo and removal lines of stamps already
    /// placed. Registering, approving and preparing may not use it.
    pub step_headroom: u64,
    /// Room kept for the committed or cancelled line of each prepared stamp.
    pub reserve_per_open_use: u64,
    /// Above this many events every result carries [`Notice::ManyEvents`].
    pub warn_events: usize,
    /// Above this file size every result carries [`Notice::NearFull`].
    pub warn_file_bytes: u64,
    pub lock_timeout: Duration,
    pub lock_poll: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 << 20,
            step_headroom: 1 << 20,
            reserve_per_open_use: 8 << 10,
            warn_events: 10_000,
            warn_file_bytes: 48 << 20,
            lock_timeout: Duration::from_secs(3),
            lock_poll: Duration::from_millis(25),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: u32,
    pub store_id: String,
    pub generation: u64,
    pub migrated_from: Option<Migration>,
    pub seals: Vec<Seal>,
    pub deleted: Vec<DeletedSeal>,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Migration {
    pub format: String,
    /// SHA-256 of the whole format-1 file, ciphertext included.
    pub source_sha256: String,
    pub source_seals: usize,
    pub source_events: usize,
    /// File name of the moved format-1 file, in the same folder.
    pub backup: String,
    pub at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seal {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub file: String,
    pub width: u32,
    pub height: u32,
    /// The registered PNG, base64.
    pub data: String,
    /// SHA-256 of the registered PNG.
    pub sha256: String,
    pub scale: f64,
    pub effects: SealEffects,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeletedSeal {
    pub id: String,
    pub name: String,
    pub sha256: String,
    /// The generation that deleted it.
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub id: String,
    /// The generation of the write that added it. Migrated format-1 lines are 1.
    pub seq: u64,
    pub at: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub op_id: String,
    /// Set on undo, redo and removal lines, so a retried step is recorded once.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub step_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub seal_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub seal_name: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub seal_revision: u64,
    /// SHA-256 of the registered PNG the line refers to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub seal_sha256: String,
    /// SHA-256 of the PNG given to the stamp.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub page: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<Note>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    #[serde(default)]
    pub processed: bool,
    #[serde(default)]
    pub angle_deg: f64,
    #[serde(default)]
    pub fade: f64,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub rotation_mode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub backup: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_sha256: String,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

/// Where one stamp attempt stands, replayed from its events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseState {
    Unknown,
    Approved,
    Prepared,
    Committed(Placement),
    Cancelled,
}

/// Where a committed stamp is in the working document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    InDocument,
    Undone,
    Removed,
}

fn next_state(state: UseState, kind: &str) -> Option<UseState> {
    use Placement::{InDocument, Removed, Undone};
    use UseState::{Approved, Cancelled, Committed, Prepared, Unknown};
    match (state, kind) {
        (Unknown, kind::USE_APPROVED) => Some(Approved),
        (Approved, kind::USE_PREPARED) => Some(Prepared),
        (Prepared, kind::USE_COMMITTED) => Some(Committed(InDocument)),
        (Prepared, kind::USE_CANCELLED) => Some(Cancelled),
        (Committed(InDocument), kind::USE_UNDONE) => Some(Committed(Undone)),
        (Committed(Undone), kind::USE_REDONE) => Some(Committed(InDocument)),
        (Committed(InDocument), kind::USE_REMOVED) => Some(Committed(Removed)),
        (Committed(Removed), kind::USE_REMOVAL_UNDONE) => Some(Committed(InDocument)),
        _ => None,
    }
}

fn is_use_kind(kind: &str) -> bool {
    matches!(
        kind,
        kind::USE_APPROVED
            | kind::USE_PREPARED
            | kind::USE_COMMITTED
            | kind::USE_CANCELLED
            | kind::USE_UNDONE
            | kind::USE_REDONE
            | kind::USE_REMOVED
            | kind::USE_REMOVAL_UNDONE
    )
}

fn is_step_kind(kind: &str) -> bool {
    matches!(kind, kind::USE_UNDONE | kind::USE_REDONE | kind::USE_REMOVED | kind::USE_REMOVAL_UNDONE)
}

impl Snapshot {
    fn empty() -> Result<Self, VaultError> {
        Ok(Self {
            schema: SCHEMA,
            store_id: new_id("store")?,
            generation: 0,
            migrated_from: None,
            seals: Vec::new(),
            deleted: Vec::new(),
            events: Vec::new(),
        })
    }

    pub fn seal(&self, id: &str) -> Option<&Seal> {
        self.seals.iter().find(|seal| seal.id == id)
    }

    pub fn is_deleted(&self, id: &str) -> bool {
        self.deleted.iter().any(|deleted| deleted.id == id)
    }

    /// The events of one stamp attempt, in order.
    pub fn use_events<'a>(&'a self, op_id: &'a str) -> impl Iterator<Item = &'a Event> + 'a {
        self.events.iter().filter(move |event| !op_id.is_empty() && event.op_id == op_id)
    }

    pub fn use_state(&self, op_id: &str) -> UseState {
        let mut state = UseState::Unknown;
        for event in self.use_events(op_id) {
            match next_state(state, &event.kind) {
                Some(next) => state = next,
                // A file with a broken sequence is refused when it is read, so this is not reached.
                None => return state,
            }
        }
        state
    }

    /// Stamp attempts prepared but not yet committed or cancelled.
    pub fn open_uses(&self) -> Vec<String> {
        let mut states: HashMap<&str, UseState> = HashMap::new();
        let mut order = Vec::new();
        for event in self.events.iter().filter(|event| is_use_kind(&event.kind)) {
            let state = states.entry(event.op_id.as_str()).or_insert_with(|| {
                order.push(event.op_id.as_str());
                UseState::Unknown
            });
            if let Some(next) = next_state(*state, &event.kind) {
                *state = next;
            }
        }
        order.into_iter().filter(|op| states.get(op) == Some(&UseState::Prepared)).map(str::to_owned).collect()
    }

    fn open_use_count(&self) -> u64 {
        self.open_uses().len() as u64
    }
}

impl Seal {
    /// The registered PNG. `None` only for a snapshot that was not read from a checked file.
    pub fn png(&self) -> Option<Vec<u8>> {
        base64::engine::general_purpose::STANDARD.decode(self.data.as_bytes()).ok()
    }
}

/// A seal to register.
#[derive(Clone, Debug, PartialEq)]
pub struct NewSeal {
    pub name: String,
    pub file: String,
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
    pub scale: f64,
    pub effects: SealEffects,
}

/// One change to a registered seal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SealChange {
    Scale(f64),
    Effects(SealEffects),
}

/// What the window approved: one stamp attempt for one revision of one seal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRef {
    pub op_id: String,
    pub seal_id: String,
    pub seal_revision: u64,
    /// SHA-256 of the registered PNG the window showed.
    pub seal_sha256: String,
}

impl ApprovalRef {
    /// A new attempt for `seal` as the window shows it. Every call gets a new op id.
    pub fn new(seal: &Seal) -> Result<Self, VaultError> {
        Ok(Self { op_id: new_id("op")?, seal_id: seal.id.clone(), seal_revision: seal.revision, seal_sha256: seal.sha256.clone() })
    }
}

/// The stamp a prepared attempt is about to place.
#[derive(Clone, Debug, PartialEq)]
pub struct UseDetails {
    pub file: String,
    pub page: u32,
    /// SHA-256 of the PNG given to the stamp, 64 hex characters.
    pub image_sha256: String,
    pub processed: bool,
    pub angle_deg: f64,
    pub fade: f64,
    pub seed: u64,
    /// `manual`, `random` or empty.
    pub rotation_mode: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseStep {
    Undone,
    Redone,
    Removed,
    RemovalUndone,
}

impl UseStep {
    fn kind(self) -> &'static str {
        match self {
            Self::Undone => kind::USE_UNDONE,
            Self::Redone => kind::USE_REDONE,
            Self::Removed => kind::USE_REMOVED,
            Self::RemovalUndone => kind::USE_REMOVAL_UNDONE,
        }
    }
}

/// Why a request was not applied to the newest store. The request was not changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    SealMissing,
    SealDeleted,
    /// The seal's revision or image differs from the one the caller saw.
    SealChanged,
    DuplicateName,
    TooManySeals,
    /// No approval is recorded for this op id.
    NotApproved,
    /// The approval recorded for this op id names another seal, revision or image.
    ApprovalMismatch,
    /// This op id is already used for a different request.
    OpIdInUse,
    /// The attempt is not in a state that allows the request.
    UseState(UseState),
    /// This step id is already used for a different step.
    StepIdReused,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VaultError {
    /// Another process held the lock for the whole wait. Nothing was applied.
    Busy,
    /// Only a format-1 `seals.bin` exists. Nothing is applied until [`SealVault::migrate`] runs.
    NeedsMigration {
        seals: usize,
        events: usize,
    },
    /// `seals.bin` is a tombstone and `seals.v2.bin` is missing. Nothing is created in its place.
    V2Missing,
    /// A file could not be read, decrypted or checked. It was left as it was.
    Unreadable(&'static str),
    /// `seals.bin` is neither format 1 nor a tombstone. It was left as it was.
    UnknownLegacyFile,
    Conflict {
        reason: Conflict,
        latest: Box<Snapshot>,
    },
    /// The write would leave too little room in the file. Nothing was applied or removed.
    Full {
        limit: u64,
    },
    Rejected(String),
    Io(&'static str),
    /// The written file did not read back as written. Check the outcome with
    /// [`SealVault::use_status`] before retrying a stamp.
    Verify,
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => write!(f, "ほかの SwiftPDF が理事長印のデータを使用中です。少し待ってから、もう一度お試しください。"),
            Self::NeedsMigration { .. } => write!(f, "理事長印のデータを新しい形式に移す必要があります。"),
            Self::V2Missing => {
                write!(f, "理事長印のデータ（seals.v2.bin）が見つかりません。新しく作り直さずに止めています。バックアップの場所を確認してください。")
            }
            Self::Unreadable(message) | Self::Io(message) => write!(f, "{message}"),
            Self::UnknownLegacyFile => write!(f, "seals.bin を理事長印のデータとして読めません。ファイルはそのままにしてあります。"),
            Self::Conflict { reason, .. } => write!(f, "{}", conflict_message(*reason)),
            Self::Full { .. } => write!(f, "理事長印の記録を保存できる上限に近いため、この操作は行いませんでした。これまでの記録は消していません。"),
            Self::Rejected(message) => write!(f, "{message}"),
            Self::Verify => write!(f, "保存した理事長印のファイルを読み直して確認できませんでした。"),
        }
    }
}

impl std::error::Error for VaultError {}

/// Last store I/O failures seen in this process. Stage, class, sizes and OS code only.
/// Never the ciphertext, the plaintext, a seal image, an approval, or a user path.
mod io_note {
    use std::sync::Mutex;
    use std::time::Instant;

    #[derive(Clone, Debug)]
    pub struct Note {
        pub stage: &'static str,
        pub class: &'static str,
        pub op: &'static str,
        pub io_kind: Option<String>,
        pub raw_os: Option<i32>,
        pub generation: u64,
        pub wrote_len: u64,
        pub back_len: Option<u64>,
        pub elapsed_ms: u128,
    }

    static LAST: Mutex<Vec<Note>> = Mutex::new(Vec::new());
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

    pub fn record(
        stage: &'static str,
        class: &'static str,
        op: &'static str,
        generation: u64,
        wrote_len: u64,
        back_len: Option<u64>,
        error: Option<&std::io::Error>,
    ) {
        let io_kind = error.map(|e| format!("{:?}", e.kind()));
        let raw_os = error.and_then(std::io::Error::raw_os_error);
        let elapsed_ms = START.get_or_init(Instant::now).elapsed().as_millis();
        log::warn!(
            "swseal2_io stage={stage} class={class} op={op} gen={generation} wrote={wrote_len} back={} file=seals.v2.bin kind={} os={}",
            back_len.map(|n| n.to_string()).unwrap_or_else(|| "-".into()),
            io_kind.as_deref().unwrap_or("-"),
            raw_os.map(|n| n.to_string()).unwrap_or_else(|| "-".into()),
        );
        let note = Note { stage, class, op, io_kind, raw_os, generation, wrote_len, back_len, elapsed_ms };
        if let Ok(mut slot) = LAST.lock() {
            if slot.len() >= 16 {
                slot.remove(0);
            }
            slot.push(note);
        }
    }

    pub fn record_io(stage: &'static str, class: &'static str, op: &'static str, generation: u64, wrote_len: u64, error: &std::io::Error) {
        record(stage, class, op, generation, wrote_len, None, Some(error));
    }

    pub fn summary() -> String {
        match LAST.lock() {
            Ok(slot) if slot.is_empty() => "no store I/O notes".into(),
            Ok(slot) => slot
                .iter()
                .map(|n| {
                    format!(
                        "stage={} class={} op={} gen={} wrote={} back={:?} file=seals.v2.bin kind={:?} os={:?} t={}ms",
                        n.stage, n.class, n.op, n.generation, n.wrote_len, n.back_len, n.io_kind, n.raw_os, n.elapsed_ms
                    )
                })
                .collect::<Vec<_>>()
                .join(" | "),
            Err(_) => "store I/O notes poisoned".into(),
        }
    }
}

pub(crate) fn last_store_io_notes() -> String {
    io_note::summary()
}

fn conflict_message(reason: Conflict) -> &'static str {
    match reason {
        Conflict::SealMissing => "理事長印が見つかりません。最新の内容を表示しました。",
        Conflict::SealDeleted => "この理事長印は既に削除されています。",
        Conflict::SealChanged => "ほかのウィンドウで変更されました。最新の内容を表示しました。",
        Conflict::DuplicateName => "同じ名前の理事長印が既にあります",
        Conflict::TooManySeals => "理事長印は8件までです",
        Conflict::NotApproved | Conflict::ApprovalMismatch => "承認の記録と一致しないため、押印しませんでした。もう一度確認してください。",
        Conflict::OpIdInUse | Conflict::StepIdReused => "同じ操作が別の内容で記録されています。",
        Conflict::UseState(_) => "この押印は既に確定または取り消されています。",
    }
}

/// Something the caller should tell the user about. The operation itself succeeded.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    ManyEvents {
        events: usize,
    },
    NearFull {
        bytes: u64,
        limit: u64,
    },
    /// An interrupted migration was finished. The format-1 file is at `backup`.
    MigrationFinished {
        backup: String,
    },
    /// An older SwiftPDF wrote `seals.bin` after migration. That file was moved to `backup`
    /// unchanged and not merged. `seals.v2.bin` was not changed by this.
    LegacyWriteSetAside {
        backup: String,
        sha256: String,
    },
    /// The tombstone could not be created. Older versions may see an empty seal list.
    TombstoneMissing,
    /// `seals.bin` holds something this version does not recognise. It was left as it was.
    UnknownLegacyFile,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    /// The store as written, or as read when nothing needed writing.
    pub snapshot: Snapshot,
    /// False when the request was already recorded and nothing was written.
    pub changed: bool,
    /// The id of a seal registered by this call.
    pub created: Option<String>,
    pub notices: Vec<Notice>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MigrationOutcome {
    /// There was no `seals.bin` and no `seals.v2.bin`. Nothing was written.
    NothingToMigrate,
    /// Migrated now. The format-1 file is at `backup`, byte for byte.
    Migrated { backup: String },
    /// `seals.v2.bin` already existed.
    AlreadyMigrated,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Migrated {
    pub outcome: MigrationOutcome,
    pub snapshot: Option<Snapshot>,
    pub notices: Vec<Notice>,
}

/// What `seals.bin` is next to an existing `seals.v2.bin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyFile {
    Absent,
    Tombstone,
    /// Format 1 with the migrated hash and no backup yet. The next operation finishes migrating.
    MigrationUnfinished,
    /// Format 1 written after migration. The next operation moves it aside.
    WrittenAfterMigration,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StoreState {
    Empty,
    NeedsMigration { seals: usize, events: usize },
    Ready { snapshot: Box<Snapshot>, legacy: LegacyFile, notices: Vec<Notice> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct UseStatus {
    pub state: UseState,
    pub events: Vec<Event>,
    pub generation: u64,
    /// The store this status was read from. Empty when there is no `seals.v2.bin`.
    pub store_id: String,
}

/// A point inside an operation where a test can stop it, or change files, as a crash or another
/// process would. Production builds pass these to a no-op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Point {
    LegacyRead,
    BeforeV2Write,
    AfterV2Write,
    AfterRename,
    BeforeTombstone,
    /// Inside every write of `seals.v2.bin`, before the file is touched.
    BeforeWrite,
    AfterWrite,
}

#[cfg(test)]
type Hook = Box<dyn FnMut(Point) -> bool + Send>;

pub struct SealVault {
    dir: PathBuf,
    limits: Limits,
    #[cfg(test)]
    hook: std::sync::Mutex<Option<Hook>>,
    #[cfg(test)]
    fail_stage: std::sync::Mutex<Option<&'static str>>,
}

impl std::fmt::Debug for SealVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealVault").field("dir", &self.dir).field("limits", &self.limits).finish()
    }
}

struct Paths {
    dir: PathBuf,
    v2: PathBuf,
    legacy: PathBuf,
    lock: PathBuf,
}

enum Legacy {
    Absent,
    Tombstone,
    Swseal1(Vec<u8>),
    Other,
}

enum Opened {
    Empty,
    Ready(Box<Snapshot>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Budget {
    /// Registering, updating, approving, preparing: keeps the step headroom free.
    General,
    /// Committing, cancelling, deleting, undo and redo lines.
    Final,
}

enum OpError {
    Conflict(Conflict),
    Rejected(String),
    Vault(VaultError),
}

impl From<VaultError> for OpError {
    fn from(error: VaultError) -> Self {
        Self::Vault(error)
    }
}

enum Change {
    Changed,
    Unchanged,
}

impl SealVault {
    /// The store in `dir`. Nothing is read or written until a method is called.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self::with_limits(dir, Limits::default())
    }

    pub fn with_limits(dir: impl Into<PathBuf>, limits: Limits) -> Self {
        Self {
            dir: dir.into(),
            limits,
            #[cfg(test)]
            hook: std::sync::Mutex::new(None),
            #[cfg(test)]
            fail_stage: std::sync::Mutex::new(None),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    #[cfg(test)]
    pub(crate) fn set_hook(&self, hook: Option<Hook>) {
        if let Ok(mut slot) = self.hook.lock() {
            *slot = hook;
        }
    }

    /// Next `seals.v2.bin` write fails at `stage` before touching the file. Production never sets this.
    #[cfg(test)]
    pub(crate) fn fail_next_write_at(&self, stage: &'static str) {
        if let Ok(mut slot) = self.fail_stage.lock() {
            *slot = Some(stage);
        }
    }

    #[cfg(test)]
    fn take_fail_stage(&self) -> Option<&'static str> {
        self.fail_stage.lock().ok().and_then(|mut slot| slot.take())
    }

    /// Run the test hook. `Err` stands for the process stopping at `point`.
    fn at(&self, point: Point) -> Result<(), VaultError> {
        #[cfg(test)]
        if let Ok(mut slot) = self.hook.lock()
            && let Some(hook) = slot.as_mut()
            && hook(point)
        {
            return Err(VaultError::Io("test: stopped"));
        }
        #[cfg(not(test))]
        let _ = point;
        Ok(())
    }

    fn paths(&self, create: bool) -> Result<Option<Paths>, VaultError> {
        if create {
            std::fs::create_dir_all(&self.dir).map_err(|_| VaultError::Io(FOLDER_ERROR))?;
        } else if self.dir.symlink_metadata().is_err() {
            return Ok(None);
        }
        let dir = std::fs::canonicalize(&self.dir).map_err(|_| VaultError::Io(FOLDER_ERROR))?;
        if !dir.is_dir() {
            return Err(VaultError::Io(FOLDER_ERROR));
        }
        Ok(Some(Paths { v2: dir.join(V2_FILE), legacy: dir.join(LEGACY_FILE), lock: dir.join(LOCK_FILE), dir }))
    }

    fn lock(&self, paths: &Paths) -> Result<pdfcraft_platform::lock::FileLock, VaultError> {
        match pdfcraft_platform::lock::lock_with_timeout(&paths.lock, self.limits.lock_timeout, self.limits.lock_poll) {
            Ok(lock) => Ok(lock),
            Err(pdfcraft_platform::lock::LockError::TimedOut(_)) => Err(VaultError::Busy),
            Err(pdfcraft_platform::lock::LockError::Io(_)) => Err(VaultError::Io(FOLDER_ERROR)),
        }
    }

    /// Read the store without changing any file. An interrupted migration or an older version's
    /// write is reported, not handled; the next change handles it.
    pub fn state(&self) -> Result<StoreState, VaultError> {
        let Some(paths) = self.paths(false)? else { return Ok(StoreState::Empty) };
        if paths.lock.symlink_metadata().is_err() && paths.v2.symlink_metadata().is_err() && paths.legacy.symlink_metadata().is_err() {
            return Ok(StoreState::Empty);
        }
        let _lock = self.lock(&paths)?;
        let legacy = read_legacy(&paths.legacy)?;
        let Some(bytes) = read_opt(&paths.v2)? else {
            return match legacy {
                Legacy::Absent => Ok(StoreState::Empty),
                Legacy::Tombstone => Err(VaultError::V2Missing),
                Legacy::Swseal1(bytes) => {
                    let library = crate::seal_store::decode_file(&bytes).map_err(|_| VaultError::Unreadable(LEGACY_ERROR))?;
                    Ok(StoreState::NeedsMigration { seals: library.seals.len(), events: library.history.len() })
                }
                Legacy::Other => Err(VaultError::UnknownLegacyFile),
            };
        };
        let snapshot = decode(&bytes)?;
        let legacy = match legacy {
            Legacy::Absent => LegacyFile::Absent,
            Legacy::Tombstone => LegacyFile::Tombstone,
            Legacy::Swseal1(bytes) => {
                if self.unfinished_migration(&paths, &snapshot, &bytes) {
                    LegacyFile::MigrationUnfinished
                } else {
                    LegacyFile::WrittenAfterMigration
                }
            }
            Legacy::Other => LegacyFile::Unknown,
        };
        let notices = self.warnings(&snapshot, bytes.len() as u64);
        Ok(StoreState::Ready { snapshot: Box::new(snapshot), legacy, notices })
    }

    /// Re-read one stamp attempt, for a caller that does not know whether its last write landed.
    pub fn use_status(&self, op_id: &str) -> Result<UseStatus, VaultError> {
        let snapshot = match self.state()? {
            StoreState::Ready { snapshot, .. } => snapshot,
            StoreState::Empty => {
                return Ok(UseStatus { state: UseState::Unknown, events: Vec::new(), generation: 0, store_id: String::new() });
            }
            StoreState::NeedsMigration { seals, events } => return Err(VaultError::NeedsMigration { seals, events }),
        };
        Ok(UseStatus {
            state: snapshot.use_state(op_id),
            events: snapshot.use_events(op_id).cloned().collect(),
            generation: snapshot.generation,
            store_id: snapshot.store_id.clone(),
        })
    }

    /// Move a format-1 `seals.bin` into `seals.v2.bin`. Only this call migrates. The format-1
    /// file is moved, not copied or rewritten, and a tombstone takes its name.
    pub fn migrate(&self) -> Result<Migrated, VaultError> {
        let Some(paths) = self.paths(true)? else { return Err(VaultError::Io(FOLDER_ERROR)) };
        let _lock = self.lock(&paths)?;
        let mut notices = Vec::new();
        if read_opt(&paths.v2)?.is_some() {
            let snapshot = match self.open_locked(&paths, &mut notices)? {
                Opened::Ready(snapshot) => *snapshot,
                Opened::Empty => return Err(VaultError::Verify),
            };
            return Ok(Migrated { outcome: MigrationOutcome::AlreadyMigrated, snapshot: Some(snapshot), notices });
        }
        let bytes = match read_legacy(&paths.legacy)? {
            Legacy::Absent => return Ok(Migrated { outcome: MigrationOutcome::NothingToMigrate, snapshot: None, notices }),
            Legacy::Tombstone => return Err(VaultError::V2Missing),
            Legacy::Other => return Err(VaultError::UnknownLegacyFile),
            Legacy::Swseal1(bytes) => bytes,
        };
        let snapshot = build_from_v1(&bytes, None, 1)?;
        if let Some(from) = &snapshot.migrated_from
            && paths.dir.join(&from.backup).symlink_metadata().is_ok()
        {
            return Err(VaultError::Io(BACKUP_EXISTS));
        }
        self.at(Point::LegacyRead)?;
        self.at(Point::BeforeV2Write)?;
        self.store(&paths, &snapshot, false, Budget::Final, "migrate")?;
        self.at(Point::AfterV2Write)?;
        let snapshot = self.finish_migration(&paths, snapshot, &mut notices)?;
        let backup = snapshot.migrated_from.as_ref().map(|from| from.backup.clone()).unwrap_or_default();
        notices.retain(|notice| !matches!(notice, Notice::MigrationFinished { .. }));
        Ok(Migrated { outcome: MigrationOutcome::Migrated { backup }, snapshot: Some(snapshot), notices })
    }

    /// Register a new seal under a new id. A name already in use is a conflict.
    pub fn register(&self, seal: NewSeal) -> Result<Applied, VaultError> {
        let image = SealImage {
            name: seal.name.clone(),
            file: seal.file.clone(),
            width: seal.width,
            height: seal.height,
            data: seal.png.clone(),
            scale: seal.scale,
            effects: seal.effects,
        };
        crate::seal_store::validate_seal(&image).map_err(VaultError::Rejected)?;
        if seal.file.chars().count() > MAX_FILE_CHARS {
            return Err(VaultError::Rejected(RECORD_ERROR.into()));
        }
        let mut created = None;
        let mut applied = self.apply(Budget::General, "register", |next| {
            if next.seals.len() >= MAX_SEALS {
                return Err(OpError::Conflict(Conflict::TooManySeals));
            }
            if next.seals.iter().any(|kept| kept.name == seal.name) {
                return Err(OpError::Conflict(Conflict::DuplicateName));
            }
            let id = new_id("seal")?;
            if next.seal(&id).is_some() || next.is_deleted(&id) {
                return Err(OpError::Rejected(WRITE_ERROR.into()));
            }
            let sha256 = crate::stamps_ui::sha256_hex(&seal.png);
            let mut event = new_event(kind::SEAL_REGISTERED)?;
            event.seal_id = id.clone();
            event.seal_name = seal.name.clone();
            event.seal_revision = 1;
            event.seal_sha256 = sha256.clone();
            next.seals.push(Seal {
                id: id.clone(),
                revision: 1,
                name: seal.name.clone(),
                file: seal.file.clone(),
                width: seal.width,
                height: seal.height,
                data: base64::engine::general_purpose::STANDARD.encode(&seal.png),
                sha256,
                scale: seal.scale,
                effects: seal.effects,
            });
            next.events.push(event);
            created = Some(id);
            Ok(Change::Changed)
        })?;
        applied.created = created;
        Ok(applied)
    }

    /// Change one seal, if it is still at `base_revision`.
    pub fn update(&self, seal_id: &str, base_revision: u64, change: SealChange) -> Result<Applied, VaultError> {
        self.apply(Budget::General, "update", |next| {
            let index = current_seal(next, seal_id, base_revision)?;
            let Some(seal) = next.seals.get_mut(index) else { return Err(OpError::Conflict(Conflict::SealMissing)) };
            let mut changed = seal.clone();
            match change {
                SealChange::Scale(scale) => {
                    changed.scale = crate::stamp_preset::checked_scale(scale).ok_or_else(|| OpError::Rejected("倍率は25%から400%です".into()))?;
                }
                SealChange::Effects(effects) => changed.effects = effects,
            }
            if changed == *seal {
                return Ok(Change::Unchanged);
            }
            changed.revision = changed.revision.checked_add(1).ok_or_else(|| OpError::Rejected(RECORD_ERROR.into()))?;
            check_seal(&changed).map_err(|_| OpError::Rejected("印影の加工設定を確認してください".into()))?;
            let mut event = new_event(kind::SEAL_UPDATED)?;
            event.seal_id = changed.id.clone();
            event.seal_name = changed.name.clone();
            event.seal_revision = changed.revision;
            event.seal_sha256 = changed.sha256.clone();
            *seal = changed;
            next.events.push(event);
            Ok(Change::Changed)
        })
    }

    /// Delete one seal, if it is still at `base_revision`. Its id is kept and never reused.
    pub fn delete(&self, seal_id: &str, base_revision: u64) -> Result<Applied, VaultError> {
        self.apply(Budget::Final, "delete", |next| {
            let index = current_seal(next, seal_id, base_revision)?;
            if index >= next.seals.len() {
                return Err(OpError::Conflict(Conflict::SealMissing));
            }
            let seal = next.seals.remove(index);
            let generation = next.generation.checked_add(1).ok_or_else(|| OpError::Rejected(RECORD_ERROR.into()))?;
            let mut event = new_event(kind::SEAL_DELETED)?;
            event.seal_id = seal.id.clone();
            event.seal_name = seal.name.clone();
            event.seal_revision = seal.revision;
            event.seal_sha256 = seal.sha256.clone();
            next.deleted.push(DeletedSeal { id: seal.id, name: seal.name, sha256: seal.sha256, generation });
            next.events.push(event);
            Ok(Change::Changed)
        })
    }

    /// Record that the window approved `approval`. The seal must still be at that revision and
    /// image. A retry of the same approval writes nothing.
    pub fn record_approval(&self, approval: &ApprovalRef) -> Result<Applied, VaultError> {
        if !id_ok(&approval.op_id, "op") {
            return Err(VaultError::Rejected(RECORD_ERROR.into()));
        }
        self.apply(Budget::General, "record_approval", |next| {
            match next.use_state(&approval.op_id) {
                UseState::Unknown => {}
                UseState::Approved => {
                    return if approval_event(next, &approval.op_id).is_some_and(|event| approval_matches(event, approval)) {
                        Ok(Change::Unchanged)
                    } else {
                        Err(OpError::Conflict(Conflict::OpIdInUse))
                    };
                }
                _ => return Err(OpError::Conflict(Conflict::OpIdInUse)),
            }
            let seal = approved_seal(next, approval)?;
            let mut event = new_event(kind::USE_APPROVED)?;
            event.op_id = approval.op_id.clone();
            event.seal_id = seal.id.clone();
            event.seal_name = seal.name.clone();
            event.seal_revision = seal.revision;
            event.seal_sha256 = seal.sha256.clone();
            next.events.push(event);
            Ok(Change::Changed)
        })
    }

    /// Record that the approved attempt is about to be placed. Refused unless the approval is
    /// recorded and the seal is still the approved revision and image, and unless there is room
    /// left for the committed or cancelled line. The caller places the stamp only after `Ok`.
    pub fn prepare_use(&self, approval: &ApprovalRef, details: &UseDetails) -> Result<Applied, VaultError> {
        check_details(details).map_err(|_| VaultError::Rejected(RECORD_ERROR.into()))?;
        self.apply(Budget::General, "prepare_use", |next| {
            match next.use_state(&approval.op_id) {
                UseState::Unknown => return Err(OpError::Conflict(Conflict::NotApproved)),
                UseState::Approved => {}
                UseState::Prepared => {
                    let same = next
                        .use_events(&approval.op_id)
                        .find(|event| event.kind == kind::USE_PREPARED)
                        .is_some_and(|event| approval_matches(event, approval) && details_match(event, details));
                    return if same { Ok(Change::Unchanged) } else { Err(OpError::Conflict(Conflict::OpIdInUse)) };
                }
                state => return Err(OpError::Conflict(Conflict::UseState(state))),
            }
            if !approval_event(next, &approval.op_id).is_some_and(|event| approval_matches(event, approval)) {
                return Err(OpError::Conflict(Conflict::ApprovalMismatch));
            }
            let seal = approved_seal(next, approval)?;
            let mut event = new_event(kind::USE_PREPARED)?;
            event.op_id = approval.op_id.clone();
            event.seal_id = seal.id.clone();
            event.seal_name = seal.name.clone();
            event.seal_revision = seal.revision;
            event.seal_sha256 = seal.sha256.clone();
            event.image_sha256 = details.image_sha256.clone();
            event.file = details.file.clone();
            event.page = details.page;
            event.note = Some(Note {
                processed: details.processed,
                angle_deg: details.angle_deg,
                fade: details.fade,
                seed: details.seed,
                rotation_mode: details.rotation_mode.clone(),
                ..Note::default()
            });
            let json = serde_json::to_vec(&event).map_err(|_| OpError::Rejected(RECORD_ERROR.into()))?;
            if json.len() > MAX_USE_EVENT_JSON {
                return Err(OpError::Rejected(RECORD_ERROR.into()));
            }
            next.events.push(event);
            Ok(Change::Changed)
        })
    }

    /// Record that the prepared stamp is in the working document. A retry writes nothing.
    pub fn commit_use(&self, op_id: &str) -> Result<Applied, VaultError> {
        self.finish_use(op_id, kind::USE_COMMITTED)
    }

    /// Record that the prepared stamp did not stay in the working document. A retry writes nothing.
    pub fn cancel_use(&self, op_id: &str) -> Result<Applied, VaultError> {
        self.finish_use(op_id, kind::USE_CANCELLED)
    }

    fn finish_use(&self, op_id: &str, finish: &'static str) -> Result<Applied, VaultError> {
        self.apply(Budget::Final, finish, |next| {
            match (next.use_state(op_id), finish) {
                (UseState::Prepared, _) => {}
                (UseState::Committed(_), kind::USE_COMMITTED) | (UseState::Cancelled, kind::USE_CANCELLED) => {
                    return Ok(Change::Unchanged);
                }
                (state, _) => return Err(OpError::Conflict(Conflict::UseState(state))),
            }
            let event = copy_use_event(next, op_id, finish, "")?;
            next.events.push(event);
            Ok(Change::Changed)
        })
    }

    /// Record an undo, redo, removal or undone removal of a committed stamp. `step_id` names this
    /// step, so a retry writes nothing.
    pub fn note_use_step(&self, op_id: &str, step_id: &str, step: UseStep) -> Result<Applied, VaultError> {
        if !id_ok(step_id, "step") {
            return Err(VaultError::Rejected(RECORD_ERROR.into()));
        }
        self.apply(Budget::Final, "note_use_step", |next| {
            if let Some(event) = next.use_events(op_id).find(|event| event.step_id == step_id) {
                return if event.kind == step.kind() { Ok(Change::Unchanged) } else { Err(OpError::Conflict(Conflict::StepIdReused)) };
            }
            let state = next.use_state(op_id);
            if next_state(state, step.kind()).is_none() {
                return Err(OpError::Conflict(Conflict::UseState(state)));
            }
            let event = copy_use_event(next, op_id, step.kind(), step_id)?;
            next.events.push(event);
            Ok(Change::Changed)
        })
    }

    /// The transaction every change goes through.
    fn apply(&self, budget: Budget, kind: &'static str, op: impl FnOnce(&mut Snapshot) -> Result<Change, OpError>) -> Result<Applied, VaultError> {
        let Some(paths) = self.paths(true)? else { return Err(VaultError::Io(FOLDER_ERROR)) };
        let _lock = self.lock(&paths)?;
        let mut notices = Vec::new();
        let (current, existed) = match self.open_locked(&paths, &mut notices)? {
            Opened::Ready(snapshot) => (*snapshot, true),
            Opened::Empty => (Snapshot::empty()?, false),
        };
        let before = current.events.len();
        let mut base = current.clone();
        if existed {
            base.events.extend(self.unrecorded_legacy(&paths, &current)?);
        }
        let records_legacy = base.events.len() > before;
        let mut next = base.clone();
        let budget = match op(&mut next) {
            Ok(Change::Changed) => budget,
            // A set-aside still has to be recorded. If that write fails the caller gets the error,
            // not a success that hides the missing line.
            Ok(Change::Unchanged) if records_legacy => {
                next = base;
                Budget::Final
            }
            Ok(Change::Unchanged) => {
                let size = file_len(&paths.v2);
                notices.extend(self.warnings(&current, size));
                return Ok(Applied { snapshot: current, changed: false, created: None, notices });
            }
            // The set-aside line, if any, is recorded by the next operation that succeeds.
            Err(OpError::Conflict(reason)) => return Err(VaultError::Conflict { reason, latest: Box::new(current) }),
            Err(OpError::Rejected(message)) => return Err(VaultError::Rejected(message)),
            Err(OpError::Vault(error)) => return Err(error),
        };
        next.generation = current.generation.checked_add(1).ok_or_else(|| VaultError::Rejected(RECORD_ERROR.into()))?;
        let generation = next.generation;
        for event in next.events.iter_mut().skip(before) {
            event.seq = generation;
        }
        let size = self.store(&paths, &next, existed, budget, kind)?;
        if !existed {
            self.ensure_tombstone(&paths, &mut notices);
        }
        notices.extend(self.warnings(&next, size));
        Ok(Applied { snapshot: next, changed: true, created: None, notices })
    }

    fn warnings(&self, snapshot: &Snapshot, size: u64) -> Vec<Notice> {
        let mut notices = Vec::new();
        if snapshot.events.len() > self.limits.warn_events {
            notices.push(Notice::ManyEvents { events: snapshot.events.len() });
        }
        if size > self.limits.warn_file_bytes {
            notices.push(Notice::NearFull { bytes: size, limit: self.limits.max_file_bytes });
        }
        notices
    }

    /// Largest file a write under `budget` may leave, keeping room for every open use's final line.
    fn file_limit(&self, snapshot: &Snapshot, budget: Budget) -> u64 {
        let reserved = self.limits.reserve_per_open_use.saturating_mul(snapshot.open_use_count());
        let headroom = if budget == Budget::General { self.limits.step_headroom } else { 0 };
        self.limits.max_file_bytes.saturating_sub(reserved).saturating_sub(headroom)
    }

    /// Encode, check the size, write, and read the file back. `existed` picks atomic replacement
    /// of the current file or creation of a new one that replaces nothing.
    fn store(&self, paths: &Paths, snapshot: &Snapshot, existed: bool, budget: Budget, op: &'static str) -> Result<u64, VaultError> {
        validate(snapshot).map_err(|_| VaultError::Rejected(RECORD_ERROR.into()))?;
        let bytes = encode(snapshot)?;
        let size = bytes.len() as u64;
        if size > self.file_limit(snapshot, budget) {
            return Err(VaultError::Full { limit: self.limits.max_file_bytes });
        }
        self.at(Point::BeforeWrite)?;
        #[cfg(test)]
        if let Some(stage) = self.take_fail_stage() {
            io_note::record(stage, "Io", op, snapshot.generation, size, None, None);
            return Err(VaultError::Io(WRITE_ERROR));
        }
        if existed {
            let mut last_stage = pdfcraft_platform::staging::AtomicWriteStage::Create;
            if let Err(_error) = with_retry(|| {
                match pdfcraft_platform::staging::write_atomically_detailed(
                    &paths.v2,
                    &bytes,
                    pdfcraft_platform::staging::StagingName::TagThenSuffix,
                ) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        last_stage = error.stage;
                        Err(error.source)
                    }
                }
            }) {
                io_note::record_io(last_stage.as_str(), "Io", op, snapshot.generation, size, &_error);
                return Err(VaultError::Io(WRITE_ERROR));
            }
        } else if let Err(error) = create_new_file(&paths.v2, &bytes, snapshot.generation, op) {
            return Err(error);
        }
        self.at(Point::AfterWrite)?;
        let back = match read_opt(&paths.v2) {
            Ok(Some(back)) => back,
            Ok(None) => {
                io_note::record("reread", "Verify", op, snapshot.generation, size, None, None);
                return Err(VaultError::Verify);
            }
            Err(error) => {
                io_note::record("reread", "Verify", op, snapshot.generation, size, None, None);
                return Err(error);
            }
        };
        if back != bytes {
            io_note::record("verify_bytes", "Verify", op, snapshot.generation, size, Some(back.len() as u64), None);
            return Err(VaultError::Verify);
        }
        match decode(&back) {
            Ok(got) if got == *snapshot => Ok(size),
            Ok(_) => {
                io_note::record("verify_decode", "Verify", op, snapshot.generation, size, Some(back.len() as u64), None);
                Err(VaultError::Verify)
            }
            Err(_) => {
                io_note::record("verify_undecodable", "Verify", op, snapshot.generation, size, Some(back.len() as u64), None);
                Err(VaultError::Verify)
            }
        }
    }

    /// Read the newest store under the lock, finishing an interrupted migration and moving aside a
    /// format-1 file written after migration. Neither changes `seals.v2.bin`, except that a
    /// migration whose source changed mid-way is rebuilt from the moved file.
    fn open_locked(&self, paths: &Paths, notices: &mut Vec<Notice>) -> Result<Opened, VaultError> {
        let legacy = read_legacy(&paths.legacy)?;
        let Some(bytes) = read_opt(&paths.v2)? else {
            return match legacy {
                Legacy::Absent => Ok(Opened::Empty),
                Legacy::Tombstone => Err(VaultError::V2Missing),
                Legacy::Swseal1(bytes) => {
                    let library = crate::seal_store::decode_file(&bytes).map_err(|_| VaultError::Unreadable(LEGACY_ERROR))?;
                    Err(VaultError::NeedsMigration { seals: library.seals.len(), events: library.history.len() })
                }
                Legacy::Other => Err(VaultError::UnknownLegacyFile),
            };
        };
        let snapshot = decode(&bytes)?;
        let snapshot = match legacy {
            Legacy::Tombstone => snapshot,
            Legacy::Absent => {
                self.ensure_tombstone(paths, notices);
                snapshot
            }
            Legacy::Swseal1(legacy_bytes) => {
                if self.unfinished_migration(paths, &snapshot, &legacy_bytes) {
                    self.finish_migration(paths, snapshot, notices)?
                } else {
                    self.set_aside_legacy(paths, &snapshot, &legacy_bytes, notices)?;
                    snapshot
                }
            }
            Legacy::Other => {
                notices.push(Notice::UnknownLegacyFile);
                snapshot
            }
        };
        Ok(Opened::Ready(Box::new(snapshot)))
    }

    fn unfinished_migration(&self, paths: &Paths, snapshot: &Snapshot, legacy: &[u8]) -> bool {
        snapshot.migrated_from.as_ref().is_some_and(|from| {
            snapshot.generation == 1
                && from.source_sha256 == crate::stamps_ui::sha256_hex(legacy)
                && paths.dir.join(&from.backup).symlink_metadata().is_err()
        })
    }

    /// Steps 6 to 8 of the migration: move `seals.bin` to its backup name, check what was moved,
    /// and create the tombstone.
    fn finish_migration(&self, paths: &Paths, snapshot: Snapshot, notices: &mut Vec<Notice>) -> Result<Snapshot, VaultError> {
        let from = snapshot.migrated_from.clone().ok_or(VaultError::Verify)?;
        let backup = paths.dir.join(&from.backup);
        move_to_new_name(&paths.legacy, &backup)?;
        self.at(Point::AfterRename)?;
        let moved = read_opt(&backup)?.ok_or(VaultError::Io(READ_ERROR))?;
        let moved_sha = crate::stamps_ui::sha256_hex(&moved);
        let mut snapshot = snapshot;
        let mut backup_name = from.backup.clone();
        if moved_sha != from.source_sha256 {
            // An older version wrote `seals.bin` between reading it and moving it. The moved file
            // is the newest format-1 data, so the migration is rebuilt from it.
            let name = migration_backup_name(&moved_sha);
            move_to_new_name(&backup, &paths.dir.join(&name))?;
            let next_generation = snapshot.generation.checked_add(1).ok_or(VaultError::Verify)?;
            let mut rebuilt = build_from_v1(&moved, Some(&snapshot.store_id), next_generation)?;
            if let Some(from) = rebuilt.migrated_from.as_mut() {
                from.backup = name.clone();
            }
            if let Some(event) = rebuilt.events.iter_mut().find(|event| event.kind == kind::MIGRATED)
                && let Some(note) = event.note.as_mut()
            {
                note.backup = name.clone();
            }
            self.store(paths, &rebuilt, true, Budget::Final, "rebuild")?;
            snapshot = rebuilt;
            backup_name = name;
        }
        self.at(Point::BeforeTombstone)?;
        self.ensure_tombstone(paths, notices);
        notices.push(Notice::MigrationFinished { backup: backup_name });
        Ok(snapshot)
    }

    /// Move a format-1 file written after migration to a new backup name, check it, and put the
    /// tombstone back. `seals.v2.bin` is not changed and the old data is not merged.
    fn set_aside_legacy(&self, paths: &Paths, snapshot: &Snapshot, legacy: &[u8], notices: &mut Vec<Notice>) -> Result<(), VaultError> {
        let sha = crate::stamps_ui::sha256_hex(legacy);
        let dest = unique_backup(&paths.dir, &format!("{LEGACY_BACKUP_PREFIX}g{}-{}", snapshot.generation, sha.get(..16).unwrap_or("")))?;
        move_to_new_name(&paths.legacy, &dest)?;
        let moved = read_opt(&dest)?.ok_or(VaultError::Io(READ_ERROR))?;
        let moved_sha = crate::stamps_ui::sha256_hex(&moved);
        if !moved.starts_with(LEGACY_MAGIC) {
            return Err(VaultError::Io(READ_ERROR));
        }
        self.ensure_tombstone(paths, notices);
        let backup = dest.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        notices.push(Notice::LegacyWriteSetAside { backup, sha256: moved_sha });
        Ok(())
    }

    /// One `legacy_write_set_aside` line for each backup of a late format-1 write that no line
    /// names yet. The backup name is the key, so a retry after a failed write records it once.
    fn unrecorded_legacy(&self, paths: &Paths, snapshot: &Snapshot) -> Result<Vec<Event>, VaultError> {
        let recorded: HashSet<&str> = snapshot
            .events
            .iter()
            .filter(|event| event.kind == kind::LEGACY_WRITE_SET_ASIDE)
            .filter_map(|event| event.note.as_ref().map(|note| note.backup.as_str()))
            .collect();
        let entries = std::fs::read_dir(&paths.dir).map_err(|_| VaultError::Io(FOLDER_ERROR))?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| VaultError::Io(FOLDER_ERROR))?;
            let Ok(name) = entry.file_name().into_string() else { continue };
            if backup_name_ok(&name, LEGACY_BACKUP_PREFIX) && !recorded.contains(name.as_str()) {
                names.push(name);
            }
        }
        names.sort();
        names.truncate(MAX_LEGACY_RECORDS);
        let mut events = Vec::new();
        for name in names {
            let path = paths.dir.join(&name);
            if !path.symlink_metadata().is_ok_and(|meta| meta.is_file()) {
                continue;
            }
            let Some(bytes) = read_opt(&path)? else { continue };
            let mut event = new_event(kind::LEGACY_WRITE_SET_ASIDE)?;
            event.note = Some(Note { backup: name, source_sha256: crate::stamps_ui::sha256_hex(&bytes), ..Note::default() });
            events.push(event);
        }
        Ok(events)
    }

    fn ensure_tombstone(&self, paths: &Paths, notices: &mut Vec<Notice>) {
        if paths.legacy.symlink_metadata().is_ok() {
            return;
        }
        if create_new_plain(&paths.legacy, TOMBSTONE, 0).is_err() && paths.legacy.symlink_metadata().is_err() {
            notices.push(Notice::TombstoneMissing);
        }
    }
}

fn current_seal(snapshot: &Snapshot, seal_id: &str, base_revision: u64) -> Result<usize, OpError> {
    let Some(index) = snapshot.seals.iter().position(|seal| seal.id == seal_id) else {
        let reason = if snapshot.is_deleted(seal_id) { Conflict::SealDeleted } else { Conflict::SealMissing };
        return Err(OpError::Conflict(reason));
    };
    match snapshot.seals.get(index) {
        Some(seal) if seal.revision == base_revision => Ok(index),
        _ => Err(OpError::Conflict(Conflict::SealChanged)),
    }
}

fn approved_seal<'a>(snapshot: &'a Snapshot, approval: &ApprovalRef) -> Result<&'a Seal, OpError> {
    let Some(seal) = snapshot.seal(&approval.seal_id) else {
        let reason = if snapshot.is_deleted(&approval.seal_id) { Conflict::SealDeleted } else { Conflict::SealMissing };
        return Err(OpError::Conflict(reason));
    };
    if seal.revision != approval.seal_revision || seal.sha256 != approval.seal_sha256 {
        return Err(OpError::Conflict(Conflict::SealChanged));
    }
    Ok(seal)
}

fn approval_event<'a>(snapshot: &'a Snapshot, op_id: &'a str) -> Option<&'a Event> {
    snapshot.use_events(op_id).find(|event| event.kind == kind::USE_APPROVED)
}

fn approval_matches(event: &Event, approval: &ApprovalRef) -> bool {
    event.op_id == approval.op_id
        && event.seal_id == approval.seal_id
        && event.seal_revision == approval.seal_revision
        && event.seal_sha256 == approval.seal_sha256
}

fn details_match(event: &Event, details: &UseDetails) -> bool {
    let note = event.note.clone().unwrap_or_default();
    event.file == details.file
        && event.page == details.page
        && event.image_sha256 == details.image_sha256
        && note.processed == details.processed
        && note.angle_deg == details.angle_deg
        && note.fade == details.fade
        && note.seed == details.seed
        && note.rotation_mode == details.rotation_mode
}

fn check_details(details: &UseDetails) -> Result<(), ()> {
    let mark = crate::seal_store::SealEvent {
        at: String::new(),
        file: details.file.clone(),
        page: details.page,
        seal: String::new(),
        event: String::new(),
        processed: details.processed,
        angle_deg: details.angle_deg,
        fade: details.fade,
        seed: details.seed,
        image_sha256: details.image_sha256.clone(),
        rotation_mode: details.rotation_mode.clone(),
    };
    if !hex64(&details.image_sha256) || !crate::seal_store::event_mark_ok(&mark) || !text_ok(&details.file, MAX_FILE_CHARS) {
        return Err(());
    }
    Ok(())
}

/// A new line for `op_id` that repeats the prepared line's seal and placement.
fn copy_use_event(snapshot: &Snapshot, op_id: &str, event_kind: &str, step_id: &str) -> Result<Event, OpError> {
    let prepared =
        snapshot.use_events(op_id).find(|event| event.kind == kind::USE_PREPARED).ok_or(OpError::Conflict(Conflict::UseState(UseState::Unknown)))?;
    let mut event = new_event(event_kind)?;
    event.op_id = op_id.to_owned();
    event.step_id = step_id.to_owned();
    event.seal_id = prepared.seal_id.clone();
    event.seal_name = prepared.seal_name.clone();
    event.seal_revision = prepared.seal_revision;
    event.seal_sha256 = prepared.seal_sha256.clone();
    event.image_sha256 = prepared.image_sha256.clone();
    event.file = prepared.file.clone();
    event.page = prepared.page;
    event.note = prepared.note.clone();
    Ok(event)
}

fn new_event(event_kind: &str) -> Result<Event, VaultError> {
    Ok(Event {
        id: new_id("ev")?,
        seq: 0,
        at: crate::seal_store::utc_now(),
        kind: event_kind.to_owned(),
        op_id: String::new(),
        step_id: String::new(),
        seal_id: String::new(),
        seal_name: String::new(),
        seal_revision: 0,
        seal_sha256: String::new(),
        image_sha256: String::new(),
        file: String::new(),
        page: 0,
        note: None,
    })
}

/// `<prefix>-` and 32 lowercase hex digits from the operating system's random source.
pub fn new_id(prefix: &str) -> Result<String, VaultError> {
    let bytes = random_bytes()?;
    let mut id = String::with_capacity(prefix.len() + 33);
    id.push_str(prefix);
    id.push('-');
    for byte in bytes {
        id.push_str(&format!("{byte:02x}"));
    }
    Ok(id)
}

#[cfg(not(target_arch = "wasm32"))]
fn random_bytes() -> Result<[u8; 16], VaultError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| VaultError::Io("乱数を取得できませんでした。"))?;
    Ok(bytes)
}

/// The web build has no folder for the store, so no id is ever needed there.
#[cfg(target_arch = "wasm32")]
fn random_bytes() -> Result<[u8; 16], VaultError> {
    Err(VaultError::Io("この環境では理事長印を使えません。"))
}

fn id_ok(id: &str, prefix: &str) -> bool {
    id.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}

fn hex64(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn text_ok(text: &str, max_chars: usize) -> bool {
    text.chars().count() <= max_chars && !text.chars().any(char::is_control)
}

fn backup_name_ok(name: &str, prefix: &str) -> bool {
    name.starts_with(prefix)
        && name.ends_with(".bak")
        && name.len() <= MAX_BACKUP_CHARS
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
}

fn check_seal(seal: &Seal) -> Result<(), ()> {
    if !id_ok(&seal.id, "seal") || seal.revision == 0 || !hex64(&seal.sha256) {
        return Err(());
    }
    if seal.data.len() > MAX_SEAL_BYTES.saturating_mul(4) / 3 + 4 {
        return Err(());
    }
    let png = seal.png().ok_or(())?;
    if crate::stamps_ui::sha256_hex(&png) != seal.sha256 {
        return Err(());
    }
    let image = SealImage {
        name: seal.name.clone(),
        file: seal.file.clone(),
        width: seal.width,
        height: seal.height,
        data: png,
        scale: seal.scale,
        effects: seal.effects,
    };
    crate::seal_store::validate_seal(&image).map_err(|_| ())
}

/// Every rule a file must meet to be read, and a snapshot to be written.
fn validate(snapshot: &Snapshot) -> Result<(), ()> {
    if snapshot.schema != SCHEMA || !id_ok(&snapshot.store_id, "store") || snapshot.generation == 0 {
        return Err(());
    }
    if let Some(from) = &snapshot.migrated_from
        && (from.format != "SWSEAL1"
            || !hex64(&from.source_sha256)
            || !backup_name_ok(&from.backup, "seals.bin.swseal1-")
            || !text_ok(&from.at, MAX_AT_CHARS))
    {
        return Err(());
    }
    if snapshot.seals.len() > MAX_SEALS {
        return Err(());
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for seal in &snapshot.seals {
        check_seal(seal)?;
        if !ids.insert(seal.id.as_str()) || !names.insert(seal.name.as_str()) {
            return Err(());
        }
    }
    for deleted in &snapshot.deleted {
        if !id_ok(&deleted.id, "seal") || !hex64(&deleted.sha256) || deleted.generation == 0 || deleted.generation > snapshot.generation {
            return Err(());
        }
        if !ids.insert(deleted.id.as_str()) {
            return Err(());
        }
    }
    let mut event_ids = HashSet::new();
    let mut states: HashMap<&str, UseState> = HashMap::new();
    let mut steps = HashSet::new();
    let mut last_seq = 0;
    for event in &snapshot.events {
        if !id_ok(&event.id, "ev") || !event_ids.insert(event.id.as_str()) {
            return Err(());
        }
        if event.seq == 0 || event.seq < last_seq || event.seq > snapshot.generation {
            return Err(());
        }
        last_seq = event.seq;
        check_event(event)?;
        if is_use_kind(&event.kind) {
            let state = states.entry(event.op_id.as_str()).or_insert(UseState::Unknown);
            *state = next_state(*state, &event.kind).ok_or(())?;
            if is_step_kind(&event.kind) && !steps.insert((event.op_id.as_str(), event.step_id.as_str())) {
                return Err(());
            }
        }
    }
    Ok(())
}

fn check_event(event: &Event) -> Result<(), ()> {
    if let Some(legacy) = event.kind.strip_prefix(kind::LEGACY_PREFIX) {
        // Format-1 lines keep their text as the format-1 reader accepted it, hash case included.
        let legacy_sha_ok =
            event.image_sha256.is_empty() || (event.image_sha256.len() == 64 && event.image_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
        return if LEGACY_EVENTS.contains(&legacy) && event.op_id.is_empty() && event.step_id.is_empty() && event.seq == 1 && legacy_sha_ok {
            Ok(())
        } else {
            Err(())
        };
    }
    let sha_ok = |text: &str| text.is_empty() || hex64(text);
    if !sha_ok(&event.seal_sha256) || !sha_ok(&event.image_sha256) {
        return Err(());
    }
    if !text_ok(&event.at, MAX_AT_CHARS) || !text_ok(&event.seal_name, MAX_NAME_CHARS) || !text_ok(&event.file, MAX_FILE_CHARS) {
        return Err(());
    }
    if let Some(note) = &event.note {
        let mode_ok = matches!(note.rotation_mode.as_str(), "" | "manual" | "random");
        let numbers_ok =
            note.angle_deg.is_finite() && note.angle_deg.abs() <= 30.0 + 1.0e-6 && note.fade.is_finite() && (0.0..=30.0).contains(&note.fade);
        let backup_ok = note.backup.is_empty() || backup_name_ok(&note.backup, "seals.bin.");
        if !mode_ok || !numbers_ok || !backup_ok || !sha_ok(&note.source_sha256) {
            return Err(());
        }
    }
    match event.kind.as_str() {
        kind::MIGRATED => (event.op_id.is_empty() && event.seq == 1).then_some(()).ok_or(()),
        kind::LEGACY_WRITE_SET_ASIDE => {
            let named = event.note.as_ref().is_some_and(|note| backup_name_ok(&note.backup, LEGACY_BACKUP_PREFIX) && hex64(&note.source_sha256));
            (named && event.op_id.is_empty() && event.step_id.is_empty() && event.seal_id.is_empty()).then_some(()).ok_or(())
        }
        kind::SEAL_REGISTERED | kind::SEAL_UPDATED | kind::SEAL_DELETED => {
            (id_ok(&event.seal_id, "seal") && event.op_id.is_empty() && event.seal_revision > 0).then_some(()).ok_or(())
        }
        kind => {
            if !is_use_kind(kind) || !id_ok(&event.op_id, "op") || !id_ok(&event.seal_id, "seal") || !hex64(&event.seal_sha256) {
                return Err(());
            }
            let step_ok = if is_step_kind(kind) { id_ok(&event.step_id, "step") } else { event.step_id.is_empty() };
            let image_ok = kind == kind::USE_APPROVED || hex64(&event.image_sha256);
            (step_ok && image_ok && event.seal_revision > 0).then_some(()).ok_or(())
        }
    }
}

fn encode(snapshot: &Snapshot) -> Result<Vec<u8>, VaultError> {
    let plain = serde_json::to_vec(snapshot).map_err(|_| VaultError::Io(WRITE_ERROR))?;
    if plain.len() as u64 > HARD_READ_CAP {
        return Err(VaultError::Full { limit: HARD_READ_CAP });
    }
    let protected = pdfcraft_platform::dpapi::protect_user(&plain).map_err(VaultError::Io)?;
    let mut bytes = Vec::with_capacity(MAGIC.len().saturating_add(protected.len()));
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&protected);
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<Snapshot, VaultError> {
    let blob = bytes.strip_prefix(MAGIC).filter(|blob| !blob.is_empty()).ok_or(VaultError::Unreadable(DECRYPT_ERROR))?;
    let plain = pdfcraft_platform::dpapi::unprotect_user(blob).map_err(|_| VaultError::Unreadable(DECRYPT_ERROR))?;
    if plain.len() as u64 > HARD_READ_CAP {
        return Err(VaultError::Unreadable(CONTENT_ERROR));
    }
    let snapshot: Snapshot = serde_json::from_slice(&plain).map_err(|_| VaultError::Unreadable(CONTENT_ERROR))?;
    validate(&snapshot).map_err(|_| VaultError::Unreadable(CONTENT_ERROR))?;
    Ok(snapshot)
}

/// The format-1 file as a format-2 store at `generation`, for migration.
fn build_from_v1(bytes: &[u8], store_id: Option<&str>, generation: u64) -> Result<Snapshot, VaultError> {
    let library = crate::seal_store::decode_file(bytes).map_err(|_| VaultError::Unreadable(LEGACY_ERROR))?;
    let source_sha256 = crate::stamps_ui::sha256_hex(bytes);
    let backup = migration_backup_name(&source_sha256);
    let at = crate::seal_store::utc_now();
    let mut seals = Vec::new();
    for seal in &library.seals {
        seals.push(Seal {
            id: new_id("seal")?,
            revision: 1,
            name: seal.name.clone(),
            file: seal.file.clone(),
            width: seal.width,
            height: seal.height,
            data: base64::engine::general_purpose::STANDARD.encode(&seal.data),
            sha256: crate::stamps_ui::sha256_hex(&seal.data),
            scale: seal.scale,
            effects: seal.effects,
        });
    }
    let mut events = Vec::new();
    for line in &library.history {
        let mut event = new_event(&format!("{}{}", kind::LEGACY_PREFIX, line.event))?;
        event.seq = 1;
        event.at = line.at.clone();
        event.seal_id = seals.iter().find(|seal| seal.name == line.seal).map(|seal| seal.id.clone()).unwrap_or_default();
        event.seal_name = line.seal.clone();
        event.image_sha256 = line.image_sha256.clone();
        event.file = line.file.clone();
        event.page = line.page;
        event.note = Some(Note {
            processed: line.processed,
            angle_deg: line.angle_deg,
            fade: line.fade,
            seed: line.seed,
            rotation_mode: line.rotation_mode.clone(),
            ..Note::default()
        });
        events.push(event);
    }
    let mut migrated = new_event(kind::MIGRATED)?;
    migrated.seq = 1;
    migrated.note = Some(Note { backup: backup.clone(), source_sha256: source_sha256.clone(), ..Note::default() });
    events.push(migrated);
    let store_id = match store_id {
        Some(id) => id.to_owned(),
        None => new_id("store")?,
    };
    let snapshot = Snapshot {
        schema: SCHEMA,
        store_id,
        generation,
        migrated_from: Some(Migration {
            format: "SWSEAL1".into(),
            source_sha256,
            source_seals: library.seals.len(),
            source_events: library.history.len(),
            backup,
            at,
        }),
        seals,
        deleted: Vec::new(),
        events,
    };
    validate(&snapshot).map_err(|_| VaultError::Unreadable(LEGACY_ERROR))?;
    Ok(snapshot)
}

fn migration_backup_name(sha: &str) -> String {
    format!("seals.bin.swseal1-{}.bak", sha.get(..16).unwrap_or(""))
}

/// The first `<base>.bak`, `<base>-2.bak`, ... that does not exist.
fn unique_backup(dir: &Path, base: &str) -> Result<PathBuf, VaultError> {
    for n in 1..=MAX_BACKUP_TRIES {
        let name = if n == 1 { format!("{base}.bak") } else { format!("{base}-{n}.bak") };
        let path = dir.join(name);
        if path.symlink_metadata().is_err() {
            return Ok(path);
        }
    }
    Err(VaultError::Io(BACKUP_EXISTS))
}

/// Read a file of at most [`HARD_READ_CAP`] bytes. `None` when it does not exist. A link or a
/// folder in its place is an error, not followed.
fn read_opt(path: &Path) -> Result<Option<Vec<u8>>, VaultError> {
    let meta = match path.symlink_metadata() {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(VaultError::Io(READ_ERROR)),
    };
    if !meta.is_file() || meta.len() > HARD_READ_CAP {
        return Err(VaultError::Unreadable(READ_ERROR));
    }
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(VaultError::Io(READ_ERROR)),
    }
}

fn read_legacy(path: &Path) -> Result<Legacy, VaultError> {
    Ok(match read_opt(path)? {
        None => Legacy::Absent,
        Some(bytes) if bytes.starts_with(TOMBSTONE_MAGIC) => Legacy::Tombstone,
        Some(bytes) if bytes.starts_with(LEGACY_MAGIC) => Legacy::Swseal1(bytes),
        Some(_) => Legacy::Other,
    })
}

fn file_len(path: &Path) -> u64 {
    path.symlink_metadata().map(|meta| meta.len()).unwrap_or(0)
}

/// Rename `from` to `to`, refusing when `to` exists. Only called under the folder lock, and no
/// SwiftPDF writes these backup names, so nothing appears at `to` between the check and the move.
fn move_to_new_name(from: &Path, to: &Path) -> Result<(), VaultError> {
    if to.symlink_metadata().is_ok() {
        return Err(VaultError::Io(BACKUP_EXISTS));
    }
    with_retry(|| std::fs::rename(from, to)).map_err(|_| VaultError::Io(WRITE_ERROR))
}

/// Windows refuses to rename or replace a file that another program has open without delete
/// sharing, such as a virus scan of the file just written, with "access denied" or a sharing
/// violation. Those errors are retried for about two seconds in all; any other error, or one that
/// lasts, is returned. A failed attempt leaves the target as it was.
fn with_retry<T>(mut attempt: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    const DELAYS_MS: [u64; 8] = [20, 40, 80, 160, 320, 500, 500, 500];
    let mut delays = DELAYS_MS.iter();
    loop {
        match attempt() {
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied || matches!(error.raw_os_error(), Some(32 | 33)) => {
                match delays.next() {
                    Some(ms) => std::thread::sleep(Duration::from_millis(*ms)),
                    None => return Err(error),
                }
            }
            result => return result,
        }
    }
}

/// Publish a new `seals.v2.bin` without replacing anything: by hard link from a finished staging
/// file, or, on a volume without hard links, by writing it with `create_new`.
fn create_new_file(path: &Path, bytes: &[u8], generation: u64, op: &'static str) -> Result<(), VaultError> {
    match pdfcraft_platform::save_path::write_new_file(None, path, bytes) {
        Ok(()) => Ok(()),
        Err(pdfcraft_platform::save_path::NewFileError::LinkUnavailable(_)) => create_new_plain(path, bytes, generation, op),
        Err(_error) => {
            match &_error {
                pdfcraft_platform::save_path::NewFileError::Io(io) => {
                    io_note::record_io("create", "Io", op, generation, bytes.len() as u64, io);
                }
                other => io_note::record(
                    match other {
                        pdfcraft_platform::save_path::NewFileError::DestinationExists => "create",
                        pdfcraft_platform::save_path::NewFileError::SameAsOriginal => "create",
                        pdfcraft_platform::save_path::NewFileError::Unresolved(_) => "create",
                        pdfcraft_platform::save_path::NewFileError::LinkUnavailable(_) => "create",
                        pdfcraft_platform::save_path::NewFileError::Io(_) => "create",
                    },
                    "Io",
                    op,
                    generation,
                    bytes.len() as u64,
                    None,
                    None,
                ),
            }
            Err(VaultError::Io(WRITE_ERROR))
        }
    }
}

fn create_new_plain(path: &Path, bytes: &[u8], generation: u64, op: &'static str) -> Result<(), VaultError> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(_error) => {
            io_note::record_io("create", "Io", op, generation, bytes.len() as u64, &_error);
            return Err(VaultError::Io(WRITE_ERROR));
        }
    };
    if let Err(_error) = file.write_all(bytes) {
        io_note::record_io("write", "Io", op, generation, bytes.len() as u64, &_error);
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(VaultError::Io(WRITE_ERROR));
    }
    if let Err(_error) = file.sync_all() {
        io_note::record_io("sync", "Io", op, generation, bytes.len() as u64, &_error);
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(VaultError::Io(WRITE_ERROR));
    }
    Ok(())
}

#[cfg(test)]
#[path = "seal_store_v2_tests.rs"]
mod tests;
