//! Reload a document when its file changes on disk (#431): a LaTeX or Typst build rewriting the
//! PDF that is open shows its new version without reopening it.
//!
//! Cheap by design. One worker thread looks at each open file's metadata (no reading) every
//! [`IDLE`], and blocks without waking at all while no open document has a file. A changed file
//! is read only once it has stopped changing ([`SETTLE`] apart) and ends like a whole PDF: its
//! last KiB is read first, so a file still being written costs one small read. The new bytes go
//! to the UI thread, which is woken for them: a clean document is reloaded in place, keeping its
//! tab, page, zoom and scroll; one with unsaved changes gets a notice instead. PdfCraft's own
//! saves are recognised by the file's stamp after writing, so they never reload.

use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use pdfcraft_engine::DocId;

use crate::PdfCraftApp;

/// How often an unchanged file is looked at.
pub const IDLE: Duration = Duration::from_millis(250);
/// How long a changed file must stay the same before it is read: writers pause between chunks.
pub const SETTLE: Duration = Duration::from_millis(40);
/// How much of the end of a changed file is read to see whether it is a whole PDF.
const TAIL: usize = 1024;

/// One version of a file, told apart without reading it: its length and modification time, and
/// what the platform adds (Unix: device, inode and change time; Windows: creation and last-write
/// times in 100 ns ticks). The modification time alone is coarse on FAT and some network
/// drives, and a rebuild can keep the length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    extra: [u64; 4],
}

impl Stamp {
    fn of(meta: &Metadata) -> Self {
        #[cfg(unix)]
        let extra = {
            use std::os::unix::fs::MetadataExt;
            [meta.dev(), meta.ino(), meta.ctime().cast_unsigned(), meta.ctime_nsec().cast_unsigned()]
        };
        #[cfg(windows)]
        let extra = {
            use std::os::windows::fs::MetadataExt;
            [meta.creation_time(), meta.last_write_time(), 0, 0]
        };
        #[cfg(not(any(unix, windows)))]
        let extra = [0; 4];
        Self { len: meta.len(), modified: meta.modified().ok(), extra }
    }

    /// The file at `path` now; `None` when it is missing, not a file or can't be read.
    pub fn at(path: &str) -> Option<Self> {
        std::fs::metadata(path).ok().filter(Metadata::is_file).map(|m| Self::of(&m))
    }
}

/// A file to watch, as the UI thread sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Watched {
    pub doc: DocId,
    pub path: String,
    /// The version PdfCraft last read or wrote (`None`: the first one the watcher sees).
    pub known: Option<Stamp>,
    /// Which reading or writing of the file by PdfCraft this is for (an open, a save, Save As):
    /// a version the watcher found for an earlier one is older than the document is now.
    pub epoch: u64,
}

/// A new version of a watched file, read and ready to open.
pub struct Changed {
    pub doc: DocId,
    /// The file it was read from, and the epoch it was watched for (see [`Watched`]).
    pub path: String,
    pub epoch: u64,
    pub stamp: Stamp,
    pub bytes: Arc<Vec<u8>>,
}

/// The watcher's record of one file.
struct Entry {
    doc: DocId,
    path: String,
    epoch: u64,
    known: Option<Stamp>,
    /// A changed version seen once, waiting to be seen unchanged again.
    pending: Option<Stamp>,
}

/// Looks at the watched files: on the worker thread, or on the UI thread in tests.
#[derive(Default)]
pub struct Watcher {
    files: Vec<Entry>,
}

impl Watcher {
    /// Watch `list` from now on. A file watched already keeps its pending change; a version the
    /// UI thread knows (it just read or saved the file) replaces the watcher's own.
    fn set(&mut self, list: Vec<Watched>) {
        let old = std::mem::take(&mut self.files);
        self.files = list
            .into_iter()
            .map(|w| {
                let prev = old.iter().find(|e| e.doc == w.doc && e.path == w.path);
                Entry {
                    known: w.known.or(prev.and_then(|e| e.known)),
                    pending: prev.and_then(|e| e.pending),
                    doc: w.doc,
                    path: w.path,
                    epoch: w.epoch,
                }
            })
            .collect();
    }

    /// Look at every file once: the new versions found, and whether a file is still changing
    /// (look again after [`SETTLE`] rather than [`IDLE`]).
    fn tick(&mut self) -> (Vec<Changed>, bool) {
        let mut found = Vec::new();
        let mut settling = false;
        for e in &mut self.files {
            // Missing (deleted, or being replaced) or unreadable: the document stays as it is.
            let Some(now) = Stamp::at(&e.path) else {
                e.pending = None;
                continue;
            };
            let known = *e.known.get_or_insert(now);
            if now == known {
                e.pending = None;
                continue;
            }
            if e.pending != Some(now) {
                // Changed since the last look: read it once it stops changing.
                e.pending = Some(now);
                settling = true;
                continue;
            }
            e.pending = None;
            match read_whole_pdf(&e.path, now) {
                Version::Ready(bytes) => {
                    e.known = Some(now);
                    found.push(Changed { doc: e.doc, path: e.path.clone(), epoch: e.epoch, stamp: now, bytes });
                }
                // Not a whole PDF in this version (a build that failed or paused): wait for the
                // next one.
                Version::Incomplete => e.known = Some(now),
                // Changed again while being read, or held by its writer: look again soon.
                Version::Busy => settling = true,
            }
        }
        (found, settling)
    }
}

/// What a changed file turned out to be.
enum Version {
    /// A whole PDF, read.
    Ready(Arc<Vec<u8>>),
    /// Not a whole PDF: a build that failed or paused.
    Incomplete,
    /// Changing, replaced or held by its writer: look again.
    Busy,
}

/// Read `path` if it is still the version `stamp` and ends like a whole PDF (`%%EOF` in its last
/// KiB, read first). The whole file is then read into a buffer of exactly its size.
fn read_whole_pdf(path: &str, stamp: Stamp) -> Version {
    // Windows refuses to open a file its writer holds exclusively: not ready yet.
    let Ok(mut file) = File::open(path) else { return Version::Busy };
    let Ok(meta) = file.metadata() else { return Version::Busy };
    if Stamp::of(&meta) != stamp {
        return Version::Busy;
    }
    let len = meta.len();
    let mut end = [0u8; TAIL];
    let tail = usize::try_from(len).map_or(TAIL, |l| l.min(TAIL));
    let (Some(end), Ok(back)) = (end.get_mut(..tail), i64::try_from(tail)) else { return Version::Incomplete };
    if file.seek(SeekFrom::End(-back)).is_err() || file.read_exact(end).is_err() {
        return Version::Busy;
    }
    if !end.windows(5).any(|w| w == b"%%EOF") {
        return Version::Incomplete;
    }
    let mut bytes = Vec::new();
    // Too big to hold: the open version stays.
    let Ok(size) = usize::try_from(len) else { return Version::Incomplete };
    if bytes.try_reserve_exact(size).is_err() {
        return Version::Incomplete;
    }
    if file.seek(SeekFrom::Start(0)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return Version::Busy;
    }
    // Still that version, under that name: a writer may have replaced the file meanwhile.
    if bytes.len() != size || Stamp::at(path) != Some(stamp) {
        return Version::Busy;
    }
    Version::Ready(Arc::new(bytes))
}

/// The worker thread: watches the newest list the UI thread sent and hands back new versions,
/// waking the UI for them. It ends when the app (its inbox's sender) is gone.
fn run(inbox: mpsc::Receiver<Vec<Watched>>, out: Arc<Mutex<Vec<Changed>>>, ctx: egui::Context) {
    let mut watcher = Watcher::default();
    // `None`: nothing to watch, so block until there is.
    let mut wait = None;
    loop {
        let message = match wait {
            None => inbox.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(wait) => inbox.recv_timeout(wait),
        };
        match message {
            Ok(list) => watcher.set(list),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(list) = inbox.try_recv() {
            watcher.set(list);
        }
        if watcher.files.is_empty() {
            wait = None;
            continue;
        }
        let (found, settling) = watcher.tick();
        if !found.is_empty() {
            out.lock().unwrap_or_else(PoisonError::into_inner).extend(found);
            ctx.request_repaint();
        }
        wait = Some(if settling { SETTLE } else { IDLE });
    }
}

/// The app's side of the watcher.
#[derive(Default)]
pub struct FileWatch {
    /// The worker's inbox (`None` until a document with a file is open).
    inbox: Option<mpsc::Sender<Vec<Watched>>>,
    found: Arc<Mutex<Vec<Changed>>>,
    /// What the watcher was last told.
    sent: Vec<Watched>,
    /// In tests ([`PdfCraftApp::run_inline`]) there is no thread: each frame looks once.
    inline: Watcher,
    /// The version of each document's file that PdfCraft last read or wrote.
    known: HashMap<DocId, Stamp>,
    /// The version each document came from (its open, reload or last save). Unlike `known`,
    /// a version found on disk and not applied (the document had unsaved changes) is not
    /// recorded here, so Save can tell that the file is not the one the document came from.
    basis: HashMap<DocId, Stamp>,
    /// How many times PdfCraft has read or written each document's file: a version the watcher
    /// found before the latest one is older than the document in memory, and is not applied
    /// (#1802).
    epoch: HashMap<DocId, u64>,
    /// The notice bar's Reload was pressed for this document.
    pub reload_request: Option<DocId>,
}

impl FileWatch {
    /// PdfCraft read or wrote `path` for `doc`: that version is not a change, and anything the
    /// watcher found before it is stale.
    pub fn known_now(&mut self, doc: DocId, path: &str) {
        self.known_as(doc, Stamp::at(path));
    }

    /// Remember `stamp`, read before the file was, for `doc`.
    pub fn known_as(&mut self, doc: DocId, stamp: Option<Stamp>) {
        if let Some(stamp) = stamp {
            self.known.insert(doc, stamp);
            self.basis.insert(doc, stamp);
        }
        self.bump(doc);
    }

    /// Whether the file at `path` is another version than the one `doc` came from: something
    /// else wrote it since the document was opened, reloaded or last saved (#2166). A file
    /// that is missing or can't be looked at, or a document whose version was never recorded,
    /// is not a difference.
    pub fn disk_differs(&self, doc: DocId, path: &str) -> bool {
        match (Stamp::at(path), self.basis.get(&doc)) {
            (Some(now), Some(basis)) => now != *basis,
            _ => false,
        }
    }

    fn bump(&mut self, doc: DocId) {
        let epoch = self.epoch.entry(doc).or_default();
        *epoch = epoch.wrapping_add(1);
    }

    fn epoch(&self, doc: DocId) -> u64 {
        self.epoch.get(&doc).copied().unwrap_or(0)
    }
}

impl PdfCraftApp {
    /// The documents to watch: every open one with a file, while the preference is on.
    fn watched(&self) -> impl Iterator<Item = (DocId, &str, Option<Stamp>, u64)> {
        self.views.iter().filter(|_| self.reload_changed_files).filter_map(|v| {
            let path = self.session.get(v.id)?.path.as_deref()?;
            Some((v.id, path, self.watch.known.get(&v.id).copied(), self.watch.epoch(v.id)))
        })
    }

    /// Every frame: tell the watcher about files opened, closed or saved (without allocating
    /// when nothing changed), and reload what changed on disk.
    pub(crate) fn watch_files(&mut self) {
        if let Some(id) = self.watch.reload_request.take()
            && let Some(i) = self.views.iter().position(|v| v.id == id)
        {
            self.request_reload(i);
        }
        let mut sent = self.watch.sent.iter();
        let unchanged = self
            .watched()
            .all(|(doc, path, known, epoch)| sent.next().is_some_and(|w| w.doc == doc && w.path == path && w.known == known && w.epoch == epoch))
            && sent.next().is_none();
        if !unchanged {
            let list: Vec<Watched> = self.watched().map(|(doc, path, known, epoch)| Watched { doc, path: path.to_owned(), known, epoch }).collect();
            let open: Vec<DocId> = self.views.iter().map(|v| v.id).collect();
            self.watch.known.retain(|doc, _| open.contains(doc));
            self.watch.basis.retain(|doc, _| open.contains(doc));
            self.watch.epoch.retain(|doc, _| open.contains(doc));
            if self.send_to_watcher(list.clone()) {
                self.watch.sent = list;
            }
        }
        if self.run_inline {
            let (found, _) = self.watch.inline.tick();
            self.watch.found.lock().unwrap_or_else(PoisonError::into_inner).extend(found);
        }
        let found = std::mem::take(&mut *self.watch.found.lock().unwrap_or_else(PoisonError::into_inner));
        for c in found {
            self.file_changed(c);
        }
    }

    /// Hand `list` to the watcher, starting it when needed. `false`: not delivered (no context
    /// yet, or the worker couldn't start or ended); the next frame tries again.
    fn send_to_watcher(&mut self, list: Vec<Watched>) -> bool {
        if self.run_inline {
            self.watch.inline.set(list);
            return true;
        }
        if self.watch.inbox.is_none() {
            if list.is_empty() {
                return true;
            }
            let Some(ctx) = self.ctx.clone() else { return false };
            let (tx, rx) = mpsc::channel();
            let out = self.watch.found.clone();
            if std::thread::Builder::new().name("pdfcraft-watch".into()).spawn(move || run(rx, out, ctx)).is_err() {
                return false;
            }
            self.watch.inbox = Some(tx);
        }
        let delivered = self.watch.inbox.as_ref().is_some_and(|tx| tx.send(list).is_ok());
        if !delivered {
            // The worker ended (it panicked): start a new one next frame.
            self.watch.inbox = None;
        }
        delivered
    }

    /// A watched file has a new version: reload its document, or ask first when it has unsaved
    /// changes.
    fn file_changed(&mut self, c: Changed) {
        let Some(i) = self.views.iter().position(|v| v.id == c.doc) else { return };
        // Found for an earlier reading or writing of the file, or for another file than the
        // document's now (a save, Save As or a reopen came between): older than the document in
        // memory, so it is not applied over it (#1802). The watcher was told the newer version.
        let same_file = self.session.get(c.doc).and_then(|d| d.path.as_deref()) == Some(c.path.as_str());
        if !same_file || self.watch.epoch(c.doc) != c.epoch {
            return;
        }
        // PdfCraft wrote this version itself (a save), or it was handled already.
        if self.watch.known.get(&c.doc) == Some(&c.stamp) {
            return;
        }
        self.watch.known.insert(c.doc, c.stamp);
        // A stamp placed but not yet in the PDF would be dropped by the reload, as unsaved work would.
        if self.has_unsaved_work(i) || self.has_pending_place(i) {
            if let Some(view) = self.views.get_mut(i) {
                view.disk_changed = true;
            }
            return;
        }
        self.reload_view(i, c.bytes);
    }

    /// Read the file now and replace the document, unsaved changes and all. The notice bar's
    /// Reload reaches this through [`Self::request_reload`], which asks first.
    pub(crate) fn reload_from_disk(&mut self, doc: DocId) {
        let Some(i) = self.views.iter().position(|v| v.id == doc) else { return };
        let Some(path) = self.session.get(doc).and_then(|d| d.path.clone()) else { return };
        let stamp = Stamp::at(&path);
        match std::fs::read(&path) {
            Ok(bytes) => {
                self.watch.known_as(doc, stamp);
                self.reload_view(i, Arc::new(bytes));
            }
            Err(e) => {
                let name = self.session.get(doc).map(|d| d.name.clone()).unwrap_or_default();
                self.notify_fmt("Couldn't read {name}: {e}", &[("name", &name), ("e", &e.to_string())]);
            }
        }
    }

    /// Open `bytes` in place of tab `i`'s document, keeping the tab and the reader's place. A
    /// version that can't be opened leaves the document as it was, and says so.
    fn reload_view(&mut self, i: usize, bytes: Arc<Vec<u8>>) {
        let Some(id) = self.views.get(i).map(|v| v.id) else { return };
        // Seal work may be writing into this document, and a hold lives on the document object a
        // reload replaces: neither is reloaded. The notice bar stays so it can be done later.
        if self.seals.blocking() || self.session.get(id).is_some_and(|doc| doc.output_hold().is_some()) {
            if let Some(view) = self.views.get_mut(i) {
                view.disk_changed = true;
            }
            return;
        }
        match self.session.reload(id, bytes) {
            Ok(()) => {
                // Whatever was unsaved is gone, so there is nothing left to recover.
                self.forget_recovery(id);
                if let (Some(view), Some(doc)) = (self.views.get_mut(i), self.session.get(id)) {
                    view.document_replaced(&doc.info);
                }
                // What the form's scripts said while it opened, as on opening.
                let out = self.session.take_js_output(id);
                self.handle_js(id, out);
            }
            Err(e) => {
                let name = self.session.get(id).map(|d| d.name.clone()).unwrap_or_default();
                self.notify_fmt("Couldn't reload {name}: {e}", &[("name", &name), ("e", &e.to_string())]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use egui_kittest::Harness;

    use super::*;
    use crate::{CloseRequest, SaveTarget};

    /// An `n`-page PDF with a proper xref table.
    fn fixture(n: usize) -> Vec<u8> {
        let mut objs: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + i)).collect();
        objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")));
        for _ in 0..n {
            objs.push("<< /Type /Page /Parent 2 0 R >>".into());
        }
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

    /// A fresh temporary folder with `thesis.pdf` in it, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(test: &str, bytes: &[u8]) -> (Self, String) {
            let dir = std::env::temp_dir().join(format!("pdfcraft-watch-{test}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("thesis.pdf").to_string_lossy().into_owned();
            std::fs::write(&path, bytes).unwrap();
            (Self(dir), path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Rewrite `path` as a build does, with a later modification time.
    fn rebuild(path: &str, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(60)).unwrap();
    }

    fn harness(path: &str) -> Harness<'static, PdfCraftApp> {
        let path = path.to_owned();
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.run_inline = true;
            app.open_path(&path);
            app
        });
        h.run_steps(4);
        h
    }

    /// Version B of the open file, found by the watcher but not yet applied: queued as the worker
    /// thread queues what it finds between frames.
    fn find_new_version(app: &mut PdfCraftApp, path: &str, bytes: &[u8]) -> Vec<Changed> {
        rebuild(path, bytes);
        // One look sees the change, the next sees it settled and reads it.
        let (mut found, _) = app.watch.inline.tick();
        found.extend(app.watch.inline.tick().0);
        assert_eq!(found.len(), 1, "the watcher found the new version");
        found
    }

    fn doc<'a>(h: &'a Harness<'static, PdfCraftApp>) -> &'a pdfcraft_engine::Document {
        let app = h.state();
        app.session.get(app.views[0].id).unwrap()
    }

    /// #1802: the watcher found version B of the file, then the user saved version C over it
    /// before B was applied. B is older than the document now: it is not applied, the undo history
    /// stays, and C stays on disk.
    #[test]
    fn a_version_found_before_a_save_is_not_applied_over_it() {
        let (_dir, path) = TempDir::new("stale-save", &fixture(2));
        let mut h = harness(&path);
        let stale = find_new_version(h.state_mut(), &path, &fixture(3));
        let app = h.state_mut();
        app.views[0].select_pages(&[0]);
        app.apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 });
        // B is on disk, not the A the document came from: Save asks first (#2166), and the
        // user saves C over it.
        assert!(!app.save_active(SaveTarget::InPlace), "asks before writing over B");
        app.overwrite_changed_file();
        assert!(!doc(&h).dirty, "saved version C");
        let app = h.state_mut();
        app.watch.found.lock().unwrap().extend(stale);
        h.run_steps(3);
        let d = doc(&h);
        assert_eq!(d.info.pages.len(), 2, "the stale version was not applied over the saved one");
        assert!(d.can_undo().is_some() && !d.dirty, "the saved document and its history stay");
        assert_eq!(Stamp::at(&path), h.state().watch.known.get(&h.state().views[0].id).copied(), "C is still on disk");
    }

    /// #1802: version B found under the old path, then Save As to another file: B is not applied
    /// to the document under its new path.
    #[test]
    fn a_version_found_before_save_as_is_not_applied_to_the_new_file() {
        let (dir, path) = TempDir::new("stale-save-as", &fixture(2));
        let mut h = harness(&path);
        let stale = find_new_version(h.state_mut(), &path, &fixture(3));
        let new_path = dir.0.join("copy.pdf").to_string_lossy().into_owned();
        let app = h.state_mut();
        app.views[0].select_pages(&[0]);
        app.apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 });
        app.save_override = Some(new_path.clone());
        assert!(app.save_active(SaveTarget::As), "saved as the new file");
        app.watch.found.lock().unwrap().extend(stale);
        h.run_steps(3);
        let d = doc(&h);
        assert_eq!(d.path.as_deref(), Some(new_path.as_str()));
        assert_eq!(d.info.pages.len(), 2, "the old file's version was not applied to the new one");
        assert!(d.can_undo().is_some() && !d.dirty, "the saved document and its history stay");
    }

    /// A stamp on the page, not yet in the PDF.
    fn placed(chairman: bool) -> crate::stamps_ui::StampPlace {
        crate::stamps_ui::StampPlace {
            page: 0,
            rect: [20.0, 40.0, 80.0, 100.0],
            name: "合成".into(),
            file: "stamp.png".into(),
            data: Arc::new(b"PNG".to_vec()),
            source_page: 0,
            opacity: 1.0,
            chairman,
            pixels: None,
            drag: None,
            scale: 100.0,
            nominal: (40.0, 40.0),
            source: if chairman { crate::stamps_ui::PlaceSource::Chairman(0) } else { crate::stamps_ui::PlaceSource::Custom(0) },
            circular: false,
            date_overlay: None,
            rotation: 0,
            seal: None,
            seal_ref: None,
            preview_gen: 0,
        }
    }

    /// Version B on disk while the document shows A, with the notice bar up. The automatic reload
    /// is off, so only the Reload under test can replace the document.
    fn changed_on_disk(h: &mut Harness<'static, PdfCraftApp>, path: &str) {
        h.state_mut().reload_changed_files = false;
        h.run_steps(1);
        rebuild(path, &fixture(3));
        h.state_mut().views[0].disk_changed = true;
    }

    /// The notice bar's Reload, as a click queues it, then the frames that act on it.
    fn press_reload(h: &mut Harness<'static, PdfCraftApp>) {
        let id = h.state().views[0].id;
        h.state_mut().watch.reload_request = Some(id);
        h.run_steps(2);
    }

    fn pages(h: &Harness<'static, PdfCraftApp>) -> usize {
        doc(h).info.pages.len()
    }

    fn rotate_first_page(h: &mut Harness<'static, PdfCraftApp>) {
        let app = h.state_mut();
        app.views[0].select_pages(&[0]);
        assert!(app.apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }));
        assert!(doc(h).dirty);
    }

    /// Reload with a placed stamp, general or chairman: the placement question comes first, and
    /// 編集に戻る keeps the placement and the document as they were. Pressing Reload again while
    /// it is up asks nothing new.
    #[test]
    fn reload_asks_about_a_placed_stamp_and_return_keeps_it() {
        for chairman in [false, true] {
            let (_dir, path) = TempDir::new(&format!("reload-place-{chairman}"), &fixture(2));
            let mut h = harness(&path);
            h.state_mut().views[0].stamp_place = Some(placed(chairman));
            changed_on_disk(&mut h, &path);
            let id = h.state().views[0].id;
            press_reload(&mut h);
            assert_eq!(h.state().place_close_request, Some(CloseRequest::Reload(id)), "chairman={chairman}");
            assert_eq!(h.state().front_prompt(), Some(crate::PromptLayer::PlaceClose));
            assert_eq!(pages(&h), 2, "not reloaded before the answer");
            press_reload(&mut h);
            assert_eq!(h.state().place_close_request, Some(CloseRequest::Reload(id)));
            assert!(h.state().close_request.is_none(), "a second press asks nothing new");
            let ctx = h.ctx.clone();
            h.state_mut().resolve_place_close(&ctx, None);
            h.run_steps(2);
            assert_eq!(pages(&h), 2, "編集に戻る does not reload");
            assert!(h.state().views[0].stamp_place.is_some(), "and keeps the placement");
            assert!(h.state().views[0].disk_changed, "the notice bar stays");
        }
    }

    /// Discarding the placement on a document with nothing unsaved reloads it.
    #[test]
    fn discarding_the_placement_reloads_a_clean_document() {
        let (_dir, path) = TempDir::new("reload-discard", &fixture(2));
        let mut h = harness(&path);
        h.state_mut().views[0].stamp_place = Some(placed(true));
        changed_on_disk(&mut h, &path);
        press_reload(&mut h);
        let ctx = h.ctx.clone();
        h.state_mut().resolve_place_close(&ctx, Some(true));
        h.run_steps(2);
        assert_eq!(pages(&h), 3, "reloaded");
        assert!(h.state().views[0].stamp_place.is_none());
        assert!(h.state().close_request.is_none() && h.state().place_close_request.is_none());
    }

    /// Nothing placed and nothing unsaved: Reload reloads at once, with no question.
    #[test]
    fn reload_without_a_placement_or_changes_does_not_ask() {
        let (_dir, path) = TempDir::new("reload-plain", &fixture(2));
        let mut h = harness(&path);
        changed_on_disk(&mut h, &path);
        press_reload(&mut h);
        assert_eq!(pages(&h), 3);
        assert!(h.state().close_request.is_none() && h.state().place_close_request.is_none());
    }

    /// A placed stamp and unsaved PDF edits: discarding the placement does not drop the edits.
    /// The save question follows; Cancel keeps everything, Don't save reloads.
    #[test]
    fn unsaved_edits_are_asked_about_after_the_placement() {
        let (_dir, path) = TempDir::new("reload-place-edits", &fixture(2));
        let mut h = harness(&path);
        rotate_first_page(&mut h);
        h.state_mut().views[0].stamp_place = Some(placed(false));
        changed_on_disk(&mut h, &path);
        let id = h.state().views[0].id;
        press_reload(&mut h);
        let ctx = h.ctx.clone();
        h.state_mut().resolve_place_close(&ctx, Some(true));
        h.run_steps(2);
        assert!(h.state().views[0].stamp_place.is_none(), "the placement is gone");
        assert_eq!(h.state().close_request, Some(CloseRequest::Reload(id)), "the edits are asked about");
        assert_eq!(h.state().front_prompt(), Some(crate::PromptLayer::Save));
        assert_eq!(pages(&h), 2);
        assert!(doc(&h).dirty, "the edits are still there");

        h.state_mut().resolve_close(&ctx, None);
        h.run_steps(2);
        assert_eq!(pages(&h), 2, "Cancel does not reload");
        assert!(doc(&h).dirty && doc(&h).can_undo().is_some(), "Cancel keeps the edits and their history");

        press_reload(&mut h);
        assert_eq!(h.state().close_request, Some(CloseRequest::Reload(id)), "asked again, placement already gone");
        h.state_mut().resolve_close(&ctx, Some(false));
        h.run_steps(2);
        assert_eq!(pages(&h), 3, "Don't save reloads");
        assert!(!doc(&h).dirty);
    }

    /// Save chosen before reloading, and the save fails: nothing is reloaded and the edits stay.
    #[test]
    fn a_failed_save_does_not_reload() {
        let (dir, path) = TempDir::new("reload-save-fails", &fixture(2));
        let mut h = harness(&path);
        rotate_first_page(&mut h);
        changed_on_disk(&mut h, &path);
        press_reload(&mut h);
        let id = h.state().views[0].id;
        assert_eq!(h.state().close_request, Some(CloseRequest::Reload(id)));
        let unwritable = dir.0.join("no-such-folder").join("copy.pdf").to_string_lossy().into_owned();
        h.state_mut().save_override = Some(unwritable);
        let ctx = h.ctx.clone();
        h.state_mut().resolve_close(&ctx, Some(true));
        h.run_steps(3);
        assert_eq!(pages(&h), 2, "not reloaded after a failed save");
        assert!(doc(&h).dirty && doc(&h).can_undo().is_some(), "the edits stay");
    }

    /// A held document keeps its hold: neither Reload nor a new version on disk replaces it,
    /// which would open a fresh document object without the hold.
    #[test]
    fn a_held_document_is_not_reloaded() {
        let (_dir, path) = TempDir::new("reload-held", &fixture(2));
        let mut h = harness(&path);
        let id = h.state().views[0].id;
        h.state_mut().session.hold_output(id, "合成の要復旧".into()).unwrap();
        changed_on_disk(&mut h, &path);
        press_reload(&mut h);
        assert_eq!(pages(&h), 2);
        assert_eq!(doc(&h).output_hold(), Some("合成の要復旧"), "the hold stays");
        let toast = h.state().toast.as_ref().map(|(text, _)| text.clone()).unwrap_or_default();
        assert!(toast.contains("合成の要復旧"), "says why: {toast}");
        // The watcher finding the new version on a clean held document does not reload it either.
        h.state_mut().views[0].disk_changed = false;
        h.state_mut().reload_changed_files = true;
        h.run_steps(6);
        assert_eq!(pages(&h), 2);
        assert_eq!(doc(&h).output_hold(), Some("合成の要復旧"));
        assert!(h.state().views[0].disk_changed, "the notice bar says the file changed");
    }

    /// A stamp placed but not yet in the PDF is unfinished work: a new version on disk raises the
    /// notice bar instead of reloading the document from under it.
    #[test]
    fn a_new_version_does_not_drop_a_pending_stamp() {
        let (_dir, path) = TempDir::new("pending-stamp", &fixture(2));
        let mut h = harness(&path);
        h.state_mut().views[0].stamp_place = Some(placed(true));
        let found = find_new_version(h.state_mut(), &path, &fixture(3));
        h.state_mut().watch.found.lock().unwrap().extend(found);
        h.run_steps(3);
        assert_eq!(doc(&h).info.pages.len(), 2, "not reloaded under the placement");
        let view = &h.state().views[0];
        assert!(view.stamp_place.is_some(), "the placement stays");
        assert!(view.disk_changed, "the notice bar says the file changed");
    }
}
