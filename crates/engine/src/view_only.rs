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
//!   [`Capability::EditText`] is also separate. With it, only added text may be applied, undone
//!   and redone ([`Session::allows_added_text`]). Forms, pages, comments and in-place save stay
//!   refused. Edited bytes for a new file come from [`Session::save_edited_copy_bytes`].
//!   [`Capability::Print`] is also separate. With it, [`Session::print_pdf`] returns a print-ready
//!   PDF and does not write, mark the document saved, or change undo. A document that forbids
//!   printing is still refused. Without the grant, the print-ready PDF is refused.
//!   [`Capability::Stamp`] is also separate. With it, only a custom-image stamp placed in this
//!   session, and a later move, resize, opacity change or deletion of that same stamp, may be
//!   applied, undone and redone. It does not turn view-only off, and it does not allow rubber
//!   stamps, other comments, forms or pages. A document that already has a signature, or whose
//!   signature state cannot be read, is refused. A document that forbids comments is still
//!   refused. Stamps that were already in the file are not editable.
//!   [`Capability::PageRotate`] is also separate. With it, only [`crate::Edit::RotatePages`] may
//!   be applied, undone and redone, and only when the PDF allows assembly and has no signature
//!   field that holds a signature. Delete, move, extract and every other page edit stay refused.
//!   The grant does not turn view-only off and does not by itself enable a page-rotate command.
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
    /// Turn pages by writing `/Rotate`. Not delete, reorder, or extract.
    PageRotate,
}

impl Capability {
    fn bit(self) -> u8 {
        match self {
            Self::SaveAs => 1 << 0,
            Self::EditText => 1 << 1,
            Self::Print => 1 << 2,
            Self::Stamp => 1 << 3,
            Self::Organize => 1 << 4,
            Self::PageRotate => 1 << 5,
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
        for other in [Capability::EditText, Capability::Print, Capability::Stamp, Capability::Organize, Capability::PageRotate] {
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

    /// A one-page PDF whose only content is an image, standing in for a scan.
    fn scan_page() -> Vec<u8> {
        let mut image =
            b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\n".to_vec();
        image.push(0xff);
        image.extend_from_slice(b"\nendstream");
        let body = "q 180 0 0 180 40 80 cm /Im0 Do Q";
        let content = format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()).into_bytes();
        let objs = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
            image,
            content,
        ];
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            out.extend_from_slice(object);
            out.extend_from_slice(b"\nendobj\n");
        }
        let start = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n", objs.len() + 1).as_bytes());
        out
    }

    #[test]
    fn view_only_added_text_is_saved_as_a_new_pdf_and_other_edits_stay_refused() {
        let original = scan_page();
        let label = if pdfcraft_fonts::document_japanese_font().is_some() { "山田 太郎 1-2-3" } else { "Note 1-2-3" };
        let mut s = Session::new().with_clock(|| 1_700_000_000).with_view_only(true);
        s.grant(Capability::SaveAs);
        s.grant(Capability::EditText);
        let id = s.open("scan.pdf", Some("scan.pdf".into()), Arc::new(original.clone()), None).unwrap();
        assert!(s.allows_added_text(id));
        assert!(s.view_only());
        assert!(!s.get(id).unwrap().allows_modification());
        assert!(!s.get(id).unwrap().allows_form_filling());
        assert!(!s.get(id).unwrap().allows_annotation());
        assert_eq!(s.apply(id, Edit::DeletePages { pages: vec![0] }), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, Edit::AddImage { page: 0, rect: None, name: "a.png".into(), bytes: Arc::new(vec![1]) }), Err(EditError::ViewOnly));
        let settings = crate::print::Settings { pages: vec![0], ..Default::default() };
        assert_eq!(s.print_pdf(id, &settings), Err(EditError::ViewOnly));
        s.apply(
            id,
            Edit::AddText {
                page: 0,
                text: crate::AddedText {
                    rect: [40.0, 280.0, 260.0, 340.0],
                    text: label.into(),
                    size: 16.0,
                    color: [0.0, 0.0, 0.6],
                    ..crate::AddedText::default()
                },
            },
        )
        .unwrap();
        assert!(s.get(id).unwrap().dirty);
        assert_eq!(s.undo(id).unwrap().as_str(), "Add text");
        assert!(!s.get(id).unwrap().dirty);
        assert_eq!(s.redo(id).unwrap().as_str(), "Add text");
        let moved = {
            let item = s.get(id).unwrap().added()[0].content.clone();
            let crate::AddedContent::Text(text) = item else { panic!("added text") };
            crate::AddedText { rect: [50.0, text.rect[1], 270.0, text.rect[3]], ..text }
        };
        s.apply(id, Edit::UpdateContent { page: 0, index: 0, content: crate::AddedContent::Text(moved) }).unwrap();
        assert_eq!(s.save_copy_bytes(id), Err(EditError::Invalid("this copy can only save the file as it was opened".into())));
        assert_eq!(s.save_bytes(id), Err(EditError::ViewOnly));
        let saved = s.save_edited_copy_bytes(id).unwrap();
        assert_ne!(saved.as_slice(), original.as_slice());
        assert!(s.get(id).unwrap().dirty, "a failed or finished copy save must keep the added text");
        s.check_save(id, &saved).unwrap();
        let mut again = Session::new().with_view_only(true);
        let copy = again.open("copy.pdf", None, saved, None).unwrap();
        let shown = page_text(&again, copy, 0);
        assert!(shown.contains(if label.starts_with('山') { "山田" } else { "Note" }), "{shown}");
        assert!(shown.contains("1-2-3"), "{shown}");
        assert_eq!(original, scan_page(), "the bytes this test opened from are unchanged");
    }

    #[test]
    fn view_only_added_text_respects_a_pdf_that_forbids_changes() {
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
        s.grant(Capability::EditText);
        let id = s.open("locked.pdf", None, locked, Some("reader")).unwrap();
        assert!(!s.allows_added_text(id));
        assert_eq!(
            s.apply(
                id,
                Edit::AddText {
                    page: 0,
                    text: crate::AddedText { rect: [40.0, 40.0, 200.0, 80.0], text: "No".into(), ..crate::AddedText::default() }
                }
            ),
            Err(EditError::NotPermitted("changes to the document"))
        );
        assert!(!s.get(id).unwrap().dirty);
        assert_eq!(s.unlock(id, "owner"), Err(EditError::ViewOnly));
    }

    #[test]
    fn a_saved_copy_matches_until_the_text_changes_again() {
        let mut s = Session::new().with_clock(|| 1_700_000_000).with_view_only(true);
        s.grant(Capability::SaveAs);
        s.grant(Capability::EditText);
        let original = Arc::new(fixture(1));
        let id = s.open("opened.pdf", Some("opened.pdf".into()), original.clone(), None).unwrap();
        s.apply(
            id,
            Edit::AddText {
                page: 0,
                text: crate::AddedText { rect: [40.0, 80.0, 200.0, 120.0], text: "Note".into(), ..crate::AddedText::default() },
            },
        )
        .unwrap();
        assert!(s.get(id).unwrap().dirty);
        let saved = s.save_edited_copy_bytes(id).unwrap();
        assert_ne!(saved.as_slice(), original.as_slice());
        assert!(s.get(id).unwrap().dirty, "writing bytes does not itself record the copy");
        s.note_saved_copy(id).unwrap();
        assert!(!s.get(id).unwrap().dirty);
        assert_eq!(s.get(id).unwrap().path.as_deref(), Some("opened.pdf"));
        assert!(s.differs_from_opened(id));
        assert_eq!(s.mark_saved(id, saved.clone(), Some("copy.pdf".into())), Err(EditError::ViewOnly));
        assert_eq!(s.get(id).unwrap().path.as_deref(), Some("opened.pdf"));
        let again = s.save_edited_copy_bytes(id).unwrap();
        assert_ne!(again.as_slice(), original.as_slice());
        s.apply(
            id,
            Edit::AddText {
                page: 0,
                text: crate::AddedText { rect: [40.0, 140.0, 200.0, 180.0], text: "More".into(), ..crate::AddedText::default() },
            },
        )
        .unwrap();
        assert!(s.get(id).unwrap().dirty);
        assert_eq!(s.undo(id).unwrap().as_str(), "Add text");
        assert!(!s.get(id).unwrap().dirty, "undo back to the saved copy is clean");
        assert_eq!(s.undo(id).unwrap().as_str(), "Add text");
        assert!(!s.get(id).unwrap().dirty, "undo back to the opened file is clean");
        assert_eq!(s.redo(id).unwrap().as_str(), "Add text");
        assert!(!s.get(id).unwrap().dirty, "redo back to the saved copy is clean");
        assert_eq!(s.redo(id).unwrap().as_str(), "Add text");
        assert!(s.get(id).unwrap().dirty, "redo past the saved copy is unsaved");
    }

    fn sha256(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn placed(doc: &crate::Document) -> Vec<crate::AddedText> {
        doc.added()
            .iter()
            .filter(|item| item.page == 0)
            .map(|item| match &item.content {
                crate::AddedContent::Text(text) => text.clone(),
                crate::AddedContent::Image(_) => panic!("added text"),
            })
            .collect()
    }

    fn render_pixels(bytes: &std::sync::Arc<Vec<u8>>, config: &pdfcraft_render::RenderConfig) -> pdfcraft_render::RenderedPage {
        let mut renderer = pdfcraft_render::PageRenderer::new(bytes.clone(), config.clone());
        let page =
            renderer.render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Pixels, scale: 2.0, ..Default::default() });
        assert!(page.error.is_none(), "{:?}", page.error);
        page
    }

    /// Pixels inside a user-space box. PDF y grows up; the raster's y grows down.
    fn crop(page: &pdfcraft_render::RenderedPage, rect: [f64; 4], page_height: f64, scale: f64) -> Vec<u8> {
        let x0 = (rect[0] * scale).floor().clamp(0.0, page.width as f64) as u32;
        let x1 = (rect[2] * scale).ceil().clamp(0.0, page.width as f64) as u32;
        let y0 = ((page_height - rect[3]) * scale).floor().clamp(0.0, page.height as f64) as u32;
        let y1 = ((page_height - rect[1]) * scale).ceil().clamp(0.0, page.height as f64) as u32;
        let mut out = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y as usize * page.width as usize + x as usize) * 4;
                let Some(pixel) = page.rgba.get(i..i + 4) else { continue };
                out.extend_from_slice(pixel);
            }
        }
        out
    }

    fn ink_count(pixels: &[u8]) -> usize {
        pixels
            .chunks(4)
            .filter(|pixel| {
                pixel.first().is_some_and(|red| *red < 250)
                    || pixel.get(1).is_some_and(|green| *green < 250)
                    || pixel.get(2).is_some_and(|blue| *blue < 250)
            })
            .count()
    }

    #[test]
    fn three_added_japanese_strings_stay_drawn_after_edits_undo_and_save() {
        if pdfcraft_fonts::document_japanese_font().is_none() {
            eprintln!("skipping three_added_japanese_strings_stay_drawn_after_edits_undo_and_save: built without craft-fonts");
            return;
        }
        let original = fixture(1);
        let dir = std::env::temp_dir().join(format!("pdfcraft-jp-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let original_path = dir.join("original.pdf");
        std::fs::write(&original_path, &original).unwrap();
        let original_hash = sha256(&std::fs::read(&original_path).unwrap());

        let mut session = Session::new().with_clock(|| 1_700_000_000).with_view_only(true);
        session.grant(Capability::SaveAs);
        session.grant(Capability::EditText);
        let id = session.open("opened.pdf", Some(original_path.to_string_lossy().into_owned()), Arc::new(original.clone()), None).unwrap();
        assert!(session.view_only());
        let add = |session: &mut Session, text: &str, rect: [f64; 4], size: f64, color: [f64; 3]| {
            session
                .apply(id, Edit::AddText { page: 0, text: crate::AddedText { rect, text: text.into(), size, color, ..crate::AddedText::default() } })
                .unwrap();
        };
        add(&mut session, "東京", [10.0, 220.0, 90.0, 280.0], 18.0, [1.0, 0.0, 0.0]);
        let first = placed(session.get(id).unwrap()).remove(0);
        let config = session.get(id).unwrap().config.clone();
        let alone = render_pixels(&session.get(id).unwrap().bytes, &config);
        let first_ink = crop(&alone, first.rect, 300.0, 2.0);
        assert!(ink_count(&first_ink) > 8, "the first string did not paint");

        add(&mut session, "大阪", [110.0, 220.0, 190.0, 280.0], 16.0, [0.0, 0.0, 1.0]);
        add(&mut session, "名古屋 1-2-3", [10.0, 30.0, 150.0, 100.0], 12.0, [0.0, 0.5, 0.0]);
        let three = placed(session.get(id).unwrap());
        assert_eq!(three.len(), 3);
        assert_ne!(three[0].color, three[1].color);
        assert_ne!(three[1].size, three[2].size);
        assert_eq!(three[0].rect, first.rect);
        let drawn = render_pixels(&session.get(id).unwrap().bytes, &config);
        assert_eq!(crop(&drawn, three[0].rect, 300.0, 2.0), first_ink, "adding more strings repainted the first one");
        for item in &three {
            assert!(ink_count(&crop(&drawn, item.rect, 300.0, 2.0)) > 8, "{} did not paint", item.text);
        }
        let extracted = page_text(&session, id, 0);
        for item in &three {
            assert!(extracted.contains(&item.text), "{extracted}");
        }

        let mut moved = three[1].clone();
        moved.rect = [moved.rect[0] + 8.0, moved.rect[1] - 20.0, moved.rect[2] + 8.0, moved.rect[3] - 20.0];
        session.apply(id, Edit::UpdateContent { page: 0, index: 1, content: crate::AddedContent::Text(moved) }).unwrap();
        let after_move = placed(session.get(id).unwrap());
        assert_eq!(after_move[0], three[0]);
        assert_eq!(after_move[2], three[2]);
        assert_ne!(after_move[1].rect, three[1].rect);
        let moved_page = render_pixels(&session.get(id).unwrap().bytes, &config);
        assert_eq!(crop(&moved_page, after_move[0].rect, 300.0, 2.0), first_ink, "moving the second string repainted the first");
        assert!(ink_count(&crop(&moved_page, after_move[2].rect, 300.0, 2.0)) > 8, "the third string disappeared when the second moved");

        let mut edited = after_move[0].clone();
        edited.text = "京都".into();
        session.apply(id, Edit::UpdateContent { page: 0, index: 0, content: crate::AddedContent::Text(edited) }).unwrap();
        let after_edit = placed(session.get(id).unwrap());
        assert_eq!(after_edit[0].text, "京都");
        assert_eq!(after_edit[1], after_move[1]);
        assert_eq!(after_edit[2], after_move[2]);
        let edited_page = render_pixels(&session.get(id).unwrap().bytes, &config);
        assert_ne!(crop(&edited_page, after_edit[0].rect, 300.0, 2.0), first_ink, "retyping the first string left its old drawing");
        assert!(ink_count(&crop(&edited_page, after_edit[1].rect, 300.0, 2.0)) > 8, "the second string disappeared when the first was retyped");
        assert!(ink_count(&crop(&edited_page, after_edit[2].rect, 300.0, 2.0)) > 8, "the third string disappeared when the first was retyped");

        assert_eq!(session.undo(id).unwrap().as_str(), "Edit content");
        assert_eq!(placed(session.get(id).unwrap()), after_move);
        assert_eq!(session.undo(id).unwrap().as_str(), "Edit content");
        assert_eq!(placed(session.get(id).unwrap()), three);
        let restored = render_pixels(&session.get(id).unwrap().bytes, &config);
        assert_eq!(crop(&restored, three[0].rect, 300.0, 2.0), first_ink, "undo did not restore the first string's drawing");
        assert_eq!(session.redo(id).unwrap().as_str(), "Edit content");
        assert_eq!(session.redo(id).unwrap().as_str(), "Edit content");
        assert_eq!(placed(session.get(id).unwrap()), after_edit);
        assert_eq!(session.undo(id).unwrap().as_str(), "Edit content");
        assert_eq!(session.undo(id).unwrap().as_str(), "Edit content");
        assert_eq!(placed(session.get(id).unwrap()), three);

        session.apply(id, Edit::DeleteContent { page: 0, index: 1 }).unwrap();
        let after_delete = placed(session.get(id).unwrap());
        assert_eq!(after_delete, vec![three[0].clone(), three[2].clone()]);
        let deleted_page = render_pixels(&session.get(id).unwrap().bytes, &config);
        assert_eq!(crop(&deleted_page, three[0].rect, 300.0, 2.0), first_ink, "deleting the second string repainted the first");
        assert!(ink_count(&crop(&deleted_page, three[2].rect, 300.0, 2.0)) > 8, "deleting the second string removed the third");
        assert_eq!(session.undo(id).unwrap().as_str(), "Delete content");
        assert_eq!(placed(session.get(id).unwrap()), three);
        let undeleted = render_pixels(&session.get(id).unwrap().bytes, &config);
        assert_eq!(
            crop(&undeleted, three[1].rect, 300.0, 2.0),
            crop(&drawn, three[1].rect, 300.0, 2.0),
            "undo did not restore the deleted string's drawing"
        );

        let saved = session.save_edited_copy_bytes(id).unwrap();
        let copy_path = dir.join("copy.pdf");
        std::fs::write(&copy_path, saved.as_ref()).unwrap();
        assert_eq!(sha256(&std::fs::read(&original_path).unwrap()), original_hash);
        assert_eq!(std::fs::read(&original_path).unwrap(), original);
        assert!(session.view_only());
        assert_eq!(session.save_bytes(id), Err(EditError::ViewOnly));
        assert_eq!(session.get(id).unwrap().path.as_deref(), Some(original_path.to_string_lossy().as_ref()));

        let mut again = Session::new().with_view_only(true);
        let copy = again.open("copy.pdf", None, std::sync::Arc::clone(&saved), None).unwrap();
        let reopened = placed(again.get(copy).unwrap());
        assert_eq!(reopened.len(), 3);
        assert_eq!(reopened[0].text, "東京");
        assert_eq!(reopened[1].text, "大阪");
        assert_eq!(reopened[2].text, "名古屋 1-2-3");
        let shown = page_text(&again, copy, 0);
        assert!(shown.contains("東京") && shown.contains("大阪") && shown.contains("名古屋") && shown.contains("1-2-3"), "{shown}");
        let copy_page = render_pixels(&again.get(copy).unwrap().bytes, &again.get(copy).unwrap().config);
        for item in &reopened {
            assert!(ink_count(&crop(&copy_page, item.rect, 300.0, 2.0)) > 8, "{} did not paint in the saved PDF", item.text);
        }
        assert_eq!(crop(&copy_page, reopened[0].rect, 300.0, 2.0), first_ink, "the saved PDF redrew the first string");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn view_only_print_is_refused_until_granted_and_then_leaves_the_document() {
        let (mut s, id) = view_only_session(1);
        let settings = crate::print::Settings { pages: vec![0], ..Default::default() };
        assert_eq!(s.print_pdf(id, &settings), Err(EditError::ViewOnly));
        s.grant(Capability::Print);
        assert!(s.view_only());
        assert!(!s.allows(Capability::SaveAs));
        assert!(!s.allows(Capability::EditText));
        assert!(!s.allows(Capability::Stamp));
        assert!(!s.allows(Capability::Organize));
        assert!(!s.allows(Capability::PageRotate));
        let before = s.get(id).unwrap().bytes.clone();
        let generation = s.get(id).unwrap().edit_generation();
        let printed = s.print_pdf(id, &settings).unwrap();
        assert!(printed.starts_with(b"%PDF"));
        assert!(text_of(printed).contains("Page 1"));
        let doc = s.get(id).unwrap();
        assert!(Arc::ptr_eq(&doc.bytes, &before));
        assert_eq!(doc.edit_generation(), generation);
        assert!(doc.can_undo().is_none() && doc.can_redo().is_none());
        assert!(!doc.dirty);
        assert_eq!(s.save_bytes(id), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, Edit::DeletePages { pages: vec![0] }), Err(EditError::ViewOnly));
    }

    #[test]
    fn view_only_print_draws_uncommitted_text_without_an_undo_step() {
        let (mut s, id) = view_only_session(1);
        s.grant(Capability::Print);
        s.grant(Capability::EditText);
        let settings = crate::print::Settings { pages: vec![0], ..Default::default() };
        let before = s.get(id).unwrap().bytes.clone();
        let generation = s.get(id).unwrap().edit_generation();
        let sample = crate::AddedText { text: "日本語".into(), rect: [40.0, 200.0, 260.0, 260.0], size: 16.0, ..crate::AddedText::default() };
        let pending = crate::PendingPrintText { page: 0, replace: None, text: sample.clone() };
        if crate::first_undrawable(&sample).is_none() {
            let printed = s.print_pdf_with_pending_text(id, &settings, Some(&pending)).unwrap();
            assert!(printed.windows(5).any(|window| window == b"Type3"), "the Japanese face stays a Type3 font");
            let shown = text_of(printed);
            assert!(shown.contains("日本語"), "{shown}");
        } else {
            assert!(s.print_pdf_with_pending_text(id, &settings, Some(&pending)).is_err(), "undrawable text is not silently dropped");
        }
        let latin = crate::PendingPrintText {
            page: 0,
            replace: None,
            text: crate::AddedText { text: "Printed note".into(), rect: [40.0, 80.0, 260.0, 140.0], size: 16.0, ..crate::AddedText::default() },
        };
        let printed = s.print_pdf_with_pending_text(id, &settings, Some(&latin)).unwrap();
        assert!(text_of(printed).contains("Printed note"));
        let doc = s.get(id).unwrap();
        assert!(Arc::ptr_eq(&doc.bytes, &before));
        assert_eq!(doc.edit_generation(), generation);
        assert!(doc.can_undo().is_none() && doc.can_redo().is_none() && !doc.dirty);
        assert_eq!(s.save_bytes(id), Err(EditError::ViewOnly));
    }

    #[test]
    fn view_only_print_keeps_a_signed_file_and_honours_a_print_ban() {
        use crate::{Changes, Printing, Protection};
        let mut signer = Session::new().with_clock(|| 1_800_000_000);
        let id = signer.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        let signed = signer.sign(id, &digital_id(), SignOptions { rect: None, ..SignOptions::default() }).unwrap();
        let mut s = Session::new().with_view_only(true);
        s.grant(Capability::Print);
        let id = s.open("signed.pdf", None, signed.clone(), None).unwrap();
        let settings = crate::print::Settings { pages: vec![0], ..Default::default() };
        let printed = s.print_pdf(id, &settings).unwrap();
        assert!(printed.starts_with(b"%PDF"));
        assert!(std::sync::Arc::ptr_eq(&s.get(id).unwrap().bytes, &signed));
        assert!(s.get(id).unwrap().is_signed());
        assert!(s.get(id).unwrap().can_undo().is_none());

        let mut author = Session::new().with_clock(|| 1_700_000_000);
        let id = author.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        author
            .apply(
                id,
                Edit::Protect(Protection {
                    permissions_password: Some("owner".into()),
                    printing: Printing::None,
                    changes: Changes::None,
                    ..Protection::default()
                }),
            )
            .unwrap();
        let locked = author.save_bytes(id).unwrap();
        let mut reader = Session::new().with_view_only(true);
        reader.grant(Capability::Print);
        let id = reader.open("locked.pdf", None, locked.clone(), None).unwrap();
        assert_eq!(reader.print_pdf(id, &settings), Err(EditError::NotPermitted("printing")));
        assert!(std::sync::Arc::ptr_eq(&reader.get(id).unwrap().bytes, &locked));
        assert_eq!(reader.unlock(id, "owner"), Err(EditError::ViewOnly));
    }

    fn stamp_edit(page: usize, rect: [f64; 4], opacity: f64) -> Edit {
        Edit::AddCustomStamp {
            page,
            rect,
            name: "Probe".into(),
            file: crate::MarkFile { name: "probe.png".into(), bytes: std::sync::Arc::new(pdfcraft_xfa::fixtures::tiny_png()), page: 0 },
            author: "Test".into(),
            opacity,
        }
    }

    #[test]
    fn view_only_stamps_are_only_the_ones_this_session_places() {
        use crate::{Changes, Printing, Protection, print::Content};
        let (mut s, id) = view_only_session(2);
        let original = s.opened_bytes(id).unwrap();
        let original_hash = sha256(&original);
        assert_eq!(s.apply(id, stamp_edit(0, [40.0, 40.0, 100.0, 100.0], 0.5)), Err(EditError::ViewOnly));
        s.grant(Capability::Stamp);
        assert!(s.view_only());
        assert!(!s.get(id).unwrap().editable());
        assert!(!s.get(id).unwrap().allows_annotation());
        assert!(s.allows_stamp(id), "no security handler still allows a session stamp");
        assert!(!s.allows(Capability::Organize));
        assert!(!s.allows(Capability::PageRotate));
        s.apply(id, stamp_edit(0, [40.0, 40.0, 100.0, 100.0], 0.5)).unwrap();
        s.apply(id, stamp_edit(1, [20.0, 20.0, 80.0, 80.0], 1.0)).unwrap();
        let doc = s.get(id).unwrap();
        assert_eq!(doc.info.annotations.len(), 2);
        assert!(doc.info.annotations.iter().all(|a| a.subtype == "Stamp"));
        assert!(doc.bytes.windows(5).any(|window| window == b"SMask"));
        assert!(doc.bytes.windows(3).any(|window| window == b"/CA"));
        assert!(s.owns_stamp(id, 0, 0));
        assert!(s.owns_stamp(id, 1, 0));
        assert!(!s.owns_stamp(id, 0, 1));
        assert_eq!(s.opened_bytes(id).unwrap().as_ref(), original.as_ref());
        assert_eq!(sha256(s.opened_bytes(id).unwrap().as_ref()), original_hash);
        assert_eq!(s.save_bytes(id), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, Edit::DeletePages { pages: vec![0] }), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, Edit::SetAnnotationContents { page: 0, index: 0, text: "no".into() }), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, Edit::MoveAnnotation { page: 0, index: 1, dx: 4.0, dy: 0.0 }), Err(EditError::NotPermitted("this annotation")));
        s.apply(id, Edit::MoveAnnotation { page: 0, index: 0, dx: 10.0, dy: 0.0 }).unwrap();
        s.apply(id, Edit::ResizeAnnotation { page: 0, index: 0, rect: [50.0, 40.0, 140.0, 130.0] }).unwrap();
        s.apply(id, Edit::StyleAnnotation { page: 0, index: 0, color: None, opacity: Some(0.25), width: None, endings: None }).unwrap();
        assert_eq!(
            s.apply(id, Edit::StyleAnnotation { page: 0, index: 0, color: Some([1.0, 0.0, 0.0]), opacity: Some(0.5), width: None, endings: None }),
            Err(EditError::NotPermitted("this annotation"))
        );
        assert_eq!(s.undo(id).unwrap().as_str(), "Change comment properties");
        assert_eq!(s.undo(id).unwrap().as_str(), "Resize comment");
        assert_eq!(s.undo(id).unwrap().as_str(), "Move comment");
        assert!(s.owns_stamp(id, 0, 0));
        s.undo(id).unwrap();
        assert_eq!(s.get(id).unwrap().info.annotations.len(), 1);
        s.redo(id).unwrap();
        assert!(s.owns_stamp(id, 0, 0));
        s.grant(Capability::SaveAs);
        let saved = s.save_edited_copy_bytes(id).unwrap();
        assert!(saved.windows(5).any(|window| window == b"SMask"));
        assert_eq!(s.opened_bytes(id).unwrap().as_ref(), original.as_ref());
        s.grant(Capability::Print);
        let markups =
            s.print_pdf(id, &crate::print::Settings { pages: vec![0], content: Content::DocumentAndMarkups, ..Default::default() }).unwrap();
        let stamps = s.print_pdf(id, &crate::print::Settings { pages: vec![0], content: Content::DocumentAndStamps, ..Default::default() }).unwrap();
        let bare = s.print_pdf(id, &crate::print::Settings { pages: vec![0], content: Content::Document, ..Default::default() }).unwrap();
        assert_ne!(markups, bare);
        assert_ne!(stamps, bare);
        assert!(s.get(id).unwrap().can_undo().is_some());
        assert_eq!(sha256(s.opened_bytes(id).unwrap().as_ref()), original_hash);

        let mut maker = Session::new().with_clock(|| 1_700_000_000);
        let made = maker.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        maker.apply(made, stamp_edit(0, [30.0, 30.0, 90.0, 90.0], 1.0)).unwrap();
        let prior = maker.save_bytes(made).unwrap();
        let mut later = Session::new().with_view_only(true);
        later.grant(Capability::Stamp);
        let id = later.open("prior.pdf", None, prior, None).unwrap();
        assert!(!later.owns_stamp(id, 0, 0));
        assert_eq!(later.apply(id, Edit::MoveAnnotation { page: 0, index: 0, dx: 5.0, dy: 0.0 }), Err(EditError::NotPermitted("this annotation")));
        assert_eq!(later.undo(id), Err(EditError::ViewOnly));
        maker.set_view_only(true);
        maker.grant(Capability::Stamp);
        assert!(!maker.owns_stamp(made, 0, 0));
        assert_eq!(maker.undo(made), Err(EditError::NotPermitted("this annotation")));
        assert_eq!(maker.apply(made, Edit::DeleteAnnotation { page: 0, index: 0 }), Err(EditError::NotPermitted("this annotation")));

        let mut author = Session::new().with_clock(|| 1_700_000_000);
        let id = author.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        author
            .apply(
                id,
                Edit::Protect(Protection {
                    permissions_password: Some("owner".into()),
                    printing: Printing::High,
                    changes: Changes::None,
                    ..Protection::default()
                }),
            )
            .unwrap();
        let locked = author.save_bytes(id).unwrap();
        let mut banned = Session::new().with_view_only(true);
        banned.grant(Capability::Stamp);
        let id = banned.open("banned.pdf", None, locked, None).unwrap();
        assert!(!banned.allows_stamp(id));
        assert_eq!(banned.apply(id, stamp_edit(0, [40.0, 40.0, 80.0, 80.0], 1.0)), Err(EditError::NotPermitted("comments")));

        let mut signer = Session::new().with_clock(|| 1_800_000_000);
        let id = signer.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        let signed = signer.sign(id, &digital_id(), SignOptions { rect: None, ..SignOptions::default() }).unwrap();
        let mut signed_session = Session::new().with_view_only(true);
        signed_session.grant(Capability::Stamp);
        signed_session.grant(Capability::Print);
        signed_session.grant(Capability::SaveAs);
        let id = signed_session.open("signed.pdf", None, signed.clone(), None).unwrap();
        assert!(!signed_session.allows_stamp(id));
        assert_eq!(
            signed_session.apply(id, stamp_edit(0, [40.0, 40.0, 80.0, 80.0], 1.0)),
            Err(EditError::NotPermitted("stamps on a signed document"))
        );
        assert!(std::sync::Arc::ptr_eq(&signed_session.get(id).unwrap().bytes, &signed));
        assert!(signed_session.get(id).unwrap().can_undo().is_none());
        assert_eq!(signed_session.save_bytes(id), Err(EditError::ViewOnly));
        let printed = signed_session.print_pdf(id, &crate::print::Settings { pages: vec![0], ..Default::default() }).unwrap();
        assert!(printed.starts_with(b"%PDF"));
        assert!(std::sync::Arc::ptr_eq(&signed_session.get(id).unwrap().bytes, &signed));

        let mut broken = Session::new().with_view_only(true);
        broken.grant(Capability::Stamp);
        match broken.open("broken.pdf", None, Arc::new(b"%PDF-1.1\n%%EOF\n".to_vec()), None) {
            Err(_) => {}
            Ok(id) => {
                let err = broken.apply(id, stamp_edit(0, [40.0, 40.0, 80.0, 80.0], 1.0)).unwrap_err();
                assert!(matches!(err, EditError::ReadOnly(_) | EditError::NotPermitted("stamps on a signed document")), "{err:?}");
            }
        }

        let images = Session::new();
        let scan = images.create_from_images(&[("scan.png".into(), pdfcraft_xfa::fixtures::tiny_png())]).unwrap();
        let mut scanned = Session::new().with_view_only(true);
        scanned.grant(Capability::Stamp);
        let id = scanned.open("scan.pdf", None, scan, None).unwrap();
        scanned.apply(id, stamp_edit(0, [4.0, 4.0, 20.0, 20.0], 1.0)).unwrap();
        assert_eq!(scanned.get(id).unwrap().info.annotations.len(), 1);
        assert!(scanned.owns_stamp(id, 0, 0));
    }

    #[test]
    fn view_only_page_rotation_saves_a_new_file_and_refuses_everything_else() {
        use crate::AddedText;
        let (mut s, id) = view_only_session(3);
        let original = s.opened_bytes(id).unwrap();
        let rotate = |pages: Vec<usize>, degrees: i64| Edit::RotatePages { pages, degrees };
        assert_eq!(s.apply(id, rotate(vec![0], 90)), Err(EditError::ViewOnly));
        assert!(!s.allows_page_rotate(id));
        s.grant(Capability::PageRotate);
        assert!(s.view_only());
        assert!(!s.get(id).unwrap().editable());
        assert!(s.allows_page_rotate(id));
        assert_eq!(s.apply(id, Edit::DeletePages { pages: vec![1] }), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, Edit::MovePages { pages: vec![2], to: 0 }), Err(EditError::ViewOnly));
        assert_eq!(s.extract(id, &[0]), Err(EditError::ViewOnly));
        assert_eq!(s.apply(id, rotate(vec![0, 9], 90)), Err(EditError::Organize(pdfcraft_organize::OrganizeError::NoSuchPage(9))));
        assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 0);
        assert!(s.get(id).unwrap().can_undo().is_none());

        s.apply(id, rotate(vec![0, 0], 90)).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages.iter().map(|page| page.rotation).collect::<Vec<_>>(), [90, 0, 0]);
        assert_eq!(s.undo(id).unwrap().as_str(), "Rotate pages");
        assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 0);
        assert_eq!(s.redo(id).unwrap().as_str(), "Rotate pages");
        assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 90);
        s.apply(id, rotate(vec![1], -90)).unwrap();
        s.apply(id, rotate(vec![0, 2], 90)).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages.iter().map(|page| page.rotation).collect::<Vec<_>>(), [180, 270, 90]);
        let before_turns = s.get(id).unwrap().info.pages[2].rotation;
        for _ in 0..4 {
            s.apply(id, rotate(vec![2], 90)).unwrap();
        }
        assert_eq!(s.get(id).unwrap().info.pages[2].rotation, before_turns);

        assert_eq!(s.save_edited_copy_bytes(id), Err(EditError::ViewOnly));
        s.grant(Capability::SaveAs);
        let saved = s.save_edited_copy_bytes(id).unwrap();
        assert!(saved.starts_with(original.as_ref()), "rotation is an incremental update");
        assert!(std::sync::Arc::ptr_eq(&s.opened_bytes(id).unwrap(), &original));
        assert_eq!(s.save_bytes(id), Err(EditError::ViewOnly));
        let mut again = Session::new().with_view_only(true);
        let copy = again.open("copy.pdf", None, saved, None).unwrap();
        assert_eq!(again.get(copy).unwrap().info.pages.iter().map(|page| page.rotation).collect::<Vec<_>>(), [180, 270, 90]);
        assert_eq!(page_text(&again, copy, 0), "Page 1");

        if pdfcraft_fonts::document_japanese_font().is_some() {
            s.grant(Capability::EditText);
            s.apply(
                id,
                Edit::AddText {
                    page: 0,
                    text: AddedText { rect: [10.0, 200.0, 80.0, 240.0], text: "東京".into(), size: 18.0, ..AddedText::default() },
                },
            )
            .unwrap();
            s.revoke(Capability::EditText);
            assert!(s.save_edited_copy_bytes(id).is_err(), "text in the history is not saved by the rotation grant");
            s.grant(Capability::EditText);
            let with_text = s.save_edited_copy_bytes(id).unwrap();
            let mut reopened = Session::new().with_view_only(true);
            let rid = reopened.open("rotated.pdf", None, with_text, None).unwrap();
            let doc = reopened.get(rid).unwrap();
            assert_eq!(doc.info.pages[0].rotation, 180);
            assert!(
                doc.added()
                    .iter()
                    .any(|item| { matches!(&item.content, pdfcraft_edit::Content::Text(text) if text.text == "東京" && item.page == 0) })
            );
            assert_eq!(s.undo(id).unwrap().as_str(), "Add text");
            assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 180, "undoing the text leaves the rotation");
            assert!(s.get(id).unwrap().added().is_empty());
            assert_eq!(s.redo(id).unwrap().as_str(), "Add text");
            assert!(s.get(id).unwrap().added().iter().any(|item| item.page == 0));
        }

        s.grant(Capability::Stamp);
        s.apply(id, stamp_edit(0, [20.0, 20.0, 80.0, 80.0], 1.0)).unwrap();
        s.apply(id, rotate(vec![0], 90)).unwrap();
        let doc = s.get(id).unwrap();
        assert_eq!(doc.info.pages[0].rotation, 270);
        assert_eq!(doc.info.annotations.iter().filter(|a| a.subtype == "Stamp").map(|a| a.page).collect::<Vec<_>>(), [0]);
        s.undo(id).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 180);
        assert_eq!(s.get(id).unwrap().info.annotations.len(), 1);
    }

    #[test]
    fn signed_and_restricted_documents_refuse_page_rotation_before_it_changes_them() {
        use crate::{Changes, Printing, Protection};
        let mut signer = Session::new().with_clock(|| 1_800_000_000);
        let id = signer.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        let signed = signer.sign(id, &digital_id(), SignOptions { rect: None, ..SignOptions::default() }).unwrap();
        let mut open = Session::new().with_clock(|| 1_800_000_000);
        let id = open.open("signed.pdf", None, signed.clone(), None).unwrap();
        assert!(open.get(id).unwrap().is_signed());
        assert!(!open.view_only());
        assert_eq!(
            open.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }),
            Err(EditError::NotPermitted("page rotation on a signed document"))
        );
        assert!(!open.get(id).unwrap().dirty);
        assert!(open.get(id).unwrap().can_undo().is_none());

        let mut view = Session::new().with_view_only(true).with_clock(|| 1_800_000_000);
        view.grant(Capability::PageRotate);
        view.grant(Capability::SaveAs);
        let id = view.open("signed.pdf", None, signed, None).unwrap();
        assert_eq!(
            view.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }),
            Err(EditError::NotPermitted("page rotation on a signed document"))
        );
        assert_eq!(
            view.save_edited_copy_bytes(id).unwrap_err(),
            EditError::Invalid("there is no added text, stamp, or page rotation to save".into())
        );

        let mut author = Session::new().with_clock(|| 1_700_000_000);
        let id = author.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
        author
            .apply(
                id,
                Edit::Protect(Protection {
                    permissions_password: Some("owner".into()),
                    printing: Printing::High,
                    changes: Changes::None,
                    ..Protection::default()
                }),
            )
            .unwrap();
        let locked = author.save_bytes(id).unwrap();
        let mut reader = Session::new().with_view_only(true);
        reader.grant(Capability::PageRotate);
        let id = reader.open("locked.pdf", None, locked.clone(), None).unwrap();
        assert!(!reader.allows_page_rotate(id));
        assert_eq!(reader.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }), Err(EditError::NotPermitted("page changes")));
        assert!(std::sync::Arc::ptr_eq(&reader.get(id).unwrap().bytes, &locked));
        assert!(!reader.get(id).unwrap().dirty);
    }

    fn text_of(bytes: Vec<u8>) -> String {
        let mut s = Session::new();
        let id = s.open("printed.pdf", None, Arc::new(bytes), None).unwrap();
        page_text(&s, id, 0)
    }
}
