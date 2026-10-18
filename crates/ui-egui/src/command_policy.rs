//! The registered commands a view-only session still runs: reading, finding, copying text,
//! viewing properties and signatures, the view and the app itself.
//!
//! Every way in to a command (menus, keyboard shortcuts, the palette, toolbars, the tool panels,
//! scripts and the control channel) ends in [`PdfCraftApp::execute`] or
//! [`PdfCraftApp::run_command`], and both ask [`PdfCraftApp::command_allowed`] first; menus and
//! the palette grey out what it refuses through `command_enabled`. A command missing from the
//! list is refused, so one added upstream stays off until it is judged safe to read with.
//!
//! The engine's view-only session and the output policy still refuse edits and files on their
//! own; this list only keeps the interface from offering them.

use crate::PdfCraftApp;

/// What the user is told when a command is refused.
pub const REFUSED: &str = "This app is view-only: that isn't available.";

/// Commands a view-only session runs. None of them changes a document or writes a file.
const VIEW_ONLY: &[&str] = &[
    // Opening, closing and the recent list (app data).
    "file.open",
    "file.open_recent",
    "file.clear_recent",
    "file.pin_folder",
    "file.close",
    "file.close_all",
    // Looking at the document.
    "file.properties",
    "file.word_count",
    "protect.properties",
    "edit.find",
    "edit.advanced_search",
    // Copying text follows the document's copy permission (`Needs::Copying`).
    "edit.copy",
    "edit.column_select",
    "comment.list",
    "comment.hide_all",
    "sign.validate",
    "sign.panel",
    // The view: layout, zoom, navigation, tabs, theme and panels.
    "view.palette",
    "view.layout.single",
    "view.layout.two_up",
    "view.continuous",
    "view.layout.cover",
    "view.fit_width_scrolling",
    "view.fit_one_page",
    "view.fit_visible",
    "view.marquee_zoom",
    "view.full_screen",
    "view.read_mode",
    "view.next_tab",
    "view.previous_tab",
    "view.previous_view",
    "view.next_view",
    "view.focus_page_input",
    "view.theme",
    "view.theme.system",
    "view.theme.light",
    "view.theme.dark",
    "ui.dock.reset",
    // The app.
    "app.preferences",
    "help.shortcuts",
    "help.about",
];

/// Help commands that would open a PdfCraft or ArtCraft page, or check that project for updates.
/// A view-only SwiftPDF session neither runs them nor shows them.
const HIDDEN_IN_VIEW_ONLY: &[&str] = &["help.discord", "help.app_page", "help.github", "help.website", "help.check_updates"];

/// Whether a view-only session hides this command instead of offering it.
pub(crate) fn hidden_in_view_only(id: &str) -> bool {
    HIDDEN_IN_VIEW_ONLY.contains(&id)
}

/// Whether a view-only session runs the command `id`; unknown commands don't.
pub fn allowed_in_view_only(id: &str) -> bool {
    VIEW_ONLY.contains(&id)
}

/// Whether a view-only session applies the view option `key` = `value` (`--dialog`, `--quick`…
/// on the command line, [`PdfCraftApp::set_option`]). Options that open a dialog or pick a tool
/// directly, without a command, get the same judgement as the commands: only reading ones.
pub fn option_allowed_in_view_only(key: &str, value: &str) -> bool {
    match key {
        "dialog" => matches!(value, "properties" | "security" | "fonts" | "advanced" | "shortcuts" | "word-count" | "preferences" | "about" | "none"),
        "quick" => matches!(value, "select" | "hand" | "marquee-zoom" | "column-select"),
        "organize" => value == "off",
        _ => true,
    }
}

impl PdfCraftApp {
    /// Whether the command `id` may run in this session: any command, unless it is view-only.
    pub fn command_allowed(&self, id: &str) -> bool {
        !self.session.view_only() || allowed_in_view_only(id)
    }

    /// Tell the user and return `true` when this session is view-only, for actions that don't
    /// change the document but are refused in it (copying a picture or a snapshot).
    pub(crate) fn refuse_in_view_only(&mut self) -> bool {
        let refused = self.session.view_only();
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
    fn only_reading_dialogs_and_tools_are_options() {
        for (key, value) in [("dialog", "properties"), ("dialog", "about"), ("dialog", "none"), ("quick", "hand"), ("organize", "off"), ("page", "3")]
        {
            assert!(option_allowed_in_view_only(key, value), "--{key} {value}");
        }
        for (key, value) in [
            ("dialog", "sign"),
            ("dialog", "certify"),
            ("dialog", "export-image"),
            ("dialog", "protect"),
            ("dialog", "split"),
            ("dialog", "js-console"),
            ("dialog", "number-pages"),
            ("dialog", "unknown"),
            ("quick", "note"),
            ("quick", "snapshot"),
            ("quick", "redact"),
            ("organize", "on"),
        ] {
            assert!(!option_allowed_in_view_only(key, value), "--{key} {value}");
        }
    }

    #[test]
    fn the_list_names_only_registered_commands() {
        for id in VIEW_ONLY {
            assert!(pdfcraft_engine::commands::command(id).is_some(), "{id} is not a registered command");
        }
    }

    #[test]
    fn editing_saving_printing_and_exporting_are_not_listed() {
        for id in [
            "file.save",
            "file.save_as",
            "file.revert",
            "create.blank",
            "create.file",
            "page.combine",
            "page.rotate",
            "page.rotate_ccw",
            "page.delete",
            "page.extract",
            "page.split",
            "page.organize",
            "print.dialog",
            "export.image",
            "export.text",
            "export.docx",
            "optimize.reduce",
            "comment.note",
            "comment.highlight",
            "form.clear",
            "form.fields",
            "sign.digital",
            "sign.certify_invisible",
            "protect.unlock",
            "protect.password",
            "edit.snapshot",
            "edit.undo",
            "edit.redo",
            "tools.js_console",
            "tools.document_js",
            "help.discord",
            "help.app_page",
            "help.github",
            "help.website",
            "help.check_updates",
            "no.such.command",
            "",
        ] {
            assert!(!allowed_in_view_only(id), "{id} must not run in a view-only session");
        }
    }
}
