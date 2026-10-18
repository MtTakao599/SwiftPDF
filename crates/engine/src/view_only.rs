//! A view-only session: an application-wide policy, separate from what a PDF's own security
//! allows ([`Document::permissions`]) and from documents that can't be edited at all
//! ([`Document::read_only_reason`]).
//!
//! With it on, [`Document::editable`] is `false` (so every `allows_*` check that needs an
//! editable document is too), and the session refuses with [`EditError::ViewOnly`]:
//! - every change to an open document: [`Session::apply`] (and everything built on it), undo,
//!   redo, revert, recording a save or a signature, lifting its restrictions with the owner
//!   password ([`Session::unlock`]), opening new unsaved work ([`Session::open_new`]);
//!   [`Session::mark_recovered`] leaves the document as opened;
//! - every file it would produce: Save, Save As, Optimize, signing and timestamps, new files
//!   (Combine, Extract, Split, Create, comment summaries and compare reports), the print-ready
//!   PDF (paper and Save as PDF) and form or comment data ([`Session::export_data`]).
//!   [`Capability::SaveAs`] is a separate grant. Without it, [`Session::save_copy_bytes`] is
//!   refused too. With it, that method returns the opened bytes only; it still does not allow
//!   [`Session::save_bytes`], a rewrite, or any other capability.
//!
//! Reading stays as it is: opening (with a password too) and reloading, rendering, text, search,
//! forms and signature validation as displayed, layers, hiding comments, reading colours, and
//! copying text as far as the document's security allows. Turning it off restores it all.

use crate::{Document, EditError, Session};

/// One action a view-only session may be granted on its own. Nothing is granted until
/// [`Session::grant`]. Viewing is the session itself, not one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    /// Write the opened bytes to a different new file.
    SaveAs,
    /// Change text or form values.
    EditText,
    /// Print.
    Print,
    /// Place a stamp.
    Stamp,
    /// Combine, split or extract pages.
    Organize,
}

impl Capability {
    fn bit(self) -> u8 {
        match self {
            Self::SaveAs => 1 << 0,
            Self::EditText => 1 << 1,
            Self::Print => 1 << 2,
            Self::Stamp => 1 << 3,
            Self::Organize => 1 << 4,
        }
    }
}

impl Session {
    /// Turn the view-only policy on or off, for this session and every document open in it.
    pub fn set_view_only(&mut self, on: bool) {
        self.view_only = on;
        for d in &mut self.docs {
            d.view_only = on;
        }
    }

    /// [`Session::set_view_only`] for a session being built.
    pub fn with_view_only(mut self, on: bool) -> Self {
        self.set_view_only(on);
        self
    }

    /// Whether the session refuses changes and PDF output ([`Session::set_view_only`]).
    pub fn view_only(&self) -> bool {
        self.view_only
    }

    /// [`EditError::ViewOnly`] when the session is view-only.
    pub(crate) fn check_not_view_only(&self) -> Result<(), EditError> {
        if self.view_only { Err(EditError::ViewOnly) } else { Ok(()) }
    }

    /// Allow `capability` in addition to viewing. It does not turn view-only off.
    pub fn grant(&mut self, capability: Capability) {
        self.capabilities |= capability.bit();
    }

    /// Withdraw a grant from [`Session::grant`].
    pub fn revoke(&mut self, capability: Capability) {
        self.capabilities &= !capability.bit();
    }

    /// Whether [`Session::grant`] was called for `capability` and not withdrawn.
    pub fn allows(&self, capability: Capability) -> bool {
        self.capabilities & capability.bit() != 0
    }
}

impl Document {
    /// Opened in, or since put under, a view-only session ([`Session::set_view_only`]).
    pub fn view_only(&self) -> bool {
        self.view_only
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::tests::fixture;
    use crate::{Capability, DocId, Edit, EditError, Session, SignOptions, SummarySort};

    fn view_only_session(pages: usize) -> (Session, DocId) {
        let mut s = Session::new().with_clock(|| 1_700_000_000).with_view_only(true);
        let id = s.open("fixture.pdf", None, Arc::new(fixture(pages)), None).expect("opens");
        (s, id)
    }

    fn page_text(s: &Session, id: DocId, page: usize) -> String {
        let doc = s.get(id).unwrap();
        let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
        let req = pdfcraft_render::RenderRequest { page, kind: pdfcraft_render::RequestKind::Text, scale: 1.0, ..Default::default() };
        r.render(req).text.map(|t| t.plain_text().trim().to_string()).unwrap_or_default()
    }

    fn digital_id() -> pdfcraft_sign::pkcs12::DigitalId {
        let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
        pdfcraft_sign::pkcs12::open(&p12, "test").unwrap()
    }

    #[test]
    fn sessions_are_editable_unless_made_view_only() {
        let mut s = Session::new();
        let id = s.open("fixture.pdf", None, Arc::new(fixture(2)), None).unwrap();
        assert!(!s.view_only());
        assert!(s.get(id).unwrap().editable());
        s.apply(id, Edit::DeletePages { pages: vec![1] }).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages.len(), 1);
    }

    #[test]
    fn view_only_documents_report_no_editing_rights() {
        let (s, id) = view_only_session(2);
        let doc = s.get(id).unwrap();
        assert!(doc.view_only());
        assert!(!doc.editable());
        assert!(!doc.allows_assembly());
        assert!(!doc.allows_modification());
        assert!(!doc.allows_form_filling());
        assert!(!doc.allows_annotation());
        assert!(!doc.allows_security_change());
        // Not the document's own restriction: it is readable and its security allows everything.
        assert_eq!(doc.read_only_reason, None);
        assert!(doc.allows_printing());
        assert!(doc.allows_copying());
    }

    #[test]
    fn view_only_refuses_edits_and_leaves_the_document_alone() {
        let (mut s, id) = view_only_session(3);
        let before = s.get(id).unwrap().bytes.clone();
        let generation = s.get(id).unwrap().edit_generation();
        assert_eq!(s.apply(id, Edit::DeletePages { pages: vec![1] }), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, Edit::SetInfo { key: "Title".into(), value: "Changed".into() }), Err(EditError::ViewOnly));
        let doc = s.get(id).unwrap();
        assert!(Arc::ptr_eq(&doc.bytes, &before));
        assert_eq!(doc.info.pages.len(), 3);
        assert_eq!(doc.edit_generation(), generation);
        assert!(!doc.dirty);
        assert_eq!(doc.can_undo(), None);
        assert!(s.autosave_snapshots().is_empty());
    }

    #[test]
    fn view_only_refuses_undo_redo_and_keeps_the_history() {
        let mut s = Session::new().with_clock(|| 1_700_000_000);
        let id = s.open("fixture.pdf", None, Arc::new(fixture(3)), None).unwrap();
        s.apply(id, Edit::DeletePages { pages: vec![1] }).unwrap();
        s.set_view_only(true);
        assert!(!s.get(id).unwrap().editable());
        assert_eq!(s.undo(id), Err(EditError::ViewOnly));
        assert_eq!(s.redo(id), Err(EditError::ViewOnly));
        assert_eq!(s.revert(id), Err(EditError::ViewOnly));
        assert_eq!(s.get(id).unwrap().info.pages.len(), 2);
        // Off again, editing (and its history) is back.
        s.set_view_only(false);
        assert!(s.get(id).unwrap().editable());
        s.undo(id).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages.len(), 3);
    }

    #[test]
    fn view_only_writes_no_pdf() {
        let (s, id) = view_only_session(2);
        assert_eq!(s.save_bytes(id), Err(EditError::ViewOnly));
        assert_eq!(s.save_full_bytes(id), Err(EditError::ViewOnly));
        assert_eq!(s.reduced_bytes(id).map(|_| ()), Err(EditError::ViewOnly));
        assert_eq!(s.extract(id, &[0]), Err(EditError::ViewOnly));
        assert_eq!(s.split_by_size(id, 1_000_000).map(|_| ()), Err(EditError::ViewOnly));
        assert_eq!(s.combine(&[("a.pdf".into(), Arc::new(fixture(1)))]), Err(EditError::ViewOnly));
        assert_eq!(s.create_blank(200.0, 300.0, 1), Err(EditError::ViewOnly));
        assert_eq!(s.create_from_text("Notes", "Hello"), Err(EditError::ViewOnly));
        assert_eq!(s.summarize_comments(id, SummarySort::Page), Err(EditError::ViewOnly));
        assert_eq!(s.compare_report(id, id), Err(EditError::ViewOnly));
        assert_eq!(s.sign(id, &digital_id(), SignOptions { rect: None, ..SignOptions::default() }), Err(EditError::ViewOnly));
    }

    #[test]
    fn view_only_prints_and_exports_nothing() {
        let (s, id) = view_only_session(2);
        let settings = crate::print::Settings { pages: vec![0, 1], ..Default::default() };
        assert_eq!(s.print_pdf(id, &settings), Err(EditError::ViewOnly));
        for format in [crate::DataFormat::Xfdf, crate::DataFormat::Fdf, crate::DataFormat::Csv] {
            assert_eq!(s.export_data(id, format, true, true), Err(EditError::ViewOnly));
        }
        // Off, the same calls produce files.
        let mut s = s;
        s.set_view_only(false);
        assert!(s.print_pdf(id, &settings).is_ok());
        assert!(s.export_data(id, crate::DataFormat::Xfdf, true, true).is_ok());
    }

    #[test]
    fn view_only_keeps_opening_with_a_password_but_refuses_unlocking() {
        use crate::{Changes, Printing, Protection};
        let mut author = Session::new().with_clock(|| 1_700_000_000);
        let id = author.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        let protection = Protection {
            open_password: Some("reader".into()),
            permissions_password: Some("owner".into()),
            printing: Printing::None,
            changes: Changes::None,
            copy: false,
            ..Protection::default()
        };
        author.apply(id, Edit::Protect(protection)).unwrap();
        let locked = author.save_bytes(id).unwrap();

        let mut s = Session::new().with_view_only(true);
        let id = s.open("locked.pdf", None, locked, Some("reader")).expect("opens with the open password");
        assert_eq!(page_text(&s, id, 0), "Page 1");
        assert!(!s.get(id).unwrap().allows_copying());
        assert_eq!(s.unlock(id, "owner"), Err(EditError::ViewOnly));
        assert!(!s.get(id).unwrap().allows_copying(), "the restrictions stay");
    }

    #[test]
    fn view_only_refuses_recording_saves_and_new_unsaved_work() {
        let (mut s, id) = view_only_session(1);
        let other = Arc::new(fixture(2));
        assert_eq!(s.mark_saved(id, other.clone(), Some("elsewhere.pdf".into())), Err(EditError::ViewOnly));
        assert_eq!(s.mark_signed(id, other.clone(), None), Err(EditError::ViewOnly));
        assert_eq!(s.get(id).unwrap().info.pages.len(), 1);
        assert!(s.open_new("new.pdf", other).is_err());
        assert_eq!(s.docs().len(), 1);
        s.mark_recovered(id, Some("recovered.pdf".into()));
        let doc = s.get(id).unwrap();
        assert!(!doc.dirty);
        assert_eq!(doc.path, None);
    }

    #[test]
    fn view_only_keeps_reading_searching_and_viewing() {
        let (mut s, id) = view_only_session(2);
        assert_eq!(page_text(&s, id, 1), "Page 2");
        let doc = s.get(id).unwrap();
        assert!(doc.words().iter().any(|w| w.text == "Page"), "search words");
        assert!(!doc.text_lines(0).is_empty(), "text selection");
        assert!(doc.check_copying().is_ok());
        assert!(s.set_hide_comments(id, true));
        // A file rewritten on disk is shown again, still view-only.
        s.reload(id, Arc::new(fixture(3))).unwrap();
        let doc = s.get(id).unwrap();
        assert_eq!(doc.info.pages.len(), 3);
        assert!(doc.view_only());
        assert!(!doc.dirty);
    }

    #[test]
    fn view_only_still_validates_signatures() {
        let mut signer = Session::new().with_clock(|| 1_800_000_000);
        let id = signer.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        let signed = signer.sign(id, &digital_id(), SignOptions { rect: None, ..SignOptions::default() }).unwrap();
        let mut s = Session::new().with_view_only(true);
        let id = s.open("signed.pdf", None, signed, None).unwrap();
        let doc = s.get(id).unwrap();
        assert!(doc.is_signed());
        assert!(doc.signatures.iter().any(|sig| sig.signed));
    }

    #[test]
    fn documents_opened_before_the_policy_follow_it() {
        let mut s = Session::new();
        let id = s.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        s.set_view_only(true);
        assert!(s.get(id).unwrap().view_only());
        assert_eq!(s.apply(id, Edit::DeletePages { pages: vec![0] }), Err(EditError::ViewOnly));
        let later = s.open("later.pdf", None, Arc::new(fixture(1)), None).unwrap();
        assert!(s.get(later).unwrap().view_only());
    }

    #[test]
    fn view_only_save_as_is_refused_until_granted_and_grants_nothing_else() {
        let (s, id) = view_only_session(2);
        assert!(!s.allows(Capability::SaveAs));
        assert_eq!(s.save_copy_bytes(id), Err(EditError::ViewOnly));
        let mut s = s;
        s.grant(Capability::SaveAs);
        assert!(s.view_only());
        assert!(s.allows(Capability::SaveAs));
        for other in [Capability::EditText, Capability::Print, Capability::Stamp, Capability::Organize] {
            assert!(!s.allows(other));
        }
        let opened = s.get(id).unwrap().bytes.clone();
        let copied = s.save_copy_bytes(id).unwrap();
        assert!(Arc::ptr_eq(&copied, &opened) || copied.as_slice() == opened.as_slice());
        s.check_save(id, &copied).unwrap();
        assert_eq!(s.save_bytes(id), Err(EditError::ViewOnly));
        assert_eq!(s.save_full_bytes(id), Err(EditError::ViewOnly));
        assert_eq!(s.mark_saved(id, copied, Some("copy.pdf".into())), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, Edit::DeletePages { pages: vec![1] }), Err(EditError::ViewOnly));
        let settings = crate::print::Settings { pages: vec![0], ..Default::default() };
        assert_eq!(s.print_pdf(id, &settings), Err(EditError::ViewOnly));
        assert_eq!(s.extract(id, &[0]), Err(EditError::ViewOnly));
        assert_eq!(s.get(id).unwrap().info.pages.len(), 2);
        assert!(!s.get(id).unwrap().dirty);
    }

    #[test]
    fn view_only_changed_document_cannot_be_copied() {
        let mut s = Session::new().with_clock(|| 1_700_000_000);
        let id = s.open("fixture.pdf", None, Arc::new(fixture(3)), None).unwrap();
        s.apply(id, Edit::DeletePages { pages: vec![1] }).unwrap();
        s.set_view_only(true);
        s.grant(Capability::SaveAs);
        assert!(matches!(s.save_copy_bytes(id), Err(EditError::Invalid(_))));
        assert_eq!(s.save_bytes(id), Err(EditError::ViewOnly));
        assert_eq!(s.get(id).unwrap().info.pages.len(), 2);
    }

    #[test]
    fn view_only_restricted_pdf_is_copied_without_lifting_its_restrictions() {
        use crate::{Changes, Printing, Protection};
        let mut author = Session::new().with_clock(|| 1_700_000_000);
        let id = author.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        author
            .apply(
                id,
                Edit::Protect(Protection {
                    open_password: Some("reader".into()),
                    permissions_password: Some("owner".into()),
                    printing: Printing::None,
                    changes: Changes::None,
                    copy: false,
                    ..Protection::default()
                }),
            )
            .unwrap();
        let locked = author.save_bytes(id).unwrap();

        let mut s = Session::new().with_view_only(true);
        s.grant(Capability::SaveAs);
        let id = s.open("locked.pdf", None, locked.clone(), Some("reader")).unwrap();
        let copied = s.save_copy_bytes(id).unwrap();
        assert_eq!(copied.as_slice(), locked.as_slice());
        s.check_save(id, &copied).unwrap();
        assert_eq!(s.unlock(id, "owner"), Err(EditError::ViewOnly));

        let mut again = Session::new().with_view_only(true);
        let id2 = again.open("copy.pdf", None, copied, Some("reader")).unwrap();
        let doc = again.get(id2).unwrap();
        assert!(!doc.allows_copying());
        assert!(!doc.allows_printing());
        assert!(!doc.allows_modification());
        assert_eq!(again.unlock(id2, "owner"), Err(EditError::ViewOnly));
    }

    #[test]
    fn view_only_signed_pdf_is_copied_as_the_bytes_that_were_opened() {
        let mut signer = Session::new().with_clock(|| 1_800_000_000);
        let id = signer.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        let signed = signer.sign(id, &digital_id(), SignOptions { rect: None, ..SignOptions::default() }).unwrap();
        let mut s = Session::new().with_view_only(true);
        s.grant(Capability::SaveAs);
        let id = s.open("signed.pdf", None, signed.clone(), None).unwrap();
        let copied = s.save_copy_bytes(id).unwrap();
        assert_eq!(copied.as_slice(), signed.as_slice());
        assert!(s.get(id).unwrap().is_signed());
        assert_eq!(s.save_full_bytes(id), Err(EditError::ViewOnly));
    }
}
