//! Whether the app may write the files a user asks for: saved and new PDFs, printing, exports
//! (images, Office, CSV, XFDF…), attachments, copied images and new digital IDs.
//!
//! It is process-wide because the writers ([`crate::editing::write_atomically`], the browser
//! download, the copied-image and digital-ID files) run without the app, some on worker threads
//! or after a file dialog answers. The app's own data (settings, logs, the last session, the
//! single-instance socket) doesn't go through those writers and is never refused.
//!
//! [`PdfCraftApp::set_view_only`] turns it on together with the engine's view-only session, so
//! the two never disagree; the desktop app does that at start.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::PdfCraftApp;

static BLOCKED: AtomicBool = AtomicBool::new(false);

/// What the user is told when an output is refused.
pub const REFUSED: &str = "This app is view-only: it doesn't save, print or export files.";

/// Refuse (or allow again) every file the user asks for.
pub fn set_blocked(on: bool) {
    BLOCKED.store(on, Ordering::SeqCst);
}

/// Whether files the user asks for are refused.
pub fn blocked() -> bool {
    BLOCKED.load(Ordering::SeqCst)
}

/// The error a writer returns, before touching the disk, while outputs are refused.
pub(crate) fn check() -> std::io::Result<()> {
    if blocked() { Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, REFUSED)) } else { Ok(()) }
}

impl PdfCraftApp {
    /// View-only in every mode: the session refuses edits and the files it would produce, and
    /// the app refuses every file the user asks for. `false` restores both.
    pub fn set_view_only(&mut self, on: bool) {
        self.session.set_view_only(on);
        set_blocked(on);
    }

    /// Tell the user and return `true` when outputs are refused; for the start of an output, so
    /// nothing is prepared and no file dialog opens.
    pub(crate) fn refuse_output(&mut self) -> bool {
        let refused = blocked();
        if refused {
            self.notify(REFUSED);
        }
        refused
    }
}
