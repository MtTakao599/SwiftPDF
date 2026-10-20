//! Whether the app may write the files a user asks for: saved and new PDFs, printing, exports
//! (images, Office, CSV, XFDF…), attachments, copied images and new digital IDs.
//!
//! It is process-wide because the writers ([`crate::editing::write_atomically`], the browser
//! download, the copied-image and digital-ID files) run without the app, some on worker threads
//! or after a file dialog answers. The app's own data (settings, logs, the last session, the
//! single-instance socket) doesn't go through those writers and is never refused.
//! Unit tests compile a per-thread copy instead, so one test cannot refuse another test's writer.
//! That copy is not in the desktop binary.
//!
//! [`PdfCraftApp::set_view_only`] turns it on together with the engine's view-only session, so
//! the two never disagree; the desktop app does that at start.

use std::sync::atomic::AtomicBool;
#[cfg(not(test))]
use std::sync::atomic::Ordering;

use crate::PdfCraftApp;

// The desktop binary reads these. Unit tests do not: they share one process, so each test thread
// keeps its own copy and a view-only test cannot refuse another test's save.
#[cfg_attr(test, allow(dead_code))]
static BLOCKED: AtomicBool = AtomicBool::new(false);
/// A copy of an opened PDF to a new path. This does not turn [`BLOCKED`] off, so export and
/// in-place save stay refused.
#[cfg_attr(test, allow(dead_code))]
static SAVE_AS: AtomicBool = AtomicBool::new(false);
/// Printing and print preview. This does not turn [`BLOCKED`] off, so in-place save and export
/// stay refused. Saving the print-ready PDF still goes through the new-file writer.
#[cfg_attr(test, allow(dead_code))]
static PRINT: AtomicBool = AtomicBool::new(false);

/// Unit tests share one process, and the test harness reuses threads. A flag set here is visible
/// only on the thread that set it, including the next test that thread runs, so each test restores
/// it before returning. The desktop binary does not compile this.
#[cfg(test)]
mod local {
    use std::cell::Cell;

    thread_local! {
        static BLOCKED: Cell<bool> = const { Cell::new(false) };
        static SAVE_AS: Cell<bool> = const { Cell::new(false) };
        static PRINT: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn set_blocked(on: bool) {
        BLOCKED.with(|v| v.set(on));
    }

    pub(super) fn blocked() -> bool {
        BLOCKED.with(|v| v.get())
    }

    pub(super) fn set_save_as(on: bool) {
        SAVE_AS.with(|v| v.set(on));
    }

    pub(super) fn save_as() -> bool {
        SAVE_AS.with(|v| v.get())
    }

    pub(super) fn set_print(on: bool) {
        PRINT.with(|v| v.set(on));
    }

    pub(super) fn print() -> bool {
        PRINT.with(|v| v.get())
    }
}

/// What the user is told when an output is refused.
pub const REFUSED: &str = "This app is view-only: it doesn't save, print or export files.";

/// Refuse (or allow again) every file the user asks for.
pub fn set_blocked(on: bool) {
    #[cfg(test)]
    local::set_blocked(on);
    #[cfg(not(test))]
    BLOCKED.store(on, Ordering::SeqCst);
}

/// Whether files the user asks for are refused.
pub fn blocked() -> bool {
    #[cfg(test)]
    return local::blocked();
    #[cfg(not(test))]
    BLOCKED.load(Ordering::SeqCst)
}

/// The error a writer returns, before touching the disk, while outputs are refused.
pub(crate) fn check() -> std::io::Result<()> {
    if blocked() { Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, REFUSED)) } else { Ok(()) }
}

/// Allow the view-only copy writer. It does not clear [`set_blocked`].
pub fn set_save_as_allowed(on: bool) {
    #[cfg(test)]
    local::set_save_as(on);
    #[cfg(not(test))]
    SAVE_AS.store(on, Ordering::SeqCst);
}

/// Whether the view-only copy writer may run. Export and in-place save do not read this.
pub fn save_as_allowed() -> bool {
    #[cfg(test)]
    return local::save_as();
    #[cfg(not(test))]
    SAVE_AS.load(Ordering::SeqCst)
}

/// Allow printing. It does not clear [`set_blocked`].
pub fn set_print_allowed(on: bool) {
    #[cfg(test)]
    local::set_print(on);
    #[cfg(not(test))]
    PRINT.store(on, Ordering::SeqCst);
}

/// Whether printing may run while file output stays blocked.
pub fn print_allowed() -> bool {
    #[cfg(test)]
    return local::print();
    #[cfg(not(test))]
    PRINT.load(Ordering::SeqCst)
}

impl PdfCraftApp {
    /// View-only in every mode: the session refuses edits and the files it would produce, and
    /// the app refuses every file the user asks for. `false` restores both.
    pub fn set_view_only(&mut self, on: bool) {
        self.session.set_view_only(on);
        set_blocked(on);
        if on {
            self.default_mode = self.default_mode.in_view_only(true);
            self.mode = self.mode.in_view_only(true);
            if let Some(mode) = self.mode_override {
                self.mode_override = Some(mode.in_view_only(true));
            }
        }
    }

    /// Grant saving the opened bytes to a new file. View-only stays on, and [`set_blocked`] stays
    /// on, so this does not allow in-place save or export.
    pub fn grant_save_as(&mut self) {
        self.session.grant(pdfcraft_engine::Capability::SaveAs);
        set_save_as_allowed(true);
    }

    /// Grant adding text onto the open PDF. View-only stays on, and no other edit is granted.
    pub fn grant_edit_text(&mut self) {
        self.session.grant(pdfcraft_engine::Capability::EditText);
    }

    /// Grant printing the open PDF. View-only stays on, and [`set_blocked`] stays on, so this
    /// does not allow in-place save, export, or replacing an existing file.
    pub fn grant_print(&mut self) {
        self.session.grant(pdfcraft_engine::Capability::Print);
        set_print_allowed(true);
    }

    /// Grant image stamps placed in this session. View-only stays on. Other comments, forms,
    /// pages, in-place save and export stay refused.
    pub fn grant_stamp(&mut self) {
        self.session.grant(pdfcraft_engine::Capability::Stamp);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thread_that_blocks_output_is_refused_and_another_thread_is_not() {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                set_blocked(false);
            }
        }
        let _restore = Restore;
        set_blocked(true);
        let worker = std::thread::spawn(check);
        assert!(check().is_err(), "the test that turned view-only on is still refused");
        let worker_result = worker.join().unwrap_or(Err(std::io::Error::other("the worker panicked")));
        assert!(worker_result.is_ok(), "another test must not inherit this thread's view-only refusal");
        set_blocked(false);
        assert!(check().is_ok());
    }
}
