//! What each registered command does in this frontend (`pdfcraft_engine::commands` says
//! what it is called, where it appears, which key runs it and when it is enabled).
//!
//! Menus, keyboard shortcuts, the palette, the tool panels and automation (`set_option`, and
//! the control channel later) all call `PdfCraftApp::execute`.

use pdfcraft_engine::Edit;
use pdfcraft_engine::commands::{self, COMMANDS, CommandSpec, Shortcut};

use crate::{
    Dialog, Mode, PdfCraftApp, PropsTab, RightPanel, SaveTarget,
    theme::{ThemeKind, ThemePreference},
    widgets,
};

/// Keys the document view handles itself (`canvas::shortcuts`), for the View menu, tooltips and
/// Help ▸ Keyboard shortcuts. Keep them in step with the bindings there.
pub(crate) const ACTUAL_SIZE: Shortcut = Shortcut::cmd("1");
pub(crate) const PAGE_LEVEL: Shortcut = Shortcut::cmd("0");
pub(crate) const FIT_WIDTH: Shortcut = Shortcut::cmd("2");
pub(crate) const ZOOM_IN: Shortcut = Shortcut::cmd("+");
pub(crate) const ZOOM_OUT: Shortcut = Shortcut::cmd("−");
pub(crate) const ROTATE_CW: Shortcut = Shortcut::cmd_shift("+");
pub(crate) const ROTATE_CCW: Shortcut = Shortcut::cmd_shift("−");
pub(crate) const PREV_VIEW: Shortcut = Shortcut::cmd("[");
pub(crate) const NEXT_VIEW: Shortcut = Shortcut::cmd("]");
pub(crate) const FIND_NEXT: Shortcut = Shortcut::cmd("G");
pub(crate) const FIND_PREV: Shortcut = Shortcut::cmd_shift("G");
pub(crate) const COPY: Shortcut = Shortcut::cmd("C");
pub(crate) const SELECT_ALL: Shortcut = Shortcut::cmd("A");
pub(crate) const PAGE_PREV: Shortcut = Shortcut::cmd("←");
pub(crate) const PAGE_NEXT: Shortcut = Shortcut::cmd("→");

/// Whether shortcuts are written the macOS way (`⇧⌘S`) rather than `Ctrl+Shift+S`. egui knows the
/// platform from the build target, and on the web from the browser's user agent.
pub(crate) fn mac_shortcuts(ctx: &egui::Context) -> bool {
    ctx.os() == egui::os::OperatingSystem::Mac
}

/// How `s` is written on this platform, using the resolved current interface language.
pub(crate) fn shortcut_label(ctx: &egui::Context, s: Shortcut) -> String {
    let key = crate::i18n::key_name(s.key);
    if mac_shortcuts(ctx) {
        Shortcut { key, ..s }.label(true)
    } else {
        let mut parts = Vec::new();
        if s.command || s.mac_ctrl {
            parts.push(crate::i18n::key_name("Ctrl"));
        }
        if s.shift {
            parts.push(crate::i18n::key_name("Shift"));
        }
        parts.push(key);
        parts.join("+")
    }
}

/// The command modifier for help text such as “Strg-scroll”; bindings stay in the engine.
pub(crate) fn command_modifier_label(ctx: &egui::Context) -> &'static str {
    if mac_shortcuts(ctx) { Shortcut::command_name(true) } else { crate::i18n::key_name("Ctrl") }
}

/// How a registered command's shortcut is written on this platform ("" without one).
pub(crate) fn command_shortcut_label(ctx: &egui::Context, id: &str) -> String {
    commands::command(id).and_then(|c| c.shortcut).map(|s| shortcut_label(ctx, s)).unwrap_or_default()
}

/// A translated tooltip with a `{key}` placeholder, filled with `s` as written on this platform:
/// "Zoom in ({key})" → "Zoom in (Ctrl++)".
pub(crate) fn key_tip(ctx: &egui::Context, template: &str, s: Shortcut) -> String {
    crate::i18n::fmt(template, &[("key", &shortcut_label(ctx, s))])
}

/// [`key_tip`] with a registered command's shortcut, so the tooltip names the real binding.
pub(crate) fn command_tip(ctx: &egui::Context, template: &str, id: &str) -> String {
    crate::i18n::fmt(template, &[("key", &command_shortcut_label(ctx, id))])
}

impl PdfCraftApp {
    /// Whether a registered command can run now. The engine judges the document (security,
    /// contents, undo history); view state it can't see is checked here.
    pub(crate) fn command_enabled(&self, spec: &CommandSpec) -> bool {
        if !self.command_allowed(spec.id) {
            return false;
        }
        // Added text is granted on its own. The command's usual "document can be modified" check
        // stays false while view-only, so it must not hide this one action.
        if spec.id == "edit.text" && self.active_ids().is_some_and(|(_, id)| self.session.allows_added_text(id)) {
            return true;
        }
        // Image stamps likewise. `allows_annotation` is false for every view-only document, which
        // is this app's mode and not the PDF's security. The PDF's own comment permission, and a
        // signature, are still checked by [`pdfcraft_engine::Session::allows_stamp`].
        if spec.id == "comment.stamp" && self.active_ids().is_some_and(|(_, id)| self.session.allows_stamp(id)) {
            return true;
        }
        // Current-page rotation is granted on its own. `allows_assembly` stays false while
        // view-only, which is this app's mode. The PDF's page permission and the signature
        // guard are what [`pdfcraft_engine::Session::allows_page_rotate`] still checks.
        if matches!(spec.id, "page.rotate" | "page.rotate_ccw")
            && self.session.view_only()
            && self.active_ids().is_some_and(|(_, id)| self.session.allows_page_rotate(id))
        {
            return true;
        }
        // Undo and Redo act on the Combine files list while its tab shows.
        if self.combine_showing() && matches!(spec.needs, commands::Needs::Undo | commands::Needs::Redo) {
            return self.combine_can_undo(spec.needs == commands::Needs::Undo);
        }
        commands::is_enabled(spec, &self.session, self.active_ids().map(|(_, id)| id))
            && (spec.id != "edit.copy"
                || self.active.and_then(|i| self.views.get(i)).is_some_and(|v| !v.organize && v.selected_text().is_some_and(|t| !t.is_empty())))
            && (spec.needs != commands::Needs::TwoPageView || self.active.and_then(|i| self.views.get(i)).is_some_and(crate::DocView::cover_applies))
            && match spec.id {
                "view.previous_view" => self.active.and_then(|i| self.views.get(i)).is_some_and(|v| !v.back.is_empty()),
                "view.next_view" => self.active.and_then(|i| self.views.get(i)).is_some_and(|v| !v.forward.is_empty()),
                _ => true,
            }
    }

    /// Why the current document cannot have its page rotated into the PDF.
    pub(crate) fn page_rotate_block_message(&self) -> String {
        let availability = self.active_ids().map(|(_, id)| self.session.page_rotate_availability(id));
        match availability {
            Some(pdfcraft_engine::PageRotateAvailability::Signature) => page_rotate_signature_message(),
            Some(pdfcraft_engine::PageRotateAvailability::NotPermitted) => {
                crate::identity::localized("この PDF の権限ではページを変更できません。", "The document's security settings don't allow this change")
            }
            Some(pdfcraft_engine::PageRotateAvailability::Unreadable) => {
                crate::identity::localized("この PDF はページを回転できる状態で開けていません。", "This PDF could not be opened for page rotation.")
            }
            _ => tl!("Open a document first").to_string(),
        }
    }

    /// Run a registered command by id. Returns `false` when the id is unknown or the command
    /// is disabled right now (the user is told why).
    ///
    /// Last-resort guard (AGENTS.md §4): a command that panics is reported and the app, with its
    /// open documents, keeps running. Edits are applied to a copy, so the document is unchanged.
    pub fn execute(&mut self, id: &str) -> bool {
        match pdfcraft_engine::guard(|| self.execute_unguarded(id)) {
            Ok(done) => done,
            Err(m) => {
                self.notify_fmt("That didn't work: an internal error stopped it ({m}). Your documents are unchanged.", &[("m", m.as_str())]);
                true
            }
        }
    }

    fn execute_unguarded(&mut self, id: &str) -> bool {
        if !self.command_allowed(id) {
            self.notify(crate::command_policy::REFUSED);
            return false;
        }
        let Some(spec) = commands::command(id) else { return false };
        if !self.command_enabled(spec) {
            if matches!(id, "page.rotate" | "page.rotate_ccw")
                && self.session.view_only()
                && self.session.allows(pdfcraft_engine::Capability::PageRotate)
            {
                self.notify(self.page_rotate_block_message());
                return false;
            }
            let why = match spec.needs {
                commands::Needs::Undo => tl!("Nothing to undo").to_string(),
                commands::Needs::Redo => tl!("Nothing to redo").to_string(),
                commands::Needs::FillForms if self.active.is_some() => tl!("This document has no form fields you can fill in").to_string(),
                commands::Needs::HasComments if self.active.is_some() => tl!("This document has no comments to flatten").to_string(),
                commands::Needs::HasFields if self.active.is_some() => tl!("This document has no form fields to flatten").to_string(),
                commands::Needs::HasRedactions if self.active.is_some() => {
                    tl!("There are no redaction marks (mark text, areas or pages first)").to_string()
                }
                commands::Needs::Marks(k) if self.active.is_some() => {
                    let kind = match k {
                        pdfcraft_engine::MarkKind::HeaderFooter => tl!("header or footer"),
                        pdfcraft_engine::MarkKind::Watermark => tl!("watermark"),
                        pdfcraft_engine::MarkKind::Background => tl!("background"),
                    };
                    crate::i18n::fmt(tl!("This document has no {kind} to change"), &[("kind", kind)])
                }
                // Restricted documents ask for their permissions password instead, so only an
                // unprotected or a read-only document gets here.
                commands::Needs::Security | commands::Needs::ProtectedSecurity if self.active.is_some() => {
                    match self.active_ids().and_then(|(_, id)| self.session.get(id)) {
                        Some(d) if !d.allows_security_change() => crate::i18n::fmt(tl!("{name} is read-only"), &[("name", &d.name)]),
                        _ => tl!("This document isn't password-protected").to_string(),
                    }
                }
                commands::Needs::Unlock if self.active.is_some() => tl!("This document has no restrictions to lift").to_string(),
                commands::Needs::Annotate if spec.id == "comment.stamp" && self.active.is_some() => self
                    .active
                    .and_then(|index| self.stamp_refusal(index))
                    .unwrap_or_else(|| tl!("The document's security settings don't allow this change").to_string()),
                commands::Needs::Assembly | commands::Needs::Modification | commands::Needs::Annotate if self.active.is_some() => {
                    tl!("The document's security settings don't allow this change").to_string()
                }
                commands::Needs::Copying if self.active.is_some() => crate::copy_refused().to_string(),
                commands::Needs::TwoPageView if self.active.is_some() => tl!("Switch to two-page view first to show the cover page").to_string(),
                _ => tl!("Open a document first").to_string(),
            };
            self.notify(why);
            return false;
        }
        let active = self.active;
        let targets = active.map(|i| self.views[i].target_pages()).unwrap_or_default();
        match id {
            "ui.dock.reset" => {
                if let Err(error) = crate::docking::command(self, true, &serde_json::Value::Null) {
                    self.notify(error);
                }
            }
            "file.open" => self.open_dialog(),
            "file.open_recent" => match self.recent.first().map(|r| r.path.clone()) {
                // The palette runs commands without a submenu: open the most recent file.
                Some(p) => self.open_recent(&p),
                None => self.notify_tr("No recent files"),
            },
            "file.clear_recent" if self.recent.is_empty() => self.notify_tr("No recent files"),
            "file.clear_recent" => self.recent.clear(),
            "file.pin_folder" => self.pin_folder_dialog(),
            "page.combine" => self.open_combine_tab(),
            "file.save" => {
                self.save_active(SaveTarget::InPlace);
            }
            "file.save_as" if self.session.view_only() => {
                self.begin_save_copy();
            }
            "file.save_as" => {
                self.save_active(SaveTarget::As);
            }
            "file.close" => {
                if let Some(i) = active {
                    self.request_close_tab(i);
                }
            }
            "file.close_all" => self.close_all(),
            "file.revert" => self.dialog = Some(Dialog::Revert),
            "file.properties" => self.dialog = Some(Dialog::Properties(PropsTab::Description)),
            "file.word_count" => self.dialog = Some(Dialog::WordCount),
            "protect.properties" => self.dialog = Some(Dialog::Properties(PropsTab::Security)),
            // A restricted document's security changes once its permissions password is entered.
            "protect.password" | "protect.remove" if self.active_ids().and_then(|(_, id)| self.session.get(id)).is_some_and(|d| d.can_unlock()) => {
                let then = if id == "protect.password" { "protect.password" } else { "protect.remove" };
                self.request_unlock(Some(then));
            }
            "protect.unlock" => self.request_unlock(None),
            "protect.password" => {
                self.protect_draft = Default::default();
                self.dialog = Some(Dialog::Protect);
            }
            "protect.remove" => {
                if self.apply_edit(Edit::RemoveProtection) {
                    self.notify_tr("Security will be removed when you save");
                }
            }
            "page.number" => {
                if let Some(i) = active {
                    let v = &self.views[i];
                    let pages: Vec<usize> = if v.selected.is_empty() { vec![v.current] } else { v.selected.iter().copied().collect() };
                    let (lo, hi) = (pages.iter().min().copied().unwrap_or(0), pages.iter().max().copied().unwrap_or(0));
                    self.number_draft = crate::NumberDraft { from: lo + 1, to: hi + 1, ..self.number_draft.clone() };
                }
                self.dialog = Some(Dialog::NumberPages);
            }
            link if pdfcraft_engine::links::for_command(link).is_some() => {
                if let Some(l) = pdfcraft_engine::links::for_command(link) {
                    self.open_url(l.url);
                }
            }
            "bookmark.add" => self.bookmark_action(crate::panels::BmAction::New),
            "bookmark.from_structure" => self.bookmark_action(crate::panels::BmAction::FromStructure),
            "edit.undo" => self.undo(),
            "edit.redo" => self.redo(),
            "edit.find" => {
                if let Some(i) = active {
                    self.views[i].open_find();
                }
            }
            "view.focus_page_input" => {
                if let (Some(ctx), Some(view)) = (self.ctx.clone(), active.and_then(|i| self.views.get_mut(i))) {
                    // The page box in the toolbar (chrome.rs); select its label so typing replaces it.
                    let id = view.page_input_id();
                    if !ctx.memory(|m| m.has_focus(id))
                        && let Some(doc) = self.session.get(view.id)
                    {
                        // Navigation and this command can arrive before the next rail frame.
                        view.page_input = view.current_page_label(doc.info.pages.get(view.current).map(|p| p.label.as_str()));
                    }
                    ctx.memory_mut(|m| m.request_focus(id));
                    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
                    let len = view.page_input.chars().count();
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len))));
                    state.store(&ctx, id);
                }
            }
            "view.palette" => self.palette_open = !self.palette_open,
            layout if crate::canvas::PageLayout::from_command(layout).is_some() => {
                if let (Some(i), Some(layout)) = (active, crate::canvas::PageLayout::from_command(layout)) {
                    self.views[i].set_layout(layout);
                }
            }
            // The Continuous scrolling checkbox (an independent toggle over the base layout).
            "view.continuous" => {
                if let Some(i) = active {
                    let v = &mut self.views[i];
                    v.set_continuous(!v.continuous());
                }
            }
            "view.layout.cover" => {
                if let Some(i) = active {
                    let v = &mut self.views[i];
                    v.set_cover(!v.cover);
                }
            }
            "view.fit_width_scrolling" | "view.fit_one_page" => {
                use crate::canvas::{Fit, PageLayout};
                // Fit one full page shows a single page on its own; fit width scrolls it.
                let (continuous, fit) = if id == "view.fit_one_page" { (false, Fit::Page) } else { (true, Fit::Width) };
                if let Some(i) = active {
                    self.views[i].set_display(PageLayout::Single, continuous);
                    self.views[i].set_fit(fit);
                }
            }
            "view.full_screen" => {
                let on = !self.full_screen;
                match self.ctx.clone() {
                    Some(ctx) => self.set_full_screen(&ctx, on),
                    None => self.full_screen = on,
                }
            }
            "view.read_mode" => self.switch_mode(if self.mode == Mode::Read { Mode::AllTools } else { Mode::Read }, false),
            "view.previous_view" | "view.next_view" => {
                if let Some(v) = active.and_then(|i| self.views.get_mut(i)) {
                    return v.view_history(id == "view.next_view");
                }
            }
            "view.next_tab" => self.step_tab(true),
            "view.previous_tab" => self.step_tab(false),
            "view.theme" => {
                let next = if self.theme == ThemeKind::Light { ThemePreference::Dark } else { ThemePreference::Light };
                self.set_theme_preference(next);
            }
            command if command.starts_with("measure.") => crate::measure_ui::command(self, command),
            "view.theme.system" => self.set_theme_preference(ThemePreference::System),
            "view.theme.light" => self.set_theme_preference(ThemePreference::Light),
            "view.theme.dark" => self.set_theme_preference(ThemePreference::Dark),
            "comment.list" => self.choose_right_panel(Some(RightPanel::Comments)),
            tool if crate::comments::CommentTool::from_command(tool).is_some() => {
                let Some(tool) = crate::comments::CommentTool::from_command(tool) else { return false };
                self.comment_prefs.group_tool[tool.group()] = tool;
                self.quick_tool = crate::QuickTool::Comment(tool);
                // Acrobat opens the Comments panel with the commenting tools, unless the user closed it.
                if self.right.is_none() && !self.comments_panel_closed {
                    self.choose_right_panel(Some(RightPanel::Comments));
                }
                // A text selection made before picking a markup tool is marked right away.
                if let (Some(kind), Some(i)) = (tool.markup(), active)
                    && let Some(doc) = self.session.get(self.views[i].id)
                {
                    let info = &doc.info;
                    if let Some((page, quads)) = self.views[i].selection_quads(info) {
                        self.views[i].clear_selection();
                        let style = self.comment_prefs.style(tool);
                        let author = self.comment_prefs.author.clone();
                        let shape = pdfcraft_engine::Shape::TextMarkup { kind, quads };
                        self.apply_edit(Edit::AddAnnotation(pdfcraft_engine::NewAnnotation { page, shape, style, contents: String::new(), author }));
                    }
                }
            }
            "form.fields" => self.choose_right_panel(Some(RightPanel::Fields)),
            "comment.flatten" => {
                self.apply_edit(Edit::Flatten { comments: true, fields: false });
            }
            "form.flatten" => {
                // Flatten what's typed in a field too (#166); a refused value flattens nothing.
                if self.commit_form_typing() {
                    self.apply_edit(Edit::Flatten { comments: false, fields: true });
                }
            }
            "form.clear" => {
                if let Some(i) = active {
                    self.views[i].forms.focus = None;
                }
                self.apply_edit(Edit::ResetForm { names: None });
            }
            "page.organize" => {
                if let Some(v) = active.and_then(|i| self.views.get_mut(i)) {
                    if v.organize {
                        v.organize = false;
                    } else {
                        v.open_organize();
                    }
                }
            }
            "page.rotate" | "page.rotate_ccw" => {
                // The page on screen, not the thumbnail selection. Other page commands keep `targets`.
                let degrees = if id == "page.rotate" { 90 } else { -90 };
                if let Some(page) = active.and_then(|i| self.views.get(i)).map(|view| view.current) {
                    self.apply_edit(Edit::RotatePages { pages: vec![page], degrees });
                }
            }
            "page.delete" => {
                self.apply_edit(Edit::DeletePages { pages: targets });
            }
            "page.insert_blank" => {
                if let (Some(i), Some(&last)) = (active, targets.last())
                    && let Some(p) = self.session.get(self.views[i].id).and_then(|d| d.info.pages.get(last))
                {
                    let (w, h) = ((p.crop[2] - p.crop[0]).abs().max(1.0) as f64, (p.crop[3] - p.crop[1]).abs().max(1.0) as f64);
                    self.apply_edit(Edit::InsertBlankPage { at: last + 1, width: w, height: h });
                }
            }
            "page.insert" => self.insert_from_file_dialog(),
            "page.replace" => self.replace_pages_dialog(),
            "edit.header_footer"
            | "edit.watermark"
            | "edit.background"
            | "edit.header_footer.update"
            | "edit.watermark.update"
            | "edit.background.update" => {
                use pdfcraft_engine::MarkKind as K;
                let kind = if id.starts_with("edit.header_footer") {
                    K::HeaderFooter
                } else if id.starts_with("edit.watermark") {
                    K::Watermark
                } else {
                    K::Background
                };
                self.marks_draft.replace = id.ends_with(".update");
                self.dialog = Some(Dialog::Marks(kind));
            }
            "edit.bates" => {
                // Bates numbering ▸ Add: the header & footer dialog with a Bates number in the
                // bottom-right box (6 digits from 1), ready to edit.
                self.marks_draft.replace = false;
                if self.marks_draft.hf.text.iter().all(|t| !t.contains("<<Bates")) {
                    self.marks_draft.hf.text[5] = "<<Bates Number#6#1##>>".into();
                }
                self.marks_draft.focused_box = 5;
                self.dialog = Some(Dialog::Marks(pdfcraft_engine::MarkKind::HeaderFooter));
            }
            "edit.header_footer.remove" | "edit.watermark.remove" | "edit.background.remove" => {
                use pdfcraft_engine::MarkKind as K;
                let kind = match id {
                    "edit.header_footer.remove" => K::HeaderFooter,
                    "edit.watermark.remove" => K::Watermark,
                    _ => K::Background,
                };
                self.apply_edit(Edit::RemoveMarks { kind });
            }
            "export.image" => self.dialog = Some(Dialog::Export(crate::export_ui::ExportKind::Image)),
            "export.text" => self.dialog = Some(Dialog::Export(crate::export_ui::ExportKind::Text)),
            "a11y.check" => self.start_accessibility_check(),
            "a11y.find_untagged" => self.find_untagged_content(),
            "ocr.recognize" => self.dialog = Some(Dialog::RecognizeText),
            "tools.js_console" => self.dialog = Some(Dialog::JsConsole),
            "tools.document_js" => {
                self.doc_js = Default::default();
                self.dialog = Some(Dialog::DocumentJs);
            }
            "app.preferences" => self.dialog = Some(Dialog::Preferences),
            "ocr.recognize_batch" => self.ocr_files_dialog(),
            "edit.edit_text" => {
                self.quick_tool = crate::QuickTool::EditText;
                self.left = crate::LeftPanel::Tool("edit");
                self.reveal_tools();
                self.notify_tr("Click text or an image to edit it");
            }
            "edit.advanced_search" => {
                if let Some(i) = self.active {
                    let f = self.views[i].find.get_or_insert_with(Default::default);
                    f.in_panel = true;
                    f.focus = true;
                    self.choose_right_panel(Some(crate::RightPanel::Search));
                }
            }
            "a11y.report" => self.show_accessibility_report(),
            "a11y.autotag" => {
                if self.apply_edit(pdfcraft_engine::Edit::AutoTagText) {
                    self.choose_right_panel(Some(RightPanel::Tags));
                }
            }
            "a11y.alt_text" => self.start_alt_text(),
            "a11y.autotag_fields" => self.autotag_form_fields(),
            "a11y.tags" => self.choose_right_panel(Some(RightPanel::Tags)),
            "a11y.reading_options" => self.dialog = Some(Dialog::Properties(crate::PropsTab::Advanced)),
            "export.all_images" => self.dialog = Some(Dialog::Export(crate::export_ui::ExportKind::AllImages)),
            "edit.text" => {
                if self.session.view_only() {
                    self.begin_text_entry();
                } else {
                    self.quick_tool = crate::QuickTool::AddText;
                    self.left = crate::LeftPanel::Tool("edit");
                    self.reveal_tools();
                }
            }
            "edit.image" => self.add_image_dialog(),
            "edit.link" => {
                self.quick_tool = crate::QuickTool::Link;
                self.notify_tr("Drag a rectangle to create a link; double-click a link to edit it");
            }
            "edit.links_from_urls" => self.links_from_urls(),
            "edit.remove_links" => self.remove_all_links(),
            "redact.mark" => {
                self.quick_tool = crate::QuickTool::Redact;
                self.left = crate::LeftPanel::Tool("redact");
                self.reveal_tools();
                // A text selection made first is marked right away.
                if let Some(i) = active
                    && let Some(doc) = self.session.get(self.views[i].id)
                {
                    let info = &doc.info;
                    if let Some((page, quads)) = self.views[i].selection_quads(info) {
                        self.views[i].clear_selection();
                        let author = self.comment_prefs.author.clone();
                        self.apply_edit(self.redact_prefs.mark(page, quads, &author));
                    }
                }
            }
            "redact.pages" => {
                if let Some(i) = active {
                    let n = self.session.get(self.views[i].id).map_or(1, |d| d.info.pages.len());
                    self.redact_pages_draft = crate::RedactPagesDraft { current: true, from: self.views[i].current + 1, to: n };
                }
                self.dialog = Some(Dialog::RedactPages);
            }
            "redact.search" => {
                self.redact_search.found = None;
                self.dialog = Some(Dialog::RedactSearch);
            }
            "redact.properties" => {
                self.redact_before = Some(self.redact_prefs.clone());
                self.dialog = Some(Dialog::RedactProps);
            }
            "redact.apply" => {
                let marks = active.and_then(|i| self.session.get(self.views[i].id)).map_or(0, |d| d.redaction_marks());
                if marks == 0 {
                    self.notify_tr("There are no redaction marks to apply");
                } else {
                    self.dialog = Some(Dialog::RedactApply);
                }
            }
            "protect.remove_hidden" => self.open_remove_hidden(),
            "print.dialog" => self.open_print(),
            "redact.sanitize" => self.dialog = Some(Dialog::Sanitize),
            "redact.clear" => {
                self.apply_edit(Edit::ClearRedactions);
            }
            "form.tab_order.row" | "form.tab_order.column" | "form.tab_order.structure" => {
                let order = match id {
                    "form.tab_order.row" => pdfcraft_engine::TabOrder::Row,
                    "form.tab_order.column" => pdfcraft_engine::TabOrder::Column,
                    _ => pdfcraft_engine::TabOrder::Structure,
                };
                let n = active.and_then(|i| self.session.get(self.views[i].id)).map_or(0, |d| d.info.pages.len());
                self.apply_edit(Edit::SetTabOrder { pages: (0..n).collect(), order });
            }
            "comment.import" | "form.import_data" => self.import_data_dialog(),
            "comment.stamp" => {
                self.left = crate::LeftPanel::Tool("stamp");
                self.reveal_tools();
            }
            "comment.export" => self.export_data_dialog(true, false),
            "comment.summarize" => self.dialog = Some(Dialog::SummarizeComments),
            "sign.digital" | "sign.certify" => {
                let certify = id == "sign.certify";
                self.quick_tool = crate::QuickTool::SignArea { certify };
                self.notify_tr("Drag to draw the area where the signature should appear.");
            }
            "sign.certify_invisible" => {
                let page = active.map_or(0, |i| self.views[i].current);
                self.start_signing(page, None, None, Some(2));
            }
            "sign.validate" => {
                let certs = self.session.trusted_certificates().to_vec();
                self.apply_trusted_certificates(certs);
                self.choose_right_panel(Some(RightPanel::Signatures));
            }
            "sign.panel" => self.choose_right_panel(Some(RightPanel::Signatures)),
            "optimize.advanced" => self.dialog = Some(Dialog::Optimize),
            "view.fit_visible" => {
                if let Some(i) = self.active
                    && let Err(e) = self.fit_visible(i)
                {
                    self.notify_fmt("Couldn't fit the visible content: {e}", &[("e", &e.to_string())]);
                }
            }
            "view.marquee_zoom" => self.quick_tool = crate::QuickTool::MarqueeZoom,
            "edit.column_select" => self.quick_tool = crate::QuickTool::ColumnSelect,
            "edit.snapshot" => {
                self.quick_tool = crate::QuickTool::Snapshot;
                self.notify_tr("Drag a rectangle around the area to copy");
            }
            "edit.copy" => {
                if let (Some(i), Some(ctx)) = (active, self.ctx.clone()) {
                    return self.copy_selected_text(i, &ctx);
                }
                return false;
            }
            "page.copy" => self.copy_pages(false),
            "page.cut" => self.copy_pages(true),
            "page.paste" => self.paste_pages(),
            "comment.hide_all" => {
                if let Some(i) = active {
                    let id = self.views[i].id;
                    let hide = !self.session.get(id).is_some_and(|d| d.comments_hidden());
                    if self.session.set_hide_comments(id, hide) {
                        self.views[i].invalidate_content();
                    }
                }
            }
            "form.export_data" => self.export_data_dialog(false, true),
            "form.merge_data" => self.merge_data_dialog(),
            "export.docx" => self.export_office_dialog(pdfcraft_engine::compare::OfficeFormat::Docx),
            "export.pptx" => self.export_office_dialog(pdfcraft_engine::compare::OfficeFormat::Pptx),
            "export.html" => self.export_office_dialog(pdfcraft_engine::compare::OfficeFormat::Html),
            "export.rtf" => self.export_office_dialog(pdfcraft_engine::compare::OfficeFormat::Rtf),
            "form.prepare" => {
                self.left = crate::LeftPanel::Tool("form");
                self.reveal_tools();
                self.choose_right_panel(Some(RightPanel::Fields));
                // Like Acrobat, a document without fields gets them detected on the way in.
                if let Some((_, id)) = self.active_ids()
                    && self.session.get(id).is_some_and(|d| d.form.is_empty() && d.editable())
                {
                    self.detect_fields();
                }
            }
            "form.detect" => self.detect_fields(),
            "doc.compare" => self.dialog = Some(Dialog::CompareFiles),
            "standards.pdfa" => {
                self.pdfa.issues = None;
                self.dialog = Some(Dialog::PdfA);
            }
            "actions.wizard" | "actions.distribution" | "actions.optimize_scans" => {
                self.wizard.editing = None;
                match id {
                    "actions.distribution" => self.wizard.selected = Some("Prepare for Distribution".into()),
                    "actions.optimize_scans" => self.wizard.selected = Some("Optimize Scanned Documents".into()),
                    _ => {}
                }
                self.dialog = Some(Dialog::ActionWizard);
            }
            "form.field.properties" => {
                if let Some((name, w)) = active.and_then(|i| self.views[i].prepare.selected.clone()) {
                    self.open_field_props(&name, w);
                }
            }
            field if crate::prepare::FieldTool::from_command(field).is_some() => {
                let Some(tool) = crate::prepare::FieldTool::from_command(field) else { return false };
                self.quick_tool = crate::QuickTool::Field(tool);
                self.left = crate::LeftPanel::Tool("form");
                self.reveal_tools();
                if let Some(i) = active {
                    self.views[i].forms.focus = None;
                }
                self.notify_fmt(
                    "Click on the page to add a {tool}, or drag to set its size",
                    &[("tool", &crate::i18n::in_sentence(tl!(tool.label())))],
                );
            }
            "sign.fill.signature.remove" => self.signature = None,
            "sign.fill.initials.remove" => self.initials = None,
            "sign.fill.signature.change" | "sign.fill.initials.change" => {
                let initials = id == "sign.fill.initials.change";
                let saved = if initials { self.initials.as_ref() } else { self.signature.as_ref() };
                self.signature_draft = saved.map_or_else(
                    || crate::fill_sign::SigDraft::new(initials, &self.comment_prefs.author),
                    |s| crate::fill_sign::SigDraft::from_saved(initials, s),
                );
                self.dialog = Some(Dialog::Signature);
            }
            fill if crate::fill_sign::FillTool::from_command(fill).is_some() => {
                let Some(tool) = crate::fill_sign::FillTool::from_command(fill) else { return false };
                self.quick_tool = crate::QuickTool::Fill(tool);
                let initials = tool == crate::fill_sign::FillTool::Initials;
                if (tool == crate::fill_sign::FillTool::Signature && self.signature.is_none()) || (initials && self.initials.is_none()) {
                    self.signature_draft = crate::fill_sign::SigDraft::new(initials, &self.comment_prefs.author);
                    self.dialog = Some(Dialog::Signature);
                }
            }
            "create.blank" => self.create_blank(),
            "create.file" => self.open_dialog(),
            "create.multiple" => self.create_multiple_dialog(),
            "create.multiple_separate" => self.create_multiple_separate_dialog(),
            "create.images" => self.create_from_images_dialog(),
            "create.clipboard" => self.create_from_clipboard(),
            "optimize.reduce" => self.reduce_file_size(),
            "page.duplicate" => {
                self.apply_edit(Edit::DuplicatePages { pages: targets });
            }
            "ocr.enhance_camera" => {
                // The corners start on the document's edges when they can be found, else
                // inset on the current page (camera_ui::page_input).
                self.quick_tool = crate::QuickTool::EnhanceCamera;
                if let Some(i) = self.active
                    && let Some(view) = self.views.get_mut(i)
                    && let Some(page) = self.session.get(view.id).and_then(|d| d.info.pages.get(view.current).cloned())
                {
                    view.camera_done = None;
                    view.camera = self.session.detect_camera_corners(view.id, view.current).map(|found| crate::camera_ui::CameraQuad {
                        page: view.current,
                        corners: found.map(|[x, y]| page.user_to_view(x as f32, y as f32)),
                        grab: None,
                    });
                }
                self.notify_tr("Drag the corners onto the document's edges, then choose Enhance page.");
            }
            "page.crop" => {
                self.quick_tool = crate::QuickTool::Crop;
                self.notify_tr("Drag a rectangle on a page to crop it; double-click a page for Set Page Boxes");
            }
            "page.boxes" => {
                self.boxes_draft.seeded = None;
                self.dialog = Some(Dialog::PageBoxes);
            }
            "page.extract" => self.open_extract_dialog(),
            "page.rotate_dialog" => {
                if let Some(i) = active {
                    let n = self.session.get(self.views[i].id).map_or(1, |d| d.info.pages.len());
                    self.rotate_draft.to = n;
                    self.rotate_draft.which = if self.views[i].selected.is_empty() { 0 } else { 1 };
                }
                self.dialog = Some(Dialog::RotatePages);
            }
            "page.split" => self.dialog = Some(Dialog::Split),
            "help.shortcuts" => self.dialog = Some(Dialog::Shortcuts),
            "help.about" => self.dialog = Some(Dialog::About),
            "help.check_updates" => self.check_for_updates(),
            _ => return false,
        }
        true
    }

    /// Shows the next tab (or the previous one) in the strip's order, wrapping around at either
    /// end (#1542). The Combine files tab, when open, comes after the documents. From Home the
    /// next tab is the first and the previous one the last.
    fn step_tab(&mut self, forward: bool) {
        let count = self.views.len() + usize::from(self.combine_tab.open);
        let current = if self.combine_showing() { Some(self.views.len()) } else { self.active };
        let Some(next) = tab_after(current, count, forward) else { return };
        if next < self.views.len() {
            self.active = Some(next);
        } else {
            self.open_combine_tab();
        }
    }

    /// Run the registered keyboard shortcuts (more specific combinations first).
    pub(crate) fn registry_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        // Buttons also own keyboard focus for Tab/Enter activation, but they must not
        // suppress document shortcuts. Text editors retain their own undo/redo handling.
        let typing = ctx.text_edit_focused();
        // A form field's editor is open on the page: its text isn't in the document until
        // committed. (Not egui's keyboard focus: an Escape in this frame has already cleared that,
        // while the field has yet to see the Escape and discard its draft.)
        let active = self.active_ids();
        let form_typing = active.and_then(|(i, _)| self.views.get(i)).is_some_and(|v| v.forms.focus.is_some());
        let mut specs: Vec<&CommandSpec> = COMMANDS.iter().filter(|c| c.shortcut.is_some()).collect();
        specs.sort_by_key(|c| std::cmp::Reverse(c.shortcut.map(|s| s.modifier_count()).unwrap_or(0)));
        for spec in specs {
            let Some(s) = spec.shortcut else { continue };
            // Keep the canvas's silent no-op when there is no previous/next view to visit.
            if matches!(spec.id, "view.previous_view" | "view.next_view") && !self.command_enabled(spec) {
                continue;
            }
            if typing && !spec.in_text {
                continue;
            }
            let Some(key) = Key::from_name(s.key) else { continue };
            let mut m = Modifiers::NONE;
            if s.command {
                m |= Modifiers::COMMAND;
            }
            if s.shift {
                m |= Modifiers::SHIFT;
            }
            if s.mac_ctrl {
                m |= Modifiers::CTRL;
            }
            if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, key))) {
                if form_typing {
                    // A form field has the keyboard: let it take this frame's typing (and
                    // Escape) first, so ⌘S saves what's on screen (#166). Runs next frame, for
                    // this document only.
                    self.deferred_commands.push((spec.id, active.map(|(_, id)| id)));
                    ctx.request_repaint();
                } else if spec.id == "file.close" && active.is_none() && crate::commands::mac_shortcuts(ctx) && !cfg!(target_arch = "wasm32") {
                    // macOS ⌘W closes the last window when no document tabs remain (#869).
                    // Route through the normal viewport-close path (and its quit guard),
                    // not the disabled document-only `file.close` command.
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    self.execute(spec.id);
                }
            }
        }
        // Redo is Ctrl+Shift+Z in the menu. Windows and Linux also use Ctrl+Y; macOS keeps
        // Cmd+Shift+Z. A focused text editor keeps its own Ctrl+Y.
        #[cfg(not(target_os = "macos"))]
        if !typing && ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y))) {
            if form_typing {
                self.deferred_commands.push(("edit.redo", active.map(|(_, id)| id)));
                ctx.request_repaint();
            } else {
                self.execute("edit.redo");
            }
        }
    }
}

/// The tab after `current` of `count` (before it when `forward` is false), wrapping around. From
/// no tab, forward is the first and back the last. `None` when there are no tabs.
fn tab_after(current: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    let last = count.checked_sub(1)?;
    Some(match (current.filter(|&i| i <= last), forward) {
        (None, true) => 0,
        (None, false) => last,
        (Some(i), true) => {
            if i == last {
                0
            } else {
                i + 1
            }
        }
        (Some(i), false) => i.checked_sub(1).unwrap_or(last),
    })
}

/// Name shared by the Pages menu and the command palette. The command id stays `page.rotate`.
pub(crate) fn page_rotate_menu_label(id: &str) -> Option<String> {
    let source = match id {
        "page.rotate" => "Rotate current page clockwise 90°",
        "page.rotate_ccw" => "Rotate current page counterclockwise 90°",
        _ => return None,
    };
    Some(crate::i18n::t(source).to_string())
}

pub(crate) fn page_rotate_menu_tip() -> String {
    crate::i18n::t("Turns the current page by 90°. You can save the result as a new PDF.").to_string()
}

/// Tooltip for display-only rotation. `key` is the shortcut when the control shows one.
/// This does not change `/Rotate`. A missing translation falls back to the English sentence.
pub(crate) fn view_rotate_tip(clockwise: bool, key: Option<&str>) -> String {
    let template = match (clockwise, key.is_some()) {
        (true, true) => crate::i18n::t("Rotate the view 90° clockwise (not saved in the PDF) ({key})"),
        (true, false) => crate::i18n::t("Rotate the view 90° clockwise (not saved in the PDF)"),
        (false, false) => crate::i18n::t("Rotate the view 90° counterclockwise (not saved in the PDF)"),
        (false, true) => crate::i18n::t("Rotate the view 90° counterclockwise (not saved in the PDF) ({key})"),
    };
    match key {
        Some(key) => crate::i18n::fmt(template, &[("key", key)]),
        None => template.to_string(),
    }
}

/// Refusal text for a signature-shaped PDF, or one whose repaired structure leaves that open.
/// It does not say the signature verifies.
pub(crate) fn page_rotate_signature_message() -> String {
    crate::identity::localized(
        "ページの回転はできません。この PDF には電子署名があるか、その可能性があります。内部構造が修復された PDF も、署名の有無を確かめられないため、安全のため回転できません。署名が有効かどうかは確認していません。",
        pdfcraft_engine::PAGE_ROTATE_SIGNATURE,
    )
}

/// Render a top-level menu's registered commands (with live labels, shortcuts and enablement).
pub(crate) fn registry_menu(app: &mut PdfCraftApp, ui: &mut egui::Ui, menu: &str) {
    for spec in commands::menu(menu) {
        if app.session.view_only() && crate::command_policy::hidden_in_view_only(spec.id) {
            continue;
        }
        let label = commands::current_label(spec, &app.session, app.active_ids().map(|(_, id)| id));
        let label = if let Some(rotated) = page_rotate_menu_label(spec.id) {
            rotated
        } else if app.session.view_only() && spec.id == "help.about" {
            crate::identity::localized("SwiftPDF について", "About SwiftPDF")
        } else {
            crate::i18n::menu_label(spec.id, &label)
        };
        // Open Recent is a submenu of the live recent list, not one action: disabled while the
        // list is empty, otherwise each entry opens its file (or focuses the tab showing it), and
        // Clear Recent Files at the foot empties the list (#430).
        if spec.id == "file.open_recent" {
            if app.recent.is_empty() {
                ui.add_enabled(false, egui::Button::new(label));
                continue;
            }
            let mut open: Option<String> = None;
            let mut clear = false;
            ui.menu_button(label, |ui| {
                for r in &app.recent {
                    if ui.button(&r.name).on_hover_text(&r.path).clicked() {
                        open = Some(r.path.clone());
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(tl!("Clear Recent Files")).clicked() {
                    clear = true;
                    ui.close();
                }
            });
            if let Some(p) = open {
                app.open_recent(&p);
                ui.close();
            }
            if clear {
                app.execute("file.clear_recent");
                ui.close();
            }
            continue;
        }
        let shortcut = spec.shortcut.or((spec.id == "edit.copy").then_some(COPY)).map(|s| shortcut_label(ui.ctx(), s)).unwrap_or_default();
        let enabled = app.command_enabled(spec);
        let mut resp = ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut));
        if matches!(spec.id, "page.rotate" | "page.rotate_ccw") {
            let tip = if enabled { page_rotate_menu_tip() } else { app.page_rotate_block_message() };
            resp = resp.on_hover_text(&tip).on_disabled_hover_text(&tip);
        }
        if resp.clicked() {
            app.execute(spec.id);
            ui.close();
        }
    }
    let _ = widgets::menu_item;
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::os::OperatingSystem;

    #[test]
    fn localized_shortcut_labels_follow_language_and_platform() {
        let previous = crate::i18n::current();
        let ctx = egui::Context::default();
        for os in [OperatingSystem::Windows, OperatingSystem::Nix] {
            ctx.set_os(os);
            crate::i18n::set_current(crate::i18n::Lang::from_code("de").unwrap());
            assert_eq!(shortcut_label(&ctx, Shortcut::cmd("K")), "Strg+K");
            assert_eq!(shortcut_label(&ctx, Shortcut::cmd_shift("S")), "Strg+Umschalt+S");
            assert_eq!(command_shortcut_label(&ctx, "view.read_mode"), "Strg+H");
            assert_eq!(command_modifier_label(&ctx), "Strg");
            assert_eq!(Shortcut::cmd_shift("S").label(false), "Ctrl+Shift+S", "engine/control labels keep their stable spelling");
            crate::i18n::set_current(crate::i18n::Lang::EN);
            assert_eq!(shortcut_label(&ctx, Shortcut::cmd_shift("S")), "Ctrl+Shift+S");
        }
        ctx.set_os(OperatingSystem::Mac);
        crate::i18n::set_current(crate::i18n::Lang::from_code("de").unwrap());
        assert_eq!(shortcut_label(&ctx, Shortcut::cmd_shift("S")), "⇧⌘S");
        assert_eq!(command_shortcut_label(&ctx, "view.read_mode"), "⌃⌘H");
        assert_eq!(command_modifier_label(&ctx), "⌘");
        crate::i18n::set_current(previous);
    }

    /// Next and previous tab step one place and wrap around at either end; from no tab they go
    /// to the first or the last (#1542).
    #[test]
    fn tab_steps_wrap_around_and_start_from_either_end() {
        assert_eq!(tab_after(Some(0), 3, true), Some(1));
        assert_eq!(tab_after(Some(2), 3, true), Some(0), "the last wraps to the first");
        assert_eq!(tab_after(Some(1), 3, false), Some(0));
        assert_eq!(tab_after(Some(0), 3, false), Some(2), "the first wraps to the last");
        assert_eq!(tab_after(None, 3, true), Some(0));
        assert_eq!(tab_after(None, 3, false), Some(2));
        assert_eq!(tab_after(Some(0), 1, true), Some(0), "a single tab stays");
        assert_eq!(tab_after(Some(7), 3, true), Some(0), "a stale index starts over");
        assert_eq!(tab_after(None, 0, true), None);
        assert_eq!(tab_after(Some(0), 0, false), None);
    }

    /// One page, no encryption. Document Properties reports no security.
    fn plain_pdf() -> Vec<u8> {
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",
            "<< /Type /Page /Parent 2 0 R >>",
        ];
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, obj) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{obj}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
        out
    }

    fn toast(app: &PdfCraftApp) -> String {
        app.toast.as_ref().map(|(text, _)| text.clone()).unwrap_or_default()
    }

    /// View-only is not a PDF restriction. An unsecured file must open the stamp tool.
    #[test]
    fn an_unsecured_pdf_opens_the_stamp_tool_without_a_security_refusal() {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        app.open_bytes("plain.pdf", None, plain_pdf()).unwrap();
        let id = app.views[0].id;
        let doc = app.session.get(id).unwrap();
        assert!(app.session.view_only());
        assert!(!doc.editable(), "view-only stays on");
        assert!(doc.security_summary().is_none(), "the file has no security handler");
        assert!(!doc.allows_annotation(), "general commenting stays tied to a fully editable document");
        assert!(app.command_allowed("comment.stamp"));
        assert!(!app.command_allowed("comment.note"));
        assert!(!app.command_allowed("file.save"));
        let spec = pdfcraft_engine::commands::command("comment.stamp").unwrap();
        assert!(app.command_enabled(spec), "the stamp command must not inherit the annotation ban from view-only");
        assert!(app.execute("comment.stamp"), "{}", toast(&app));
        assert!(!toast(&app).contains("security settings"), "{}", toast(&app));
        assert_eq!(app.left, crate::LeftPanel::Tool("stamp"));
        assert!(app.session.get(id).unwrap().info.annotations.is_empty(), "opening the tool places nothing");
        assert!(!app.execute("comment.note"));
        assert_eq!(toast(&app), crate::command_policy::REFUSED);
        assert!(!app.execute("page.delete"));
        assert_eq!(toast(&app), crate::command_policy::REFUSED);
    }

    fn locked_pdf() -> Vec<u8> {
        use pdfcraft_engine::{Changes, Printing, Protection};
        let mut author = pdfcraft_engine::Session::new().with_clock(|| 1_700_000_000);
        let id = author.open("plain.pdf", None, std::sync::Arc::new(plain_pdf()), None).unwrap();
        author
            .apply(
                id,
                pdfcraft_engine::Edit::Protect(Protection {
                    permissions_password: Some("owner".into()),
                    printing: Printing::High,
                    changes: Changes::None,
                    ..Protection::default()
                }),
            )
            .unwrap();
        author.save_bytes(id).unwrap().as_ref().clone()
    }

    fn signed_pdf() -> Vec<u8> {
        let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
        let identity = pdfcraft_engine::sign::pkcs12::open(&p12, "test").unwrap();
        let mut signer = pdfcraft_engine::Session::new().with_clock(|| 1_800_000_000);
        let id = signer.open("plain.pdf", None, std::sync::Arc::new(plain_pdf()), None).unwrap();
        signer.sign(id, &identity, pdfcraft_engine::SignOptions { rect: None, ..pdfcraft_engine::SignOptions::default() }).unwrap().as_ref().clone()
    }

    fn stamp_app(bytes: Vec<u8>) -> PdfCraftApp {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        app.open_bytes("sample.pdf", None, bytes).unwrap();
        app
    }

    #[test]
    fn a_comment_ban_and_a_signature_still_refuse_the_stamp_tool() {
        let mut banned = stamp_app(locked_pdf());
        let id = banned.views[0].id;
        assert!(!banned.session.allows_stamp(id));
        assert!(!banned.session.get(id).unwrap().editable());
        assert!(!banned.execute("comment.stamp"));
        assert!(toast(&banned).contains("注釈"), "{}", toast(&banned));
        assert!(banned.session.get(id).unwrap().info.annotations.is_empty());

        let mut signed = stamp_app(signed_pdf());
        let id = signed.views[0].id;
        assert!(signed.session.get(id).unwrap().is_signed());
        assert!(!signed.session.allows_stamp(id));
        assert!(!signed.execute("comment.stamp"));
        assert!(toast(&signed).contains("電子署名"), "{}", toast(&signed));
        assert!(signed.session.get(id).unwrap().info.annotations.is_empty());
        assert!(!signed.command_allowed("comment.note"));
    }
}
