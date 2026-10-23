//! Page-rotation guard for signature-shaped PDF objects.
//!
//! This does not decide whether a signature is cryptographically valid, and it does not change
//! [`crate::Document::is_signed`]. It only answers whether page rotation can prove the file has
//! no signature-shaped structure. Anything signed, signature-shaped, or unreadable is refused.

use std::collections::HashSet;
use std::sync::Arc;

use pdfcraft_cos::{Dict, Document, ObjRef, Object};

/// Shown when rotation is refused. It does not claim the signature verifies.
pub(crate) const MESSAGE: &str = "Page rotation is unavailable because the document contains or may contain a digital signature.";

/// How many older revisions are opened. Past this, the file is treated as unreadable rather
/// than assumed unsigned.
const MAX_OLDER_REVISIONS: usize = 64;

/// Field tree nodes visited while looking for `/FT /Sig`. Past this, the tree is unreadable.
const MAX_FIELD_NODES: u32 = 10_000;

/// Objects the parser will walk for one document: current object numbers, excluding freed entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScanLimit {
    pub objects: usize,
    pub lowest: Option<u32>,
    pub highest: Option<u32>,
    pub revisions: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Assessment {
    /// No signature-shaped structure in the objects and older revisions that were read.
    Unsigned(ScanLimit),
    /// A signature field value, certification or usage-rights entry, or signature dictionary.
    ProtectedOrSuspect(ScanLimit),
    /// A reference or an older revision could not be read, so the file is not treated as unsigned.
    Indeterminate(ScanLimit),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Found {
    Clear,
    Suspect,
    Unreadable,
}

pub(crate) fn blocks_page_rotation(cos: &Document) -> bool {
    !matches!(assess(cos), Assessment::Unsigned(_))
}

pub(crate) fn assess(cos: &Document) -> Assessment {
    let limit = limit_of(cos);
    if cross_references_unclear(cos) {
        return Assessment::Indeterminate(limit);
    }
    match assess_current(cos) {
        Found::Clear => {}
        Found::Suspect => return Assessment::ProtectedOrSuspect(limit),
        Found::Unreadable => return Assessment::Indeterminate(limit),
    }
    let ends = cos.revision_ends();
    if ends.len() > MAX_OLDER_REVISIONS.saturating_add(1) {
        return Assessment::Indeterminate(limit);
    }
    let bytes = cos.bytes();
    let older = ends.len().saturating_sub(1);
    for end in ends.iter().take(older) {
        let Some(prefix) = bytes.get(..*end) else {
            return Assessment::Indeterminate(limit);
        };
        if prefix.is_empty() {
            return Assessment::Indeterminate(limit);
        }
        // The file key of an authenticated document opens its own older revisions. A prefix
        // encrypted any other way fails here and stays Indeterminate.
        let opened = match cos.open_with_same_credentials(Arc::new(prefix.to_vec())) {
            Ok(doc) => doc,
            Err(_) => return Assessment::Indeterminate(limit),
        };
        if cross_references_unclear(&opened) {
            return Assessment::Indeterminate(limit);
        }
        match assess_current(&opened) {
            Found::Clear => {}
            Found::Suspect => return Assessment::ProtectedOrSuspect(limit),
            Found::Unreadable => return Assessment::Indeterminate(limit),
        }
    }
    Assessment::Unsigned(limit)
}

/// Whether another reader may resolve object numbers differently from cos, so the objects
/// checked here need not be the ones that reader shows. That is the case when the reader
/// rebuilt or repaired the cross-reference data, or set aside entries that disagree (a hybrid
/// /XRefStm against its table, one number listed twice in a section). The repair log also
/// carries leniencies that are not about cross-references; those refuse too.
fn cross_references_unclear(doc: &Document) -> bool {
    !doc.repair_log().is_empty() || !doc.xref_oddities().is_empty()
}

fn limit_of(cos: &Document) -> ScanLimit {
    let numbers = cos.object_numbers();
    ScanLimit { objects: numbers.len(), lowest: numbers.first().copied(), highest: numbers.last().copied(), revisions: cos.revisions().len() }
}

fn assess_current(cos: &Document) -> Found {
    match fields_and_perms(cos) {
        Found::Clear => {}
        other => return other,
    }
    // The trailer is not an object number. A signature key placed there is still part of the file.
    match signature_dictionary(cos, cos.trailer(), false) {
        Found::Clear => {}
        other => return other,
    }
    if let Some(Object::Dict(dict)) = cos.trailer().get(b"Encrypt") {
        match signature_dictionary(cos, dict, true) {
            Found::Clear => {}
            other => return other,
        }
    }
    let encrypt = cos.trailer().reference(b"Encrypt").map(|refer| refer.num);
    for (refer, result) in cos.scan_objects_checked() {
        let object = match result {
            Ok(object) => object,
            Err(_) => return Found::Unreadable,
        };
        if let Some(dict) = object.as_dict() {
            match signature_dictionary(cos, dict, encrypt == Some(refer.num)) {
                Found::Clear => {}
                other => return other,
            }
        }
    }
    Found::Clear
}

fn fields_and_perms(doc: &Document) -> Found {
    let Some(root) = doc.root() else { return Found::Unreadable };
    let catalog = match doc.try_get(root.num) {
        Ok(object) => object,
        Err(_) => return Found::Unreadable,
    };
    let Some(catalog) = catalog.as_dict() else { return Found::Unreadable };
    let acro = catalog.get(b"AcroForm").cloned();
    let perms = catalog.get(b"Perms").cloned();
    if let Some(perms) = perms {
        match perms_block(doc, &perms) {
            Found::Clear => {}
            other => return other,
        }
    }
    if let Some(acro) = acro {
        return signature_fields(doc, &acro);
    }
    Found::Clear
}

fn perms_block(doc: &Document, raw: &Object) -> Found {
    let object = match resolve_owned(doc, raw) {
        Ok(object) => object,
        Err(found) => return found,
    };
    if matches!(object.as_ref(), Object::Null) {
        return Found::Clear;
    }
    let Some(perms) = object.as_dict() else { return Found::Suspect };
    for key in [b"DocMDP".as_slice(), b"UR".as_slice(), b"UR3".as_slice()] {
        if let Some(value) = perms.get(key) {
            match signature_pointer(doc, value) {
                Found::Clear => {}
                other => return other,
            }
        }
    }
    Found::Clear
}

fn signature_fields(doc: &Document, raw: &Object) -> Found {
    let object = match resolve_owned(doc, raw) {
        Ok(object) => object,
        Err(found) => return found,
    };
    if matches!(object.as_ref(), Object::Null) {
        return Found::Clear;
    }
    let Some(form) = object.as_dict() else { return Found::Unreadable };
    let Some(fields_raw) = form.get(b"Fields").cloned() else { return Found::Clear };
    let fields = match resolve_owned(doc, &fields_raw) {
        Ok(object) => object,
        Err(found) => return found,
    };
    if matches!(fields.as_ref(), Object::Null) {
        return Found::Clear;
    }
    let Some(fields) = fields.as_array() else { return Found::Unreadable };
    walk_fields(doc, fields)
}

struct FieldNode {
    value: Object,
    inherited_ft: Option<Vec<u8>>,
}

fn walk_fields(doc: &Document, fields: &[Object]) -> Found {
    let mut stack: Vec<FieldNode> = fields.iter().rev().cloned().map(|value| FieldNode { value, inherited_ft: None }).collect();
    let mut seen = HashSet::<ObjRef>::new();
    let mut visits = 0u32;
    while let Some(node) = stack.pop() {
        visits = visits.saturating_add(1);
        if visits > MAX_FIELD_NODES {
            return Found::Unreadable;
        }
        let object = match &node.value {
            Object::Null => continue,
            Object::Ref(refer) => {
                if !seen.insert(*refer) {
                    continue;
                }
                match doc.try_get(refer.num) {
                    Ok(object) => object,
                    Err(_) => return Found::Unreadable,
                }
            }
            other => Arc::new(other.clone()),
        };
        if matches!(object.as_ref(), Object::Null) {
            return Found::Unreadable;
        }
        let Some(dict) = object.as_dict().cloned() else { return Found::Unreadable };
        let own = match own_ft(doc, &dict) {
            Ok(name) => name,
            Err(found) => return found,
        };
        let ft = own.or(node.inherited_ft);
        if ft.as_deref() == Some(b"Sig") {
            match signature_value(doc, &dict) {
                Found::Clear => {}
                other => return other,
            }
        }
        let Some(kids_raw) = dict.get(b"Kids").cloned() else { continue };
        let kids = match resolve_owned(doc, &kids_raw) {
            Ok(object) => object,
            Err(found) => return found,
        };
        if matches!(kids.as_ref(), Object::Null) {
            continue;
        }
        let Some(kids) = kids.as_array() else { return Found::Unreadable };
        for kid in kids.iter().rev().cloned() {
            stack.push(FieldNode { value: kid, inherited_ft: ft.clone() });
        }
    }
    Found::Clear
}

fn own_ft(doc: &Document, dict: &Dict) -> Result<Option<Vec<u8>>, Found> {
    let Some(raw) = dict.get(b"FT") else { return Ok(None) };
    match raw {
        Object::Name(name) => Ok(Some(name.clone())),
        Object::Null => Ok(None),
        Object::Ref(refer) => match doc.try_get(refer.num) {
            Err(_) => Err(Found::Unreadable),
            Ok(object) => match object.as_name() {
                Some(name) => Ok(Some(name.to_vec())),
                None if matches!(object.as_ref(), Object::Null) => Err(Found::Unreadable),
                None => Ok(None),
            },
        },
        _ => Ok(None),
    }
}

/// `/V` on a signature field. Missing and explicit null are an empty field. Anything else,
/// including a dangling reference or a non-dictionary, is not treated as unsigned.
fn signature_value(doc: &Document, dict: &Dict) -> Found {
    let Some(raw) = dict.get(b"V") else { return Found::Clear };
    match raw {
        Object::Null => Found::Clear,
        Object::Ref(refer) => match doc.try_get(refer.num) {
            Err(_) => Found::Unreadable,
            Ok(object) if matches!(object.as_ref(), Object::Null) => Found::Suspect,
            Ok(_) => Found::Suspect,
        },
        _ => Found::Suspect,
    }
}

/// A `/Perms` entry. Explicit null is the absence of a signature. A dictionary, a dangling
/// reference, or any other value is not treated as unsigned.
fn signature_pointer(doc: &Document, raw: &Object) -> Found {
    match raw {
        Object::Null => Found::Clear,
        Object::Ref(refer) => match doc.try_get(refer.num) {
            Err(_) => Found::Unreadable,
            Ok(object) if matches!(object.as_ref(), Object::Null) => Found::Suspect,
            Ok(_) => Found::Suspect,
        },
        _ => Found::Suspect,
    }
}

fn signature_dictionary(doc: &Document, dict: &Dict, encrypt_object: bool) -> Found {
    if let Some(b"Sig" | b"DocTimeStamp" | b"SigRef" | b"TransformParams") = dict.name(b"Type") {
        return Found::Suspect;
    }
    if let Some(Object::Ref(refer)) = dict.get(b"Type") {
        match doc.try_get(refer.num) {
            Err(_) => return Found::Unreadable,
            Ok(object) => {
                if matches!(object.as_name(), Some(b"Sig" | b"DocTimeStamp" | b"SigRef" | b"TransformParams")) {
                    return Found::Suspect;
                }
                if matches!(object.as_ref(), Object::Null) {
                    return Found::Unreadable;
                }
            }
        }
    }
    match byte_range(doc, dict) {
        Found::Clear => {}
        other => return other,
    }
    subfilter(doc, dict, encrypt_object)
}

/// A `/ByteRange` key on a dictionary is signature-shaped whatever the value is. A short array,
/// a string, or a dangling reference is not treated as proof that the key is unrelated. The word
/// inside a content stream is not this key.
fn byte_range(doc: &Document, dict: &Dict) -> Found {
    let Some(raw) = dict.get(b"ByteRange") else { return Found::Clear };
    match resolve_owned(doc, raw) {
        Ok(_) => Found::Suspect,
        Err(found) => found,
    }
}

/// Public-key encryption uses these `/SubFilter` names on the trailer `/Encrypt` dictionary
/// (ISO 32000-2 §7.6.4). They are not signature formats.
fn encryption_subfilter(name: &[u8]) -> bool {
    matches!(name, b"adbe.pkcs7.s3" | b"adbe.pkcs7.s4" | b"adbe.pkcs7.s5")
}

/// `/SubFilter` is refused unless this dictionary is the document encryption dictionary and the
/// value is one of the public-key encryption names above. A signature name, an unknown name, a
/// non-name, or the same encryption name on any other dictionary is not treated as unsigned.
fn subfilter(doc: &Document, dict: &Dict, encrypt_object: bool) -> Found {
    let Some(raw) = dict.get(b"SubFilter") else { return Found::Clear };
    let object = match resolve_owned(doc, raw) {
        Ok(object) => object,
        Err(found) => return found,
    };
    if matches!(object.as_ref(), Object::Null) {
        return Found::Unreadable;
    }
    if encrypt_object && object.as_name().is_some_and(encryption_subfilter) && !dict.contains(b"ByteRange") {
        return Found::Clear;
    }
    Found::Suspect
}

fn resolve_owned(doc: &Document, raw: &Object) -> Result<Arc<Object>, Found> {
    match raw {
        Object::Ref(refer) => doc.try_get(refer.num).map_err(|_| Found::Unreadable),
        other => Ok(Arc::new(other.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::{Assessment, assess};
    use pdfcraft_cos::{Dict, Document, Object, PdfString};
    use std::sync::Arc;

    fn signature() -> Dict {
        let mut dict = Dict::new();
        dict.set(b"Type".to_vec(), Object::name("Sig"));
        dict.set(b"Filter".to_vec(), Object::name("Adobe.PPKLite"));
        dict.set(b"SubFilter".to_vec(), Object::name("adbe.pkcs7.detached"));
        dict.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]));
        dict.set(b"Contents".to_vec(), Object::String(PdfString::literal(vec![1, 2])));
        dict
    }

    #[test]
    fn an_unreferenced_signature_is_inside_the_numbers_the_parser_walks() {
        let mut doc = Document::new_empty();
        let added = doc.add(signature());
        let Assessment::ProtectedOrSuspect(limit) = assess(&doc) else { panic!("unreferenced signature was treated as unsigned") };
        assert!(limit.objects >= 1);
        let low = limit.lowest.expect("scan range");
        let high = limit.highest.expect("scan range");
        assert!(low <= added.num && added.num <= high, "signature {added:?} outside {low}..={high}");
    }

    #[test]
    fn a_private_note_that_mentions_signature_words_stays_unsigned() {
        let mut doc = Document::new_empty();
        let root = doc.root().expect("catalog");
        let mut note = Dict::new();
        note.set(b"Comment".to_vec(), Object::String(PdfString::literal(b"ByteRange DocMDP /Sig")));
        doc.update_dict(root, |dict| dict.set(b"PrivateNote".to_vec(), Object::Dict(note))).expect("note");
        assert!(matches!(assess(&doc), Assessment::Unsigned(_)));
    }

    #[test]
    fn a_signature_freed_in_a_later_revision_is_still_refused() {
        use pdfcraft_cos::{SaveOptions, write_full, write_incremental};
        let mut doc = Document::new_empty();
        let pages = doc.get(doc.root().expect("catalog")).as_dict().expect("catalog").reference(b"Pages").expect("pages");
        let mut page = Dict::new();
        page.set(b"Type".to_vec(), Object::name("Page"));
        page.set(b"Parent".to_vec(), Object::Ref(pages));
        page.set(b"MediaBox".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(10), Object::Int(10)]));
        let page_ref = doc.add(page);
        doc.update_dict(pages, |dict| {
            dict.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(page_ref)]));
            dict.set(b"Count".to_vec(), Object::Int(1));
        })
        .expect("page tree");
        let bytes = write_full(&doc, &SaveOptions::default()).expect("full");
        let mut doc = Document::open(Arc::new(bytes)).expect("open");
        assert!(!doc.revisions().is_empty(), "a written file must keep a revision the next save can append to");
        let sig = doc.add(signature());
        let with_sig = write_incremental(&doc, &SaveOptions::default()).expect("add");
        let mut doc = Document::open(Arc::new(with_sig)).expect("with signature");
        assert!(doc.object_numbers().contains(&sig.num));
        doc.free(sig);
        let saved = write_incremental(&doc, &SaveOptions::default()).expect("free");
        let doc = Document::open(Arc::new(saved)).expect("reopen");
        assert!(!doc.object_numbers().contains(&sig.num), "{:?}", doc.object_numbers());
        assert!(matches!(assess(&doc), Assessment::ProtectedOrSuspect(_)));
    }

    #[test]
    fn a_content_stream_filter_without_byte_range_stays_unsigned() {
        let mut doc = Document::new_empty();
        let mut stream = Dict::new();
        stream.set(b"Filter".to_vec(), Object::name("FlateDecode"));
        stream.set(b"Length".to_vec(), Object::Int(0));
        let _ = doc.add(stream);
        assert!(matches!(assess(&doc), Assessment::Unsigned(_)));
    }

    #[test]
    fn the_encryption_dictionary_keeps_its_public_key_subfilter() {
        let mut doc = Document::new_empty();
        let mut encrypt = Dict::new();
        encrypt.set(b"Filter".to_vec(), Object::name("Adobe.PubSec"));
        encrypt.set(b"SubFilter".to_vec(), Object::name("adbe.pkcs7.s5"));
        let refer = doc.add(encrypt);
        doc.trailer_mut().set(b"Encrypt".to_vec(), Object::Ref(refer));
        assert!(matches!(assess(&doc), Assessment::Unsigned(_)));

        let name = doc.add(Object::name("adbe.pkcs7.s4"));
        doc.update_dict(refer, |dict| dict.set(b"SubFilter".to_vec(), Object::Ref(name))).expect("indirect");
        assert!(matches!(assess(&doc), Assessment::Unsigned(_)), "an indirect encryption name on /Encrypt stays non-signature");

        doc.update_dict(refer, |dict| dict.set(b"SubFilter".to_vec(), Object::name("adbe.pkcs7.detached"))).expect("signature name");
        assert!(matches!(assess(&doc), Assessment::ProtectedOrSuspect(_)));
    }

    #[test]
    fn an_orphan_encryption_subfilter_is_not_treated_as_the_encrypt_dictionary() {
        let mut doc = Document::new_empty();
        let mut dict = Dict::new();
        dict.set(b"SubFilter".to_vec(), Object::name("adbe.pkcs7.s5"));
        let _ = doc.add(dict);
        assert!(matches!(assess(&doc), Assessment::ProtectedOrSuspect(_)));
    }

    #[test]
    fn a_signature_inside_an_object_stream_is_refused() {
        // Object 3 lives only in object stream 4. The classic table marks it free; the xref
        // stream says where it is. The dictionary is not a top-level object.
        let mut out = b"%PDF-1.5\n".to_vec();
        let mut offs = [0usize; 6];
        let mut obj = |out: &mut Vec<u8>, n: usize, body: &[u8]| {
            offs[n] = out.len();
            out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        };
        obj(&mut out, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
        obj(&mut out, 2, b"<< /Type /Pages /Kids [] /Count 0 >>");
        let inner = b"3 0 << /Type /Sig /SubFilter /adbe.pkcs7.detached >>";
        let mut stm = format!("<< /Type /ObjStm /N 1 /First 4 /Length {} >>\nstream\n", inner.len()).into_bytes();
        stm.extend_from_slice(inner);
        stm.extend_from_slice(b"\nendstream");
        obj(&mut out, 4, &stm);
        let data = [2u8, 0, 4, 0];
        let mut xs = format!("<< /Type /XRef /Size 6 /W [1 2 1] /Index [3 1] /Length {} >>\nstream\n", data.len()).into_bytes();
        xs.extend_from_slice(&data);
        xs.extend_from_slice(b"\nendstream");
        obj(&mut out, 5, &xs);
        let table = out.len();
        out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for (n, off) in offs.iter().enumerate().skip(1) {
            let line = if n == 3 { "0000000000 65535 f \n".to_string() } else { format!("{off:010} 00000 n \n") };
            out.extend_from_slice(line.as_bytes());
        }
        out.extend_from_slice(format!("trailer << /Size 6 /Root 1 0 R /XRefStm {} >>\nstartxref\n{table}\n%%EOF\n", offs[5]).as_bytes());
        let doc = Document::open(Arc::new(out)).expect("open");
        assert!(!doc.revisions().is_empty() && doc.repair_log().is_empty(), "revisions {:?} log {:?}", doc.revisions().len(), doc.repair_log());
        assert_eq!(doc.get(pdfcraft_cos::ObjRef::new(3, 0)).as_dict().and_then(|d| d.name(b"Type")), Some(b"Sig".as_slice()));
        assert!(matches!(assess(&doc), Assessment::ProtectedOrSuspect(_)));
    }

    #[test]
    fn more_than_sixty_four_older_revisions_stay_indeterminate() {
        use pdfcraft_cos::{SaveOptions, write_full, write_incremental};
        let mut doc = Document::new_empty();
        let pages = doc.get(doc.root().expect("catalog")).as_dict().expect("catalog").reference(b"Pages").expect("pages");
        let mut page = Dict::new();
        page.set(b"Type".to_vec(), Object::name("Page"));
        page.set(b"Parent".to_vec(), Object::Ref(pages));
        page.set(b"MediaBox".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(10), Object::Int(10)]));
        let page_ref = doc.add(page);
        doc.update_dict(pages, |dict| {
            dict.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(page_ref)]));
            dict.set(b"Count".to_vec(), Object::Int(1));
        })
        .expect("pages");
        let mut bytes = write_full(&doc, &SaveOptions::default()).expect("full");
        for n in 0..65 {
            let mut doc = Document::open(Arc::new(bytes)).expect("open");
            let root = doc.root().expect("catalog");
            doc.update_dict(root, |dict| dict.set(b"Lang".to_vec(), Object::String(PdfString::literal(format!("n{n}"))))).expect("lang");
            bytes = write_incremental(&doc, &SaveOptions::default()).expect("incremental");
        }
        let doc = Document::open(Arc::new(bytes)).expect("reopen");
        assert!(doc.revisions().len() > 64, "{}", doc.revisions().len());
        assert!(matches!(assess(&doc), Assessment::Indeterminate(_)));
        assert!(super::blocks_page_rotation(&doc));
    }

    #[test]
    fn a_few_revisions_without_a_signature_still_rotate() {
        use pdfcraft_cos::{SaveOptions, write_full, write_incremental};
        let mut doc = Document::new_empty();
        let root = doc.root().expect("catalog");
        doc.update_dict(root, |dict| dict.set(b"Lang".to_vec(), Object::name("en"))).expect("lang");
        let bytes = write_full(&doc, &SaveOptions::default()).expect("full");
        let mut doc = Document::open(Arc::new(bytes)).expect("open");
        let root = doc.root().expect("catalog");
        doc.update_dict(root, |dict| dict.set(b"Lang".to_vec(), Object::name("ja"))).expect("lang");
        let bytes = write_incremental(&doc, &SaveOptions::default()).expect("incremental");
        let doc = Document::open(Arc::new(bytes)).expect("reopen");
        assert_eq!(doc.revisions().len(), 2);
        assert!(matches!(assess(&doc), Assessment::Unsigned(_)));
    }

    #[test]
    fn a_reconstructed_xref_is_not_treated_as_unsigned() {
        use pdfcraft_cos::{SaveOptions, write_full};
        let doc = Document::new_empty();
        let bytes = write_full(&doc, &SaveOptions::default()).expect("full");
        let marker = b"startxref\n";
        let at = bytes.windows(marker.len()).rposition(|window| window == marker).expect("startxref");
        let mut broken = bytes[..at + marker.len()].to_vec();
        broken.extend_from_slice(b"999999\n%%EOF\n");
        let doc = Document::open(Arc::new(broken)).expect("open");
        assert!(doc.revisions().is_empty() && !doc.repair_log().is_empty());
        assert!(matches!(assess(&doc), Assessment::Indeterminate(_)));
    }

    fn reconstructed(body: &str) -> Document {
        let text = format!(
            "%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R >> endobj\n{body}trailer << /Root 1 0 R >>\n%%EOF"
        );
        let doc = Document::open(Arc::new(text.into_bytes())).expect("open");
        assert!(doc.revisions().is_empty() && !doc.repair_log().is_empty(), "the fixture must be reconstructed");
        doc
    }

    #[test]
    fn a_reconstructed_file_without_signature_names_is_still_refused() {
        assert!(matches!(assess(&reconstructed("")), Assessment::Indeterminate(_)));
        let note = "4 0 obj << /Title (ByteRange and DocMDP in a string) >> endobj\n";
        assert!(matches!(assess(&reconstructed(note)), Assessment::Indeterminate(_)));
        let slash = "4 0 obj << /Title (a /Sig inside a string) >> endobj\n";
        assert!(matches!(assess(&reconstructed(slash)), Assessment::Indeterminate(_)));
    }

    #[test]
    fn a_readable_cross_reference_with_a_repair_note_is_refused() {
        use pdfcraft_cos::{SaveOptions, write_full};
        let bytes = write_full(&Document::new_empty(), &SaveOptions::default()).expect("full");
        let mut doc = Document::open(Arc::new(bytes)).expect("open");
        assert!(!doc.revisions().is_empty() && doc.repair_log().is_empty() && doc.xref_oddities().is_empty());
        assert!(matches!(assess(&doc), Assessment::Unsigned(_)));
        doc.note_repair("hybrid /XRefStm at 5 unreadable".into());
        assert!(matches!(assess(&doc), Assessment::Indeterminate(_)));
    }

    #[test]
    fn a_reconstructed_file_hiding_an_older_signature_definition_is_refused() {
        let doc = reconstructed("5 0 obj << /Type /Sig /Contents <00> >> endobj\n5 0 obj << /Lang (en) >> endobj\n");
        assert_eq!(doc.get(pdfcraft_cos::ObjRef::new(5, 0)).as_dict().and_then(|d| d.name(b"Type")), None, "the reader keeps the last definition");
        assert!(matches!(assess(&doc), Assessment::Indeterminate(_)));
    }

    #[test]
    fn a_reconstructed_file_with_signature_shaped_names_is_refused() {
        for body in [
            "4 0 obj << /Type /S#69g >> endobj\n",
            "4 0 obj << /Foo [0 1 2 3] /ByteRange 5 >> endobj\n",
            "4 0 obj << /SigFlags 3 >> endobj\n",
            "4 0 obj << /Perms << >> >> endobj\n",
            "4 0 obj << /Filter /Standard >> endobj\n9 0 obj << /Encrypt 4 0 R >> endobj\n",
            "4 0 obj << /Type /ObjStm /N 0 /First 0 /Length 0 >>\nstream\n\nendstream\nendobj\n",
        ] {
            assert!(matches!(assess(&reconstructed(body)), Assessment::Indeterminate(_)), "{body}");
        }
    }

    fn encrypted_with_page() -> Vec<u8> {
        use pdfcraft_cos::{Algorithm, NewEncryption, SaveOptions, write_full};
        let mut doc = Document::new_empty();
        let pages = doc.get(doc.root().expect("catalog")).as_dict().expect("catalog").reference(b"Pages").expect("pages");
        let mut page = Dict::new();
        page.set(b"Type".to_vec(), Object::name("Page"));
        page.set(b"Parent".to_vec(), Object::Ref(pages));
        page.set(b"MediaBox".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(10), Object::Int(10)]));
        let page_ref = doc.add(page);
        doc.update_dict(pages, |dict| {
            dict.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(page_ref)]));
            dict.set(b"Count".to_vec(), Object::Int(1));
        })
        .expect("page tree");
        doc.set_encryption(&NewEncryption {
            algorithm: Algorithm::Aes256,
            user_password: "pw",
            owner_password: "owner",
            permissions: -1,
            encrypt_metadata: true,
            seed: [7; 32],
        })
        .expect("encrypt");
        write_full(&doc, &SaveOptions::default()).expect("full")
    }

    #[test]
    fn an_authenticated_encrypted_file_reads_its_older_revisions() {
        use pdfcraft_cos::{SaveOptions, write_incremental};
        let bytes = encrypted_with_page();
        let mut doc = Document::open_with_password(Arc::new(bytes), Some("pw")).expect("open");
        let root = doc.root().expect("catalog");
        doc.update_dict(root, |dict| dict.set(b"Lang".to_vec(), Object::name("ja"))).expect("lang");
        let saved = write_incremental(&doc, &SaveOptions::default()).expect("incremental");
        let doc = Document::open_with_password(Arc::new(saved), Some("pw")).expect("reopen");
        assert_eq!(doc.revisions().len(), 2);
        assert!(matches!(assess(&doc), Assessment::Unsigned(_)));
    }

    #[test]
    fn a_signature_freed_from_an_encrypted_file_is_still_refused() {
        use pdfcraft_cos::{SaveOptions, write_incremental};
        let bytes = encrypted_with_page();
        let mut doc = Document::open_with_password(Arc::new(bytes), Some("pw")).expect("open");
        let sig = doc.add(signature());
        let with_sig = write_incremental(&doc, &SaveOptions::default()).expect("add");
        let mut doc = Document::open_with_password(Arc::new(with_sig), Some("pw")).expect("with signature");
        doc.free(sig);
        let saved = write_incremental(&doc, &SaveOptions::default()).expect("free");
        let doc = Document::open_with_password(Arc::new(saved), Some("pw")).expect("reopen");
        assert!(!doc.object_numbers().contains(&sig.num));
        assert!(matches!(assess(&doc), Assessment::ProtectedOrSuspect(_)), "the older encrypted revision is read, not skipped");
    }

    #[test]
    fn guard_cost_stays_small_for_a_wide_file_and_for_repeated_revision_reads() {
        use pdfcraft_cos::{SaveOptions, write_full, write_incremental};
        use std::time::Instant;
        let mut doc = Document::new_empty();
        let mut items = Vec::with_capacity(1000);
        for n in 0..1000 {
            items.push(Object::Ref(doc.add(Object::Int(n))));
        }
        let root = doc.root().expect("catalog");
        doc.update_dict(root, |dict| dict.set(b"Extra".to_vec(), Object::Array(items))).expect("reachable");
        let wide = write_full(&doc, &SaveOptions { object_streams: false, ..SaveOptions::default() }).expect("wide");
        let wide = Document::open(Arc::new(wide)).expect("open wide");
        assert!(wide.object_numbers().len() >= 1000, "{}", wide.object_numbers().len());
        let started = Instant::now();
        assert!(matches!(assess(&wide), Assessment::Unsigned(_)));
        let wide_ms = started.elapsed().as_millis();

        let mut bytes = write_full(&Document::new_empty(), &SaveOptions::default()).expect("base");
        for n in 0..32 {
            let mut doc = Document::open(Arc::new(bytes)).expect("open");
            let root = doc.root().expect("catalog");
            doc.update_dict(root, |dict| dict.set(b"Lang".to_vec(), Object::String(PdfString::literal(format!("r{n}"))))).expect("lang");
            bytes = write_incremental(&doc, &SaveOptions::default()).expect("save");
        }
        let revised = Document::open(Arc::new(bytes)).expect("revised");
        let started = Instant::now();
        for _ in 0..4 {
            assert!(matches!(assess(&revised), Assessment::Unsigned(_)));
        }
        let revised_ms = started.elapsed().as_millis();
        let dir = std::env::var_os("SWIFTPDF_VALIDATE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("swiftpdf-rotate-validate"));
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join("boundary-perf.txt"),
            format!(
                "wide_objects={} wide_ms={wide_ms} revisions={} four_assess_ms={revised_ms}\n",
                wide.object_numbers().len(),
                revised.revisions().len()
            ),
        );
        assert!(wide_ms < 2_000, "scanning 1000 objects took {wide_ms} ms");
        assert!(revised_ms < 5_000, "four assessments of 32 revisions took {revised_ms} ms");
    }
}
