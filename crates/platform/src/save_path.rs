//! Deciding whether a copy may be written to a new path, and writing it without replacing
//! anything that is already there.
//!
//! [`crate::staging::write_atomically`] renames the staging file over the target. On Unix that
//! replaces an existing file. This writer uses the same staging file (new, flushed, removed on
//! failure) and then publishes it only with a hard link. A hard link fails when the name is
//! already taken, including when the name appears after the last check, so that race does not
//! replace anything. A volume that cannot make a hard link gets an error. There is no rename:
//! on Unix a rename would replace a file created in that gap. The bytes may be the opened PDF
//! or, later, a revision made while editing. This function only decides where they may be written.

use std::path::Path;

use crate::staging::{StagingName, create_staging, staging_suffixes};

/// Why a new-file copy was not written.
#[derive(Debug)]
pub enum NewFileError {
    /// `dest` is the original file, including through a different spelling.
    SameAsOriginal,
    /// Something is already at `dest` (a file, directory, or link). It was left as it was.
    DestinationExists,
    /// The paths could not be resolved, so nothing was written.
    Unresolved(String),
    /// The folder cannot add a new name by hard link, so nothing was published. A rename is not
    /// used in its place.
    LinkUnavailable(std::io::Error),
    /// The staging write failed. An existing destination is left as it was.
    Io(std::io::Error),
}

impl std::fmt::Display for NewFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SameAsOriginal => write!(f, "that is the original file, so it was not changed"),
            Self::DestinationExists => write!(f, "a file is already there, and this version does not overwrite it"),
            Self::Unresolved(message) => write!(f, "{message}"),
            Self::LinkUnavailable(error) => {
                write!(f, "this folder cannot create a new file without replacing an existing one, so nothing was written ({error})")
            }
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for NewFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::LinkUnavailable(error) => Some(error),
            _ => None,
        }
    }
}

/// Refuse when `dest` is the same file as `original`, or when `dest` already exists, or when
/// that cannot be decided. `original` of `None` only checks that the destination is absent.
pub fn classify(original: Option<&Path>, dest: &Path) -> Result<(), NewFileError> {
    if dest.file_name().is_none() {
        return Err(NewFileError::Unresolved("the destination has no file name, so nothing was written".into()));
    }
    if let Some(original) = original {
        match same_file_as(original, dest) {
            Ok(true) => return Err(NewFileError::SameAsOriginal),
            Ok(false) => {}
            Err(message) => return Err(NewFileError::Unresolved(message)),
        }
    }
    if dest.symlink_metadata().is_ok() {
        return Err(NewFileError::DestinationExists);
    }
    Ok(())
}

/// `Ok(true)` when both paths name one file. `Ok(false)` when they do not. `Err` when the
/// original cannot be resolved, or an existing destination cannot be told apart from it.
pub fn same_file_as(original: &Path, dest: &Path) -> Result<bool, String> {
    let original_canon =
        std::fs::canonicalize(original).map_err(|_| "the original file could not be resolved, so nothing was written".to_string())?;
    if let Ok(dest_canon) = std::fs::canonicalize(dest) {
        if paths_equal(&original_canon, &dest_canon)? {
            return Ok(true);
        }
        return match same_file::is_same_file(&original_canon, &dest_canon) {
            Ok(same) => Ok(same),
            // The destination exists but its identity could not be read. The caller refuses an
            // existing destination either way; failing here does not write.
            Err(error) => Err(error.to_string()),
        };
    }
    if dest.symlink_metadata().is_ok() {
        // A link or other entry that does not resolve. It is not safe to follow it.
        return Err("the destination could not be resolved, so nothing was written".to_string());
    }
    let name = dest.file_name().ok_or_else(|| "the destination has no file name, so nothing was written".to_string())?;
    let parent = dest.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|_| "the destination folder could not be resolved, so nothing was written".to_string())?;
    let joined = parent.join(name);
    paths_equal(&original_canon, &joined)
}

/// Write `bytes` to a new file at `dest`. The destination's folder must exist. Nothing already
/// at `dest` is opened or replaced. On failure the staging file is removed and `dest` is left
/// as it was, including when `dest` did not exist.
pub fn write_new_file(original: Option<&Path>, dest: &Path, bytes: &[u8]) -> Result<(), NewFileError> {
    write_new_file_linking(original, dest, bytes, |staging, dest| std::fs::hard_link(staging, dest))
}

/// [`write_new_file`] with the function that publishes the staging file. Production passes
/// [`std::fs::hard_link`]. Tests pass a function that fails, so a rename cannot sneak in.
fn write_new_file_linking(
    original: Option<&Path>,
    dest: &Path,
    bytes: &[u8],
    link: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), NewFileError> {
    classify(original, dest)?;
    let dir = dest.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| NewFileError::Unresolved("the destination has no file name, so nothing was written".into()))?;
    let (tmp, file) = create_staging(dir, &name, StagingName::TagThenSuffix, staging_suffixes()).map_err(NewFileError::Io)?;
    let result = (|| {
        {
            let mut file = file;
            std::io::Write::write_all(&mut file, bytes).map_err(NewFileError::Io)?;
            file.sync_all().map_err(NewFileError::Io)?;
        }
        // The original or the destination may have changed while the bytes were written.
        classify(original, dest)?;
        publish_linking(&tmp, dest, link)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Publish by hard link only. `Ok` means `dest` is a new name for the completed staging file.
/// Any failure leaves `dest` as it was: an error after the name appeared is "already there",
/// and any other error is reported without renaming onto `dest`.
fn publish_linking(staging: &Path, dest: &Path, link: impl FnOnce(&Path, &Path) -> std::io::Result<()>) -> Result<(), NewFileError> {
    match link(staging, dest) {
        Ok(()) => {
            let _ = std::fs::remove_file(staging);
            Ok(())
        }
        Err(error) => {
            if error.kind() == std::io::ErrorKind::AlreadyExists || dest.symlink_metadata().is_ok() {
                Err(NewFileError::DestinationExists)
            } else {
                Err(NewFileError::LinkUnavailable(error))
            }
        }
    }
}

fn paths_equal(a: &Path, b: &Path) -> Result<bool, String> {
    let a = path_key(a)?;
    let b = path_key(b)?;
    #[cfg(windows)]
    {
        Ok(a.eq_ignore_ascii_case(&b))
    }
    #[cfg(not(windows))]
    {
        Ok(a == b)
    }
}

fn path_key(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or_else(|| "the path is not text, so nothing was written".to_string())?;
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    #[cfg(windows)]
    {
        Ok(text.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        Ok(text.replace('\\', "/"))
    }
}

/// The path of a file created by [`write_new_file`], so a test can see a staging file left behind.
#[cfg(test)]
fn list(dir: &Path) -> Vec<String> {
    let mut names =
        std::fs::read_dir(dir).into_iter().flatten().flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn dir(test: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("swiftpdf-save-path-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn a_new_file_is_written_and_an_existing_one_is_not() {
        let dir = dir("new");
        let original = dir.join("opened.pdf");
        std::fs::write(&original, b"ORIGINAL").unwrap();
        let dest = dir.join("copy.pdf");
        write_new_file(Some(&original), &dest, b"%PDF-copy").unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"%PDF-copy");
        assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        std::fs::write(&dest, b"KEEP").unwrap();
        let error = write_new_file(Some(&original), &dest, b"%PDF-replaced").unwrap_err();
        assert!(matches!(error, NewFileError::DestinationExists), "{error}");
        assert_eq!(std::fs::read(&dest).unwrap(), b"KEEP");
        assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_original_spelled_another_way_is_refused() {
        let dir = dir("same");
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let original = dir.join("opened.pdf");
        std::fs::write(&original, b"ORIGINAL").unwrap();
        let via_parent = nested.join("..").join("opened.pdf");
        let error = write_new_file(Some(&original), &via_parent, b"NO").unwrap_err();
        assert!(matches!(error, NewFileError::SameAsOriginal), "{error}");
        let mixed = PathBuf::from(format!("{}/nested/../opened.pdf", dir.display()));
        let error = write_new_file(Some(&original), &mixed, b"NO").unwrap_err();
        assert!(matches!(error, NewFileError::SameAsOriginal), "{error}");
        assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        #[cfg(windows)]
        {
            let upper = dir.join("OPENED.PDF");
            let error = write_new_file(Some(&original), &upper, b"NO").unwrap_err();
            assert!(matches!(error, NewFileError::SameAsOriginal), "{error}");
            assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        }
        let link = dir.join("hard.pdf");
        std::fs::hard_link(&original, &link).unwrap();
        let error = write_new_file(Some(&original), &link, b"NO").unwrap_err();
        assert!(matches!(error, NewFileError::SameAsOriginal), "{error}");
        assert_eq!(std::fs::read(&link).unwrap(), b"ORIGINAL");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_symlink_to_the_original_is_refused_when_one_can_be_created() {
        let dir = dir("link");
        let original = dir.join("opened.pdf");
        std::fs::write(&original, b"ORIGINAL").unwrap();
        let link = dir.join("link.pdf");
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_file(&original, &link);
        #[cfg(unix)]
        let created = std::os::unix::fs::symlink(&original, &link);
        #[cfg(not(any(windows, unix)))]
        let created: std::io::Result<()> = Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "no symlink"));
        if created.is_ok() {
            let error = write_new_file(Some(&original), &link, b"NO").unwrap_err();
            assert!(matches!(error, NewFileError::SameAsOriginal), "{error}");
            assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_writes_nothing() {
        let dir = dir("missing");
        let original = dir.join("opened.pdf");
        std::fs::write(&original, b"ORIGINAL").unwrap();
        let dest = dir.join("absent").join("copy.pdf");
        let error = write_new_file(Some(&original), &dest, b"NO").unwrap_err();
        assert!(matches!(error, NewFileError::Unresolved(_)), "{error}");
        assert!(!dir.join("absent").exists());
        assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        assert_eq!(list(&dir), vec!["opened.pdf".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_japanese_name_round_trips() {
        let dir = dir("ja");
        let folder = dir.join("保存");
        std::fs::create_dir_all(&folder).unwrap();
        let original = folder.join("原本.pdf");
        std::fs::write(&original, b"ORIGINAL").unwrap();
        let dest = folder.join("副本.pdf");
        write_new_file(Some(&original), &dest, b"%PDF-ja").unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"%PDF-ja");
        assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_hard_link_does_not_publish_by_renaming() {
        let dir = dir("nolink");
        let original = dir.join("opened.pdf");
        std::fs::write(&original, b"ORIGINAL").unwrap();
        let dest = dir.join("copy.pdf");
        let error = write_new_file_linking(Some(&original), &dest, b"NEW", |_, _| {
            Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "hard links are not available on this folder"))
        })
        .unwrap_err();
        assert!(matches!(error, NewFileError::LinkUnavailable(_)), "{error}");
        assert!(!dest.exists(), "a rename fallback would have created the destination");
        assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        assert_eq!(list(&dir), vec!["opened.pdf".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_created_at_the_destination_during_save_is_left_alone() {
        let dir = dir("race");
        let original = dir.join("opened.pdf");
        std::fs::write(&original, b"ORIGINAL").unwrap();
        let dest = dir.join("copy.pdf");
        let error = write_new_file_linking(Some(&original), &dest, b"NEW", |_staging, dest| {
            std::fs::write(dest, b"RACE").unwrap();
            Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "hard link failed after the name appeared"))
        })
        .unwrap_err();
        assert!(matches!(error, NewFileError::DestinationExists), "{error}");
        assert_eq!(std::fs::read(&dest).unwrap(), b"RACE");
        assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL");
        assert_eq!(list(&dir), vec!["copy.pdf".to_string(), "opened.pdf".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
