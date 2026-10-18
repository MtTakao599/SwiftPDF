//! SwiftPDF's name and the names of its per-user folders, kept apart from PdfCraft's so both can
//! be installed and run side by side: preferences, logs, crash recovery, digital IDs, the
//! clipboard copy and the single-instance socket never share a folder or a socket.

/// The name shown in the window title and the About window.
pub const APP_NAME: &str = "SwiftPDF";
/// The per-user folder name on Windows (under `%LOCALAPPDATA%`) and macOS.
pub const DIR_NAME: &str = "SwiftPDF";
/// The per-user folder name on XDG desktops (Linux, the BSDs).
pub const XDG_DIR_NAME: &str = "swiftpdf";
/// The key the preferences are saved under in `app.ron` (and in browser storage on the web).
pub const STORAGE_KEY: &str = "swiftpdf";

/// Japanese when the UI language is Japanese; English otherwise. Used for SwiftPDF wording that
/// must not replace the PdfCraft catalog strings the other languages still translate.
pub fn localized(ja: &str, en: &str) -> String {
    if crate::i18n::current().code() == "ja" { ja.to_owned() } else { en.to_owned() }
}
