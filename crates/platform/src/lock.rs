//! Exclusive locks between processes on a lock file that holds no data.
//!
//! [`lock_with_timeout`] opens (or creates) the file and takes the operating system's exclusive
//! lock on it (`LockFileEx` on Windows, `flock` elsewhere). The lock lives as long as the returned
//! [`FileLock`]: dropping it, or the process ending in any way, closes the handle and the system
//! releases it. The file is never written, truncated or removed, so every process that names the
//! same folder reaches the same file whatever spelling or junction it took to get there.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

/// A held lock. Released when dropped.
#[derive(Debug)]
pub struct FileLock {
    _file: File,
}

#[derive(Debug)]
pub enum LockError {
    /// Another process held the lock for the whole wait.
    TimedOut(Duration),
    /// The lock file could not be opened or locked.
    Io(std::io::Error),
}

/// Take the exclusive lock on `path`, retrying every `poll` (at least 1 ms) until `timeout` has
/// passed. Never waits longer than `timeout` plus one poll.
pub fn lock_with_timeout(path: &Path, timeout: Duration, poll: Duration) -> Result<FileLock, LockError> {
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(LockError::Io)?;
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(FileLock { _file: file }),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(e)) => return Err(LockError::Io(e)),
        }
        let waited = start.elapsed();
        if waited >= timeout {
            return Err(LockError::TimedOut(waited));
        }
        std::thread::sleep(poll.min(timeout.saturating_sub(waited)).max(Duration::from_millis(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfcraft-platform-lock-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_second_holder_times_out_and_gets_the_lock_once_the_first_is_dropped() {
        let dir = test_dir("hold");
        let path = dir.join("x.lock");
        let first = lock_with_timeout(&path, Duration::from_secs(1), Duration::from_millis(10)).unwrap();
        let start = Instant::now();
        match lock_with_timeout(&path, Duration::from_millis(200), Duration::from_millis(10)) {
            Err(LockError::TimedOut(waited)) => assert!(waited >= Duration::from_millis(200)),
            other => panic!("expected a timeout, got {other:?}"),
        }
        assert!(start.elapsed() < Duration::from_secs(2), "the wait is bounded");
        drop(first);
        assert!(lock_with_timeout(&path, Duration::from_millis(200), Duration::from_millis(10)).is_ok());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0, "the lock file holds no data");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_zero_poll_still_waits_between_tries() {
        let dir = test_dir("poll");
        let path = dir.join("x.lock");
        let _first = lock_with_timeout(&path, Duration::from_secs(1), Duration::ZERO).unwrap();
        assert!(matches!(lock_with_timeout(&path, Duration::from_millis(50), Duration::ZERO), Err(LockError::TimedOut(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_is_an_error_not_a_wait() {
        let dir = test_dir("missing");
        let start = Instant::now();
        assert!(matches!(
            lock_with_timeout(&dir.join("no").join("x.lock"), Duration::from_secs(3), Duration::from_millis(10)),
            Err(LockError::Io(_))
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
