//! Chairman seals, stored outside the custom-stamp library.
//!
//! The file is DPAPI ciphertext for the current Windows user ([`pdfcraft_platform::dpapi`]).
//! It is not written into `app.ron`. A file that cannot be decrypted is left byte-for-byte as it
//! was and is not replaced with an empty library. The same Windows user on this PC can decrypt
//! it. Another user, or another PC, cannot. An administrator who can act as this user often can.
//! The original image the user picked, and any operating-system backup of this file, are outside
//! this encryption.

use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

pub const MAX_SEALS: usize = 8;
pub const MAX_SEAL_BYTES: usize = 2 << 20;
const MAX_HISTORY: usize = 400;
const MAX_NAME_CHARS: usize = 40;
const MAX_PLAINTEXT: usize = 16 << 20;
const MAGIC: &[u8] = b"SWSEAL1\n";

/// When set, the next save fails before it touches the file. Tests only.
#[cfg(test)]
pub static FAIL_SEAL_WRITE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealImage {
    pub name: String,
    pub file: String,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// One line of the chairman-seal log. `event` is `confirmed`, `undone`, `redone`, `removed` or
/// `removal-undone`. It records what happened to the working document. It does not mean the PDF
/// was saved, printed or officially issued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealEvent {
    pub at: String,
    pub file: String,
    pub page: u32,
    pub seal: String,
    pub event: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SealLibrary {
    pub seals: Vec<SealImage>,
    pub history: Vec<SealEvent>,
}

/// A session's view of the encrypted file.
#[derive(Clone, Debug)]
pub struct SealStore {
    pub path: Option<PathBuf>,
    pub library: SealLibrary,
    /// An existing file could not be decrypted or parsed. Nothing may be saved over it.
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct FileBody {
    seals: Vec<StoredSeal>,
    history: Vec<StoredEvent>,
}

#[derive(Serialize, Deserialize)]
struct StoredSeal {
    name: String,
    file: String,
    width: u32,
    height: u32,
    data: String,
}

#[derive(Serialize, Deserialize)]
struct StoredEvent {
    at: String,
    file: String,
    page: u32,
    seal: String,
    event: String,
}

impl SealStore {
    pub fn empty() -> Self {
        Self { path: None, library: SealLibrary::default(), error: None }
    }

    /// Read `path`. A missing file is an empty library. A file that cannot be decrypted or
    /// parsed sets [`Self::error`] and does not rewrite the file.
    pub fn load(path: PathBuf) -> Self {
        let mut store = Self::empty();
        store.path = Some(path);
        store.reload();
        store
    }

    pub fn locked(&self) -> bool {
        self.error.is_some()
    }

    fn reload(&mut self) {
        let Some(path) = &self.path else {
            self.error = Some("印影の保管先がありません".into());
            return;
        };
        if !path.exists() {
            self.library = SealLibrary::default();
            self.error = None;
            return;
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.error = Some("印影ファイルを読めませんでした。ファイルはそのままにしてあります。".into());
                self.library = SealLibrary::default();
                return;
            }
        };
        match decode_file(&bytes) {
            Ok(library) => {
                self.library = library;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error.into());
                self.library = SealLibrary::default();
            }
        }
    }

    pub fn add_seal(&mut self, seal: SealImage) -> Result<(), String> {
        self.ensure_open()?;
        if self.library.seals.len() >= MAX_SEALS {
            return Err(format!("理事長印は{MAX_SEALS}件までです"));
        }
        validate_seal(&seal)?;
        if self.library.seals.iter().any(|kept| kept.name == seal.name) {
            return Err("同じ名前の理事長印が既にあります".into());
        }
        let previous = self.library.clone();
        self.library.seals.push(seal);
        self.persist(previous)
    }

    pub fn delete_seal(&mut self, index: usize) -> Result<(), String> {
        self.ensure_open()?;
        if index >= self.library.seals.len() {
            return Err("削除する印影が見つかりません".into());
        }
        let previous = self.library.clone();
        self.library.seals.remove(index);
        self.persist(previous)
    }

    pub fn append_event(&mut self, event: SealEvent) -> Result<(), String> {
        self.ensure_open()?;
        if event.seal.chars().count() > MAX_NAME_CHARS || event.file.chars().count() > 240 || event.at.chars().count() > 40 {
            return Err("使用履歴に記録できない内容です".into());
        }
        if !matches!(event.event.as_str(), "confirmed" | "undone" | "redone" | "removed" | "removal-undone") {
            return Err("使用履歴に記録できない内容です".into());
        }
        let previous = self.library.clone();
        self.library.history.push(event);
        if self.library.history.len() > MAX_HISTORY {
            let extra = self.library.history.len() - MAX_HISTORY;
            self.library.history.drain(0..extra);
        }
        self.persist(previous)
    }

    /// Drop the last history line and write the file again. Used when the stamp itself did not
    /// stay applied.
    pub fn pop_event(&mut self) -> Result<(), String> {
        self.ensure_open()?;
        if self.library.history.is_empty() {
            return Ok(());
        }
        let previous = self.library.clone();
        self.library.history.pop();
        self.persist(previous)
    }

    fn ensure_open(&self) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.path.is_none() {
            return Err("印影の保管先がありません".into());
        }
        Ok(())
    }

    fn persist(&mut self, previous: SealLibrary) -> Result<(), String> {
        if let Err(error) = self.write() {
            self.library = previous;
            return Err(error);
        }
        Ok(())
    }

    fn write(&self) -> Result<(), String> {
        #[cfg(test)]
        if FAIL_SEAL_WRITE.load(Ordering::SeqCst) {
            return Err("印影ファイルを書き出せませんでした".into());
        }
        let Some(path) = &self.path else { return Err("印影の保管先がありません".into()) };
        if self.error.is_some() {
            return Err("印影ファイルを復号できないため、上書きしません".into());
        }
        let plain = encode_library(&self.library)?;
        let protected = pdfcraft_platform::dpapi::protect_user(&plain).map_err(str::to_owned)?;
        let mut bytes = Vec::with_capacity(MAGIC.len() + protected.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&protected);
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|_| "印影ファイルを書き出せませんでした".to_string())?;
        }
        pdfcraft_platform::staging::write_atomically(path, &bytes, pdfcraft_platform::staging::StagingName::TagThenSuffix)
            .map_err(|_| "印影ファイルを書き出せませんでした".to_string())
    }
}

fn validate_seal(seal: &SealImage) -> Result<(), String> {
    let name = seal.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS || name.chars().any(|c| c.is_control()) {
        return Err("印影の名前を確認してください".into());
    }
    if seal.data.len() > MAX_SEAL_BYTES || seal.data.is_empty() {
        return Err("印影が大きすぎます".into());
    }
    if !seal.data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("理事長印は透過PNGにしてください".into());
    }
    if seal.width == 0 || seal.height == 0 || seal.width > 8000 || seal.height > 8000 {
        return Err("印影の画像サイズを確認してください".into());
    }
    Ok(())
}

fn encode_library(library: &SealLibrary) -> Result<Vec<u8>, String> {
    let body = FileBody {
        seals: library
            .seals
            .iter()
            .map(|seal| StoredSeal {
                name: seal.name.clone(),
                file: seal.file.clone(),
                width: seal.width,
                height: seal.height,
                data: base64::engine::general_purpose::STANDARD.encode(&seal.data),
            })
            .collect(),
        history: library
            .history
            .iter()
            .map(|event| StoredEvent {
                at: event.at.clone(),
                file: event.file.clone(),
                page: event.page,
                seal: event.seal.clone(),
                event: event.event.clone(),
            })
            .collect(),
    };
    let plain = serde_json::to_vec(&body).map_err(|_| "印影を保存できませんでした".to_string())?;
    if plain.len() > MAX_PLAINTEXT {
        return Err("印影を保存できませんでした".into());
    }
    Ok(plain)
}

fn decode_file(bytes: &[u8]) -> Result<SealLibrary, &'static str> {
    let Some(blob) = bytes.strip_prefix(MAGIC) else {
        return Err("印影ファイルを復号できませんでした。ファイルはそのままにしてあります。");
    };
    if blob.is_empty() {
        return Err("印影ファイルを復号できませんでした。ファイルはそのままにしてあります。");
    }
    let plain =
        pdfcraft_platform::dpapi::unprotect_user(blob).map_err(|_| "印影ファイルを復号できませんでした。ファイルはそのままにしてあります。")?;
    if plain.len() > MAX_PLAINTEXT {
        return Err("印影ファイルを復号できませんでした。ファイルはそのままにしてあります。");
    }
    let body: FileBody = serde_json::from_slice(&plain).map_err(|_| "印影ファイルを復号できませんでした。ファイルはそのままにしてあります。")?;
    if body.seals.len() > MAX_SEALS || body.history.len() > MAX_HISTORY {
        return Err("印影ファイルを復号できませんでした。ファイルはそのままにしてあります。");
    }
    let mut seals = Vec::new();
    for stored in body.seals {
        let data = base64::engine::general_purpose::STANDARD
            .decode(stored.data.as_bytes())
            .map_err(|_| "印影ファイルを復号できませんでした。ファイルはそのままにしてあります。")?;
        let seal = SealImage { name: stored.name, file: stored.file, width: stored.width, height: stored.height, data };
        validate_seal(&seal).map_err(|_| "印影ファイルを復号できませんでした。ファイルはそのままにしてあります。")?;
        seals.push(seal);
    }
    let history = body
        .history
        .into_iter()
        .map(|event| SealEvent { at: event.at, file: event.file, page: event.page, seal: event.seal, event: event.event })
        .collect();
    Ok(SealLibrary { seals, history })
}

/// `secs` is seconds since the Unix epoch. The text is UTC and does not say the document was issued.
pub fn format_unix_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = tod / 3_600;
    let minute = (tod % 3_600) / 60;
    let second = tod % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

pub fn utc_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|dur| dur.as_secs()).unwrap_or(0);
    format_unix_utc(secs)
}

/// Howard Hinnant's civil-from-days, for a UTC date. `z` is days since 1970-01-01.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

/// A placement of a chairman seal in this session, so undo can name the same seal.
#[derive(Clone, Debug)]
pub struct SealUse {
    pub num: u32,
    pub generation: u16,
    pub name: String,
    pub page: u32,
    pub file_name: String,
    pub active: bool,
}

impl crate::PdfCraftApp {
    /// Use `path` for chairman seals and load it. A decrypt failure is kept on the store.
    pub fn set_seal_store(&mut self, path: PathBuf) {
        self.seal_store = SealStore::load(path);
    }
}

/// Read PNG or JPEG dimensions and straight RGBA for a preview. `None` when the bytes are not
/// one of those, or when the decoded image would be unreasonably large.
pub fn preview_rgba(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let image = image::load_from_memory(bytes).ok()?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 || width > 8000 || height > 8000 {
        return None;
    }
    if width.saturating_mul(height) > 4_000_000 {
        return None;
    }
    Some((width, height, rgba.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        pdfcraft_xfa::fixtures::tiny_png()
    }

    fn sample() -> SealImage {
        SealImage { name: "試験印".into(), file: "seal.png".into(), width: 2, height: 2, data: png() }
    }

    #[test]
    fn unix_epoch_formats_as_utc() {
        assert_eq!(format_unix_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_unix_utc(86_400), "1970-01-02 00:00:00 UTC");
    }

    #[test]
    #[cfg(windows)]
    fn ciphertext_round_trip_hides_the_png_and_a_bad_file_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("swiftpdf-seals-{}-round", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("seals.bin");
        let mut store = SealStore::load(path.clone());
        store.add_seal(sample()).unwrap();
        let on_disk = std::fs::read(&path).unwrap();
        assert!(on_disk.starts_with(MAGIC));
        assert!(!on_disk.windows(8).any(|window| window == &png()[..8]));
        assert!(!on_disk.windows(4).any(|window| window == b"\x89PNG"));
        let again = SealStore::load(path.clone());
        assert!(again.error.is_none());
        assert_eq!(again.library.seals.len(), 1);
        assert_eq!(again.library.seals[0].data, png());
        let mut again = again;
        again
            .append_event(SealEvent {
                at: "1970-01-01 00:00:00 UTC".into(),
                file: "sample.pdf".into(),
                page: 1,
                seal: "試験印".into(),
                event: "confirmed".into(),
            })
            .unwrap();
        let logged = SealStore::load(path.clone());
        assert_eq!(logged.library.history.len(), 1);
        assert_eq!(logged.library.history[0].event, "confirmed");
        again
            .append_event(SealEvent {
                at: "1970-01-01 00:00:01 UTC".into(),
                file: "sample.pdf".into(),
                page: 1,
                seal: "試験印".into(),
                event: "removal-undone".into(),
            })
            .unwrap();
        assert!(
            again
                .append_event(SealEvent {
                    at: "1970-01-01 00:00:02 UTC".into(),
                    file: "sample.pdf".into(),
                    page: 1,
                    seal: "試験印".into(),
                    event: "issued".into(),
                })
                .is_err()
        );
        let logged = SealStore::load(path.clone());
        assert_eq!(logged.library.history.len(), 2);
        assert_eq!(logged.library.history[1].event, "removal-undone");
        assert!(!std::fs::read(&path).unwrap().windows(8).any(|window| window == &png()[..8]));

        let mut damaged = on_disk.clone();
        let last = damaged.len() - 1;
        damaged[last] ^= 0xff;
        std::fs::write(&path, &damaged).unwrap();
        let mut broken = SealStore::load(path.clone());
        assert!(broken.error.is_some());
        assert!(broken.library.seals.is_empty());
        assert!(broken.add_seal(sample()).is_err());
        assert!(broken.delete_seal(0).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), damaged);

        FAIL_SEAL_WRITE.store(true, Ordering::SeqCst);
        let mut fresh = SealStore::load(dir.join("other.bin"));
        let err = fresh.add_seal(sample()).unwrap_err();
        assert!(!err.contains("PNG"));
        assert!(fresh.library.seals.is_empty());
        assert!(!dir.join("other.bin").exists());
        FAIL_SEAL_WRITE.store(false, Ordering::SeqCst);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(windows)]
    fn settings_do_not_store_chairman_seals() {
        let dir = std::env::temp_dir().join(format!("swiftpdf-seals-{}-settings", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = crate::PdfCraftApp::new();
        app.set_view_only(true);
        app.grant_stamp();
        app.set_seal_store(dir.join("seals.bin"));
        app.start_chairman_seal("seal.png".into(), png());
        app.save_custom_stamp();
        assert!(app.custom_stamps.is_empty());
        assert_eq!(app.seal_store.library.seals.len(), 1);
        assert!(!app.persist().contains("iVBORw0KGgo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn one_page() -> Vec<u8> {
        b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R >> endobj
4 0 obj << /Length 0 >> stream
endstream endobj
xref
0 5
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000135 00000 n 
0000000224 00000 n 
trailer << /Size 5 /Root 1 0 R >>
startxref
273
%%EOF
"
        .to_vec()
    }

    fn stamp_count(app: &crate::PdfCraftApp) -> usize {
        let id = app.views[0].id;
        app.session.get(id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count()
    }

    fn page_has_colour(bytes: &[u8]) -> bool {
        let mut renderer = pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes.to_vec()), Default::default());
        let shot = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 2.0, ..Default::default() });
        assert!(shot.error.is_none(), "{:?}", shot.error);
        shot.rgba.chunks_exact(4).any(|px| {
            let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
            a > 20 && (r != g || g != b)
        })
    }

    struct PolicyRestore((bool, bool, bool));

    impl Drop for PolicyRestore {
        fn drop(&mut self) {
            crate::output_policy::set_blocked(self.0.0);
            crate::output_policy::set_save_as_allowed(self.0.1);
            crate::output_policy::set_print_allowed(self.0.2);
        }
    }

    fn policy() -> PolicyRestore {
        PolicyRestore((crate::output_policy::blocked(), crate::output_policy::save_as_allowed(), crate::output_policy::print_allowed()))
    }

    struct ResetSealWrite;

    impl Drop for ResetSealWrite {
        fn drop(&mut self) {
            FAIL_SEAL_WRITE.store(false, Ordering::SeqCst);
        }
    }

    fn place_chairman(app: &mut crate::PdfCraftApp) {
        app.set_view_only(true);
        app.grant_stamp();
        app.grant_edit_text();
        app.start_chairman_seal("test-seal.png".into(), png());
        app.save_custom_stamp();
        assert_eq!(app.seal_store.library.seals.len(), 1, "{:?}", app.toast);
        let seal = app.seal_store.library.seals[0].clone();
        app.quick_tool = crate::QuickTool::ChairmanSeal(0);
        app.views[0].stamp_place = Some(crate::stamps_ui::StampPlace {
            page: 0,
            rect: [20.0, 200.0, 80.0, 260.0],
            name: seal.name.clone(),
            file: seal.file.clone(),
            data: std::sync::Arc::new(seal.data.clone()),
            source_page: 0,
            opacity: 1.0,
            chairman: true,
            pixels: preview_rgba(&seal.data),
            drag: None,
        });
        app.seal_ack = false;
        app.ask_stamp_confirm();
        assert!(matches!(app.dialog, Some(crate::Dialog::ConfirmSeal)));
        assert!(!app.seal_ack, "opening the approval dialog clears the checkbox");
        app.seal_ack = true;
        app.commit_stamp_place();
        app.dialog = None;
    }

    /// The approval checkbox is not part of redo: the stamp comes back from the history step,
    /// and the log says confirmed, then undone, then redone.
    #[test]
    #[cfg(windows)]
    fn chairman_seal_comes_back_on_redo_without_another_approval() {
        let _policy = policy();
        let dir = std::env::temp_dir().join(format!("swiftpdf-seals-{}-redo", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original_path = dir.join("opened.pdf");
        let original = one_page();
        std::fs::write(&original_path, &original).unwrap();
        use sha2::Digest;
        let original_sha = sha2::Sha256::digest(&original);
        let mut app = crate::PdfCraftApp::new();
        app.set_seal_store(dir.join("seals.bin"));
        app.open_bytes("opened.pdf", Some(original_path.to_string_lossy().into_owned()), original.clone()).unwrap();
        place_chairman(&mut app);
        assert_eq!(stamp_count(&app), 1, "confirm places the seal: {:?}", app.toast);
        assert_eq!(app.seal_store.library.history.last().map(|event| event.event.as_str()), Some("confirmed"));
        assert!(app.views[0].stamp_place.is_none());
        let undo = pdfcraft_engine::commands::command("edit.undo").unwrap();
        let redo = pdfcraft_engine::commands::command("edit.redo").unwrap();
        assert!(app.command_enabled(undo));
        assert!(app.execute("edit.undo"), "{:?}", app.toast);
        assert_eq!(stamp_count(&app), 0, "undo removes the seal: {:?}", app.toast);
        assert_eq!(app.seal_store.library.history.last().map(|event| event.event.as_str()), Some("undone"), "{:?}", app.seal_store.library.history);
        app.process_pending_edits();
        assert!(app.command_enabled(redo), "a later frame must not drop redo: {:?}", app.toast);
        assert!(app.execute("edit.redo"), "{:?}", app.toast);
        assert_eq!(stamp_count(&app), 1, "redo restores the seal without a new approval: {:?}", app.toast);
        assert_eq!(app.seal_store.library.history.iter().map(|event| event.event.as_str()).collect::<Vec<_>>(), ["confirmed", "undone", "redone"]);
        let id = app.views[0].id;
        let doc = app.session.get(id).unwrap();
        assert!(page_has_colour(&doc.bytes), "redo draws the seal image, not an empty stamp");
        assert!(page_has_colour(&doc.display), "the bytes on screen keep the seal image");
        assert!(app.dialog.is_none(), "redo does not open the approval dialog");
        assert!(app.views[0].stamp_place.is_none(), "redo does not arm a new placement");
        let seal = app.seal_store.library.seals[0].clone();
        app.views[0].stamp_place = Some(crate::stamps_ui::StampPlace {
            page: 0,
            rect: [20.0, 40.0, 80.0, 100.0],
            name: seal.name.clone(),
            file: seal.file,
            data: std::sync::Arc::new(seal.data.clone()),
            source_page: 0,
            opacity: 1.0,
            chairman: true,
            pixels: preview_rgba(&seal.data),
            drag: None,
        });
        app.seal_ack = true;
        app.ask_stamp_confirm();
        assert!(!app.seal_ack, "redo must not leave the checkbox able to stamp again");
        assert!(matches!(app.dialog, Some(crate::Dialog::ConfirmSeal)));
        app.dialog = None;
        app.views[0].stamp_place = None;
        assert_eq!(stamp_count(&app), 1, "asking again does not place another seal");
        assert_eq!(sha2::Sha256::digest(std::fs::read(&original_path).unwrap()), original_sha);
        assert!(app.has_unsaved_work(0), "the redone seal is unsaved");
        app.request_close_tab(0);
        assert!(app.close_request.is_some(), "closing before save asks");
        app.close_request = None;
        app.grant_save_as();
        let saved = app.session.save_edited_copy_bytes(id).expect("save as");
        assert!(page_has_colour(&saved), "the copy keeps the redone seal");
        app.session.note_saved_copy(id).unwrap();
        assert!(!app.has_unsaved_work(0), "the saved snapshot is the redone document");
        app.request_close_tab(0);
        assert!(app.close_request.is_none() && app.views.is_empty(), "closing after save does not ask again");
        assert_eq!(sha2::Sha256::digest(std::fs::read(&original_path).unwrap()), original_sha);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A history write that fails puts the document back. The notice says so, and redo does not
    /// become a way to stamp without the approval checkbox.
    #[test]
    #[cfg(windows)]
    fn chairman_seal_redo_stops_safely_when_history_cannot_be_written() {
        let _policy = policy();
        let _reset = ResetSealWrite;
        let dir = std::env::temp_dir().join(format!("swiftpdf-seals-{}-fail", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = crate::PdfCraftApp::new();
        app.set_seal_store(dir.join("seals.bin"));
        app.open_bytes("sample.pdf", None, one_page()).unwrap();
        place_chairman(&mut app);
        assert!(app.execute("edit.undo"), "{:?}", app.toast);
        assert_eq!(stamp_count(&app), 0);
        FAIL_SEAL_WRITE.store(true, Ordering::SeqCst);
        assert!(app.execute("edit.redo"));
        assert_eq!(stamp_count(&app), 0, "a failed history write puts the seal back to undone: {:?}", app.toast);
        let toast = app.toast.as_ref().map(|toast| toast.0.as_str()).unwrap_or("");
        assert!(toast.contains("使用履歴を記録できない"), "{toast}");
        assert!(!toast.contains("やり直しました"), "a rolled-back redo must not be announced as done: {toast}");
        assert!(app.views[0].stamp_place.is_none());
        assert!(app.dialog.is_none());
        assert_eq!(app.seal_store.library.history.last().map(|event| event.event.as_str()), Some("undone"));
        FAIL_SEAL_WRITE.store(false, Ordering::SeqCst);
        assert!(app.execute("edit.redo"), "{:?}", app.toast);
        assert_eq!(stamp_count(&app), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(windows)]
    fn chairman_seal_does_not_block_text_or_general_stamps() {
        let _policy = policy();
        let dir = std::env::temp_dir().join(format!("swiftpdf-seals-{}-other", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = crate::PdfCraftApp::new();
        app.set_view_only(true);
        app.grant_stamp();
        app.grant_edit_text();
        app.set_seal_store(dir.join("seals.bin"));
        app.open_bytes("sample.pdf", None, one_page()).unwrap();
        let id = app.views[0].id;
        let file = pdfcraft_engine::MarkFile { name: "mark.png".into(), bytes: std::sync::Arc::new(png()), page: 0 };
        assert!(app.apply_edit(pdfcraft_engine::Edit::AddCustomStamp {
            page: 0,
            rect: [30.0, 30.0, 90.0, 70.0],
            name: "Mark".into(),
            file,
            author: "Ada".into(),
            opacity: 1.0,
        }));
        assert_eq!(stamp_count(&app), 1);
        assert!(app.execute("edit.undo"));
        assert_eq!(stamp_count(&app), 0);
        assert!(app.execute("edit.redo"), "{:?}", app.toast);
        assert_eq!(stamp_count(&app), 1, "a general stamp still redoes");
        assert!(app.seal_store.library.history.is_empty(), "a general stamp is not a chairman-seal event");
        let text = if pdfcraft_fonts::document_japanese_font().is_some() { "日本語" } else { "Note" };
        assert!(
            app.apply_edit(pdfcraft_engine::Edit::AddText {
                page: 0,
                text: pdfcraft_engine::AddedText {
                    rect: [10.0, 40.0, 160.0, 80.0],
                    text: text.into(),
                    size: 16.0,
                    ..pdfcraft_engine::AddedText::default()
                },
            }),
            "{:?}",
            app.toast
        );
        assert!(app.execute("edit.undo"));
        assert!(app.session.get(id).unwrap().added().is_empty());
        assert!(app.execute("edit.redo"), "{:?}", app.toast);
        let shown = app.session.get(id).unwrap().added().iter().map(|item| item.content.clone()).collect::<Vec<_>>();
        assert!(format!("{shown:?}").contains(text), "{shown:?}");
        assert_eq!(stamp_count(&app), 1, "redoing the text leaves the stamp");

        let mut author = pdfcraft_engine::Session::new();
        let made = author.open("sample.pdf", None, std::sync::Arc::new(one_page()), None).unwrap();
        author
            .apply(
                made,
                pdfcraft_engine::Edit::Protect(pdfcraft_engine::Protection {
                    permissions_password: Some("owner".into()),
                    changes: pdfcraft_engine::Changes::None,
                    ..pdfcraft_engine::Protection::default()
                }),
            )
            .unwrap();
        let locked = author.save_bytes(made).unwrap();
        app.open_bytes("banned.pdf", None, locked.as_ref().clone()).unwrap();
        let banned = app.views.len() - 1;
        assert_eq!(app.session.get(app.views[banned].id).unwrap().name, "banned.pdf");
        assert!(app.stamp_refusal(banned).is_some());
        assert!(!app.session.allows_stamp(app.views[banned].id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn click_at(h: &mut egui_kittest::Harness<'static, crate::PdfCraftApp>, at: egui::Pos2) {
        h.hover_at(at);
        h.run_steps(1);
        h.drag_at(at);
        h.run_steps(1);
        h.drop_at(at);
        h.run_steps(3);
    }

    fn ui_stamp_count(h: &egui_kittest::Harness<'static, crate::PdfCraftApp>) -> usize {
        let app = h.state();
        app.session.get(app.views[0].id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count()
    }

    /// The same clicks as the window: register, approve, confirm, then Undo and Redo from the
    /// menu in the detailed interface and from the keyboard in the simple one.
    #[test]
    #[cfg(windows)]
    fn chairman_seal_ui_undo_and_redo_in_both_modes() {
        use egui_kittest::kittest::Queryable;
        let _policy = policy();
        for mode in [crate::UiMode::Simple, crate::UiMode::Detailed] {
            let dir = std::env::temp_dir().join(format!("swiftpdf-seal-ui-{}-{}", std::process::id(), mode.as_str()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let original_path = dir.join("opened.pdf");
            let original = one_page();
            std::fs::write(&original_path, &original).unwrap();
            use sha2::Digest;
            let original_sha = sha2::Sha256::digest(&original);
            let seal_path = dir.join("seals.bin");
            let path_s = original_path.to_string_lossy().into_owned();
            let bytes = original.clone();
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
                let mut app = crate::PdfCraftApp::new();
                app.set_option("language", "ja").unwrap();
                app.set_view_only(true);
                app.grant_stamp();
                app.grant_edit_text();
                app.set_ui_mode(mode);
                app.set_seal_store(seal_path);
                app.open_bytes("opened.pdf", Some(path_s), bytes).unwrap();
                app.set_option("zoom", "100").unwrap();
                app
            });
            h.run_steps(8);
            assert!(h.state_mut().execute("comment.stamp"), "{mode:?}");
            h.run_steps(3);
            h.state_mut().start_chairman_seal("test-seal.png".into(), png());
            h.run_steps(2);
            h.get_by_label("OK").click();
            h.run_steps(3);
            assert!(matches!(h.state().quick_tool, crate::QuickTool::ChairmanSeal(0)), "{mode:?} {:?}", h.state().toast);
            let at = {
                let r = h.state().views[0].page_screen_rect(0).expect("page");
                r.min + egui::vec2(r.width() * 0.4, r.height() * 0.25)
            };
            click_at(&mut h, at);
            h.get_by_label("確定").click();
            h.run_steps(2);
            assert!(matches!(h.state().dialog, Some(crate::Dialog::ConfirmSeal)), "{mode:?} {:?}", h.state().toast);
            h.get_by_label("この文書への押印が承認されていることを確認しました").click();
            h.run_steps(1);
            assert!(h.state().seal_ack, "{mode:?}");
            h.get_all_by_label("確定").nth(1).expect("dialog confirm").click();
            h.run_steps(4);
            assert_eq!(ui_stamp_count(&h), 1, "{mode:?} place {:?}", h.state().toast);
            assert!(h.state().dialog.is_none(), "{mode:?}");
            assert_eq!(h.state().seal_store.library.history.last().map(|event| event.event.as_str()), Some("confirmed"));
            if mode == crate::UiMode::Detailed {
                h.get_by_label("メニュー").click();
                h.run_steps(2);
                h.get_by_label("編集 ⏵").hover();
                h.run_steps(3);
                h.get_by_label_contains("取り消し スタンプを追加").click();
            } else {
                h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
            }
            h.run_steps(6);
            assert_eq!(ui_stamp_count(&h), 0, "{mode:?} undo {:?}", h.state().toast);
            assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_redo(), Some("Add stamp"), "{mode:?}");
            if mode == crate::UiMode::Detailed {
                h.get_by_label("メニュー").click();
                h.run_steps(2);
                h.get_by_label("編集 ⏵").hover();
                h.run_steps(3);
                h.get_by_label_contains("やり直し スタンプを追加").click();
            } else {
                h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
            }
            h.run_steps(6);
            assert_eq!(ui_stamp_count(&h), 1, "{mode:?} redo {:?}", h.state().toast);
            assert_eq!(
                h.state().seal_store.library.history.iter().map(|event| event.event.as_str()).collect::<Vec<_>>(),
                ["confirmed", "undone", "redone"]
            );
            h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
            h.run_steps(4);
            assert_eq!(ui_stamp_count(&h), 0, "{mode:?} second undo {:?}", h.state().toast);
            h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Y);
            h.run_steps(4);
            assert_eq!(ui_stamp_count(&h), 1, "{mode:?}: Ctrl+Y redoes the seal {:?}", h.state().toast);
            assert_eq!(sha2::Sha256::digest(std::fs::read(&original_path).unwrap()), original_sha);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_missing_file_is_empty_and_oversize_png_is_refused() {
        let dir = std::env::temp_dir().join(format!("swiftpdf-seals-{}-empty", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = SealStore::load(dir.join("seals.bin"));
        assert!(store.error.is_none());
        assert!(store.library.seals.is_empty());
        let mut huge = sample();
        huge.data = vec![0; MAX_SEAL_BYTES + 1];
        let mut store = store;
        assert!(store.add_seal(huge).is_err());
        assert!(!dir.join("seals.bin").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
