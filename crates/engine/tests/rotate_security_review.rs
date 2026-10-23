//! Page-rotation guard against synthetic signature-shaped files, through the session the app uses.
//! Every file here is hand-written for the test. None is a signed document: the signature
//! dictionaries carry no valid CMS data, so the guard must refuse on structure alone.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use pdfcraft_cos::{Algorithm, Dict, Document, NewEncryption, Object, PdfString, SaveOptions, write_full, write_incremental};
use pdfcraft_engine::{Capability, Edit, Session};

const MESSAGE: &str = "Page rotation is unavailable because the document contains or may contain a digital signature.";
const BASE: [(u32, &str); 3] = [
    (1, "<< /Type /Catalog /Pages 2 0 R >>"),
    (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >>"),
    (3, "<< /Type /Page /Parent 2 0 R >>"),
];

/// Objects written in order, then either a classic table or only a trailer (no xref at all).
fn build(objects: &[(u32, &str)], xref: bool) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (num, body) in objects {
        offsets.push((*num, out.len()));
        out.extend_from_slice(format!("{num} 0 obj\n{body}\nendobj\n").as_bytes());
    }
    if !xref {
        out.extend_from_slice(b"trailer << /Root 1 0 R >>\n%%EOF\n");
        return out;
    }
    let size = objects.iter().map(|(num, _)| *num).max().unwrap_or(0) + 1;
    let table = out.len();
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for num in 1..size {
        // The last definition of a number is the one the table points at.
        match offsets.iter().rev().find(|(n, _)| *n == num) {
            Some((_, off)) => out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes()),
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(format!("trailer << /Size {size} /Root 1 0 R >>\nstartxref\n{table}\n%%EOF\n").as_bytes());
    out
}

fn with(extra: &[(u32, &str)], catalog: Option<&str>) -> Vec<(u32, String)> {
    let mut objects: Vec<(u32, String)> = BASE.iter().map(|(n, b)| (*n, (*b).to_string())).collect();
    if let Some(catalog) = catalog {
        objects[0].1 = catalog.to_string();
    }
    objects.extend(extra.iter().map(|(n, b)| (*n, (*b).to_string())));
    objects
}

fn build_owned(objects: &[(u32, String)], xref: bool) -> Vec<u8> {
    let refs: Vec<(u32, &str)> = objects.iter().map(|(n, b)| (*n, b.as_str())).collect();
    build(&refs, xref)
}

fn break_startxref(bytes: &[u8]) -> Vec<u8> {
    let marker = b"startxref\n";
    let at = bytes.windows(marker.len()).rposition(|w| w == marker).expect("startxref");
    let mut broken = bytes[..at + marker.len()].to_vec();
    broken.extend_from_slice(b"999999\n%%EOF\n");
    broken
}

/// How the parser read the file. Recorded so a refusal can be traced to what the reader saw.
fn interpretation(bytes: &[u8], password: Option<&str>, sig_num: Option<u32>) -> String {
    match Document::open_with_password(Arc::new(bytes.to_vec()), password) {
        Err(e) => format!("open failed: {e}"),
        Ok(doc) => {
            let sig = sig_num.map(|n| {
                let object = doc.get(pdfcraft_cos::ObjRef::new(n, 0));
                let dict = object.as_dict().cloned().or_else(|| match object.as_ref() {
                    Object::Stream(s) => Some(s.dict.clone()),
                    _ => None,
                });
                let shape = dict.map(|d| {
                    let ty = d.name(b"Type").map(|t| String::from_utf8_lossy(t).into_owned()).unwrap_or_default();
                    let ft = d.name(b"FT").map(|t| String::from_utf8_lossy(t).into_owned()).unwrap_or_default();
                    format!("Type={ty} FT={ft} ByteRange={} SubFilter={}", d.contains(b"ByteRange"), d.contains(b"SubFilter"))
                });
                format!(" obj{n}=[{}]", shape.unwrap_or_else(|| "not a dictionary / missing".into()))
            });
            let shaped = doc
                .object_numbers()
                .into_iter()
                .filter(|n| {
                    let object = doc.get(pdfcraft_cos::ObjRef::new(*n, 0));
                    object.as_dict().is_some_and(|d| d.name(b"Type") == Some(b"Sig") || d.name(b"FT") == Some(b"Sig") || d.contains(b"ByteRange"))
                })
                .count();
            format!(
                "revisions={} repaired={} oddities={} objects={} signature_shaped_visible={shaped}{}",
                doc.revisions().len(),
                !doc.repair_log().is_empty(),
                doc.xref_oddities().len(),
                doc.object_numbers().len(),
                sig.unwrap_or_default()
            )
        }
    }
}

/// Rotate page 0 in a view-only session that was granted page rotation, as the app does.
/// `Ok(true)` rotated, `Ok(false)` refused with the guard message and nothing changed.
fn rotate(bytes: &[u8], password: Option<&str>) -> Result<bool, String> {
    let mut session = Session::new().with_view_only(true);
    session.grant(Capability::PageRotate);
    let id = session.open("probe.pdf", None, Arc::new(bytes.to_vec()), password).map_err(|e| format!("open: {e}"))?;
    let before = session.get(id).expect("doc").bytes.clone();
    match session.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }) {
        Ok(_) => Ok(true),
        Err(err) => {
            let doc = session.get(id).expect("doc");
            assert!(Arc::ptr_eq(&doc.bytes, &before), "a refusal must not change the bytes");
            assert!(!doc.dirty, "a refusal must not mark the document edited");
            assert_eq!(doc.can_undo(), None, "a refusal must not add an undo step");
            assert_eq!(doc.can_redo(), None);
            let text = err.to_string();
            if text == MESSAGE || text.contains("signed document") { Ok(false) } else { Err(format!("refused for another reason: {text}")) }
        }
    }
}

struct Case {
    name: &'static str,
    bytes: Vec<u8>,
    password: Option<&'static str>,
    sig_num: Option<u32>,
    allow: bool,
}

fn run(cases: Vec<Case>, log: &str) {
    let mut report = String::new();
    let mut wrong = Vec::new();
    for case in &cases {
        let result = rotate(&case.bytes, case.password);
        let seen = interpretation(&case.bytes, case.password, case.sig_num);
        let outcome = match &result {
            Ok(true) => "rotated",
            Ok(false) => "refused",
            Err(_) => "error",
        };
        let _ = writeln!(report, "{} | expect {} | {outcome} | {seen}", case.name, if case.allow { "rotate" } else { "refuse" });
        if result != Ok(case.allow) {
            wrong.push(format!("{}: {result:?} ({seen})", case.name));
        }
    }
    // `SWIFTPDF_VALIDATE_DIR` when set, otherwise a folder in the system temp directory.
    let dir = std::env::var_os("SWIFTPDF_VALIDATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("swiftpdf-rotate-validate"))
        .join("step111");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(log), &report);
    assert!(wrong.is_empty(), "{wrong:#?}\n{report}");
}

const SIG: &str = "<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange [0 10 20 30] /Contents <00> >>";

fn reconstructed(extra: &[(u32, &str)], catalog: Option<&str>) -> Vec<u8> {
    let bytes = build_owned(&with(extra, catalog), false);
    let doc = Document::open(Arc::new(bytes.clone())).expect("open");
    assert!(doc.revisions().is_empty() && !doc.repair_log().is_empty(), "fixture must be reconstructed");
    bytes
}

fn classic(extra: &[(u32, &str)], catalog: Option<&str>) -> Vec<u8> {
    let bytes = build_owned(&with(extra, catalog), true);
    let doc = Document::open(Arc::new(bytes.clone())).expect("open");
    assert!(!doc.revisions().is_empty() && doc.repair_log().is_empty(), "fixture must have a readable xref");
    bytes
}

fn page_doc() -> Document {
    let mut doc = Document::new_empty();
    let pages = doc.get(doc.root().expect("catalog")).as_dict().expect("catalog").reference(b"Pages").expect("pages");
    let mut page = Dict::new();
    page.set(b"Type".to_vec(), Object::name("Page"));
    page.set(b"Parent".to_vec(), Object::Ref(pages));
    page.set(b"MediaBox".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(300), Object::Int(400)]));
    let page_ref = doc.add(page);
    doc.update_dict(pages, |dict| {
        dict.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(page_ref)]));
        dict.set(b"Count".to_vec(), Object::Int(1));
    })
    .expect("page tree");
    doc
}

fn sig_dict() -> Dict {
    let mut dict = Dict::new();
    dict.set(b"Type".to_vec(), Object::name("Sig"));
    dict.set(b"Filter".to_vec(), Object::name("Adobe.PPKLite"));
    dict.set(b"SubFilter".to_vec(), Object::name("adbe.pkcs7.detached"));
    dict.set(b"ByteRange".to_vec(), Object::Array(vec![0.into(), 10.into(), 20.into(), 30.into()]));
    dict.set(b"Contents".to_vec(), Object::String(PdfString::literal(vec![1, 2])));
    dict
}

/// Revision 1 holds a signature dictionary, revision 2 frees it.
fn freed_signature(password: Option<&str>, base: Vec<u8>) -> (Vec<u8>, u32) {
    let mut doc = Document::open_with_password(Arc::new(base), password).expect("open");
    let sig = doc.add(sig_dict());
    let with_sig = write_incremental(&doc, &SaveOptions::default()).expect("add");
    let mut doc = Document::open_with_password(Arc::new(with_sig), password).expect("with sig");
    doc.free(sig);
    (write_incremental(&doc, &SaveOptions::default()).expect("free"), sig.num)
}

fn encrypted() -> Vec<u8> {
    let mut doc = page_doc();
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

fn encrypted_revisions(n: usize) -> Vec<u8> {
    let mut bytes = encrypted();
    for i in 0..n {
        let mut doc = Document::open_with_password(Arc::new(bytes), Some("pw")).expect("open");
        let root = doc.root().expect("catalog");
        doc.update_dict(root, |d| d.set(b"Lang".to_vec(), Object::String(PdfString::literal(format!("r{i}"))))).expect("lang");
        bytes = write_incremental(&doc, &SaveOptions::default()).expect("incremental");
    }
    bytes
}

/// An appended section that only gives the file a different `/ID`. The current revision still
/// authenticates (AES-256 keys do not depend on `/ID`), but the older revision now carries
/// credentials that differ from the current ones.
fn with_other_id(bytes: Vec<u8>) -> Vec<u8> {
    let doc = Document::open_with_password(Arc::new(bytes.clone()), Some("pw")).expect("open");
    let trailer = doc.trailer();
    let root = trailer.reference(b"Root").expect("root");
    let encrypt = trailer.reference(b"Encrypt").expect("encrypt ref");
    let size = trailer.int(b"Size").expect("size");
    let prev = doc.revisions().last().expect("revision").xref_offset;
    let mut out = bytes;
    let table = out.len();
    out.extend_from_slice(
        format!(
            "xref\n0 1\n0000000000 65535 f \ntrailer << /Size {size} /Root {} {} R /Encrypt {} {} R /ID [<0102030405060708090A0B0C0D0E0F10><0102030405060708090A0B0C0D0E0F10>] /Prev {prev} >>\nstartxref\n{table}\n%%EOF\n",
            root.num, root.generation, encrypt.num, encrypt.generation
        )
        .as_bytes(),
    );
    out
}

/// A signature dictionary under a private catalog key. `write_full` compresses both into an
/// object stream, so no signature name appears in the raw bytes.
fn object_stream_sig_written_by_cos() -> Vec<u8> {
    let mut doc = page_doc();
    let sig = doc.add(sig_dict());
    let root = doc.root().expect("catalog");
    doc.update_dict(root, |d| d.set(b"PrivateNote".to_vec(), Object::Ref(sig))).expect("reachable");
    let bytes = write_full(&doc, &SaveOptions::default()).expect("full");
    assert!(bytes.windows(7).any(|w| w == b"/ObjStm") && !bytes.windows(4).any(|w| w == b"/Sig"), "the signature sits in a compressed object stream");
    let reopened = Document::open(Arc::new(bytes.clone())).expect("reopen");
    let kept = reopened
        .object_numbers()
        .into_iter()
        .any(|n| reopened.get(pdfcraft_cos::ObjRef::new(n, 0)).as_dict().is_some_and(|d| d.name(b"Type") == Some(b"Sig")));
    assert!(kept, "the signature dictionary survives the write");
    bytes
}

#[test]
fn reconstructed_files_refuse_every_signature_shape() {
    let cos_objstm = object_stream_sig_written_by_cos();
    let broken_objstm = break_startxref(&cos_objstm);
    assert!(!broken_objstm.windows(4).any(|w| w == b"/Sig"), "the signature dictionary is compressed out of sight");
    let (freed, freed_sig) = freed_signature(None, write_full(&page_doc(), &SaveOptions::default()).expect("base"));
    let cases = vec![
        Case {
            name: "plain /ByteRange",
            bytes: reconstructed(&[(4, "<< /ByteRange [0 1 2 3] >>")], None),
            password: None,
            sig_num: Some(4),
            allow: false,
        },
        Case {
            name: "escaped /Byte#52ange",
            bytes: reconstructed(&[(4, "<< /Byte#52ange [0 1 2 3] >>")], None),
            password: None,
            sig_num: Some(4),
            allow: false,
        },
        Case {
            name: "escaped /Sub#46ilter",
            bytes: reconstructed(&[(4, "<< /Sub#46ilter /adbe.pkcs7.detached >>")], None),
            password: None,
            sig_num: Some(4),
            allow: false,
        },
        Case { name: "/Type /Sig", bytes: reconstructed(&[(4, "<< /Type /Sig >>")], None), password: None, sig_num: Some(4), allow: false },
        Case {
            name: "/Type, comment and line breaks, /Sig",
            bytes: reconstructed(&[(4, "<< /Type %note\r\n\t /Sig >>")], None),
            password: None,
            sig_num: Some(4),
            allow: false,
        },
        Case {
            name: "/Type/Sig without a space",
            bytes: reconstructed(&[(4, "<</Type/Sig>>")], None),
            password: None,
            sig_num: Some(4),
            allow: false,
        },
        Case { name: "unreferenced signature dictionary", bytes: reconstructed(&[(9, SIG)], None), password: None, sig_num: Some(9), allow: false },
        Case {
            name: "older definition hidden by a later one",
            bytes: reconstructed(&[(5, SIG), (5, "<< /Lang (en) >>")], None),
            password: None,
            sig_num: Some(5),
            allow: false,
        },
        Case {
            name: "old incremental signature, xref broken",
            bytes: break_startxref(&freed),
            password: None,
            sig_num: Some(freed_sig),
            allow: false,
        },
        Case {
            name: "broken signature reference",
            bytes: reconstructed(
                &[(5, "<< /FT /Sig /T (s) /V 99 0 R >>")],
                Some("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] >> >>"),
            ),
            password: None,
            sig_num: Some(5),
            allow: false,
        },
        Case {
            name: "/Perms /DocMDP",
            bytes: reconstructed(&[(6, "<< /Lang (x) >>")], Some("<< /Type /Catalog /Pages 2 0 R /Perms << /DocMDP 6 0 R >> >>")),
            password: None,
            sig_num: None,
            allow: false,
        },
        Case {
            name: "/Perms alone",
            bytes: reconstructed(&[], Some("<< /Type /Catalog /Pages 2 0 R /Perms << >> >>")),
            password: None,
            sig_num: None,
            allow: false,
        },
        Case {
            name: "uncompressed object stream with a signature",
            bytes: reconstructed(&[(7, "<< /Type /ObjStm /N 1 /First 4 /Length 20 >>\nstream\n8 0 << /Type /Sig >>\nendstream")], None),
            password: None,
            sig_num: Some(8),
            allow: false,
        },
        Case { name: "compressed object stream written by cos, xref broken", bytes: broken_objstm, password: None, sig_num: None, allow: false },
        Case {
            name: "undeterminable: field tree points at nothing",
            bytes: reconstructed(&[], Some("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [50 0 R] >> >>")),
            password: None,
            sig_num: Some(50),
            allow: false,
        },
        Case {
            name: "false positive: /Sig inside a string",
            bytes: reconstructed(&[(4, "<< /Title (see /Sig) >>")], None),
            password: None,
            sig_num: None,
            allow: false,
        },
        Case {
            name: "false positive: any # escape",
            bytes: reconstructed(&[(4, "<< /A#42 1 >>")], None),
            password: None,
            sig_num: None,
            allow: false,
        },
        // Without a cross-reference table another reader may pick other objects than cos does,
        // so even a file with no signature name is not treated as unsigned.
        Case { name: "simple unsigned file without xref", bytes: reconstructed(&[], None), password: None, sig_num: None, allow: false },
        Case {
            name: "unsigned file without xref, outlines and a link",
            bytes: reconstructed(
                &[
                    (4, "<< /Type /Outlines /Count 0 >>"),
                    (5, "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /A << /S /URI /URI (https://example.invalid) >> >>"),
                ],
                Some("<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>"),
            ),
            password: None,
            sig_num: None,
            allow: false,
        },
    ];
    run(cases, "reconstructed.txt");
}

#[test]
fn files_with_a_readable_xref_refuse_signature_shapes_and_rotate_otherwise() {
    let (freed, freed_sig) = freed_signature(None, write_full(&page_doc(), &SaveOptions::default()).expect("base"));
    let objstm = object_stream_sig_written_by_cos();
    let cases = vec![
        Case { name: "unsigned, classic xref", bytes: classic(&[], None), password: None, sig_num: None, allow: true },
        Case {
            name: "unsigned, written by cos",
            bytes: write_full(&page_doc(), &SaveOptions::default()).expect("full"),
            password: None,
            sig_num: None,
            allow: true,
        },
        Case { name: "plain /ByteRange", bytes: classic(&[(4, "<< /ByteRange [0 1 2 3] >>")], None), password: None, sig_num: Some(4), allow: false },
        Case {
            name: "escaped /Byte#52ange",
            bytes: classic(&[(4, "<< /Byte#52ange [0 1 2 3] >>")], None),
            password: None,
            sig_num: Some(4),
            allow: false,
        },
        Case {
            name: "escaped /Sub#46ilter",
            bytes: classic(&[(4, "<< /Sub#46ilter /adbe.pkcs7.detached >>")], None),
            password: None,
            sig_num: Some(4),
            allow: false,
        },
        Case { name: "/Type, comment, /Sig", bytes: classic(&[(4, "<< /Type %c\n /Sig >>")], None), password: None, sig_num: Some(4), allow: false },
        Case { name: "unreferenced signature dictionary", bytes: classic(&[(9, SIG)], None), password: None, sig_num: Some(9), allow: false },
        Case { name: "signature freed in a later revision", bytes: freed, password: None, sig_num: Some(freed_sig), allow: false },
        Case {
            name: "broken signature reference",
            bytes: classic(&[(5, "<< /FT /Sig /T (s) /V 99 0 R >>")], Some("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] >> >>")),
            password: None,
            sig_num: Some(5),
            allow: false,
        },
        Case {
            name: "/Perms /DocMDP",
            bytes: classic(&[(6, "<< /Lang (x) >>")], Some("<< /Type /Catalog /Pages 2 0 R /Perms << /DocMDP 6 0 R >> >>")),
            password: None,
            sig_num: None,
            allow: false,
        },
        Case { name: "signature in a compressed object stream", bytes: objstm, password: None, sig_num: None, allow: false },
        Case {
            name: "undeterminable: field tree points at nothing",
            bytes: classic(&[], Some("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [50 0 R] >> >>")),
            password: None,
            sig_num: Some(50),
            allow: false,
        },
    ];
    run(cases, "classic.txt");
}

#[test]
fn encrypted_files_read_their_own_older_revisions_and_refuse_foreign_ones() {
    let (freed, freed_sig) = freed_signature(Some("pw"), encrypted());
    let cases = vec![
        Case { name: "encrypted, one revision", bytes: encrypted(), password: Some("pw"), sig_num: None, allow: true },
        Case { name: "encrypted, three revisions", bytes: encrypted_revisions(2), password: Some("pw"), sig_num: None, allow: true },
        Case { name: "encrypted, signature freed in a later revision", bytes: freed, password: Some("pw"), sig_num: Some(freed_sig), allow: false },
        Case {
            name: "encrypted, older revision with other credentials",
            bytes: with_other_id(encrypted()),
            password: Some("pw"),
            sig_num: None,
            allow: false,
        },
    ];
    run(cases, "encrypted.txt");
    let other = with_other_id(encrypted());
    let current = Document::open_with_password(Arc::new(other.clone()), Some("pw")).expect("current revision authenticates");
    let ends = current.revision_ends();
    assert_eq!(ends.len(), 2);
    let older = Arc::new(other[..ends[0]].to_vec());
    assert!(current.open_with_same_credentials(older.clone()).is_err(), "the refusal comes from the older revision's credentials");
    assert!(Document::open_with_password(older, Some("pw")).is_ok(), "the older revision itself is a readable file");
    assert!(rotate(&encrypted(), Some("wrong")).is_err_and(|e| e.starts_with("open:")), "a wrong password does not open");
    assert!(rotate(&encrypted(), None).is_err_and(|e| e.starts_with("open:")), "no password does not open");
}

/// Writes `N 0 obj` bodies in order and returns the bytes with each object's offset.
fn objects(header: &[u8], bodies: &[(u32, &[u8])]) -> (Vec<u8>, Vec<(u32, usize)>) {
    let mut out = header.to_vec();
    let mut offsets = Vec::new();
    for (num, body) in bodies {
        offsets.push((*num, out.len()));
        out.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    (out, offsets)
}

fn offset_of(offsets: &[(u32, usize)], num: u32) -> usize {
    offsets.iter().find(|(n, _)| *n == num).map(|(_, off)| *off).expect("object written")
}

/// An object stream holding `inner` (object number and body) and a cross-reference stream that
/// places those numbers in it.
fn object_stream_parts(inner: &[(u32, &[u8])]) -> (Vec<u8>, Vec<u8>) {
    let mut header = String::new();
    let mut data = Vec::new();
    for (num, body) in inner {
        header.push_str(&format!("{num} {} ", data.len()));
        data.extend_from_slice(body);
        data.push(b' ');
    }
    let mut content = header.clone().into_bytes();
    content.extend_from_slice(&data);
    let mut stm = format!("<< /Type /ObjStm /N {} /First {} /Length {} >>\nstream\n", inner.len(), header.len(), content.len()).into_bytes();
    stm.extend_from_slice(&content);
    stm.extend_from_slice(b"\nendstream");
    let mut index = String::new();
    let mut entries = Vec::new();
    for (i, (num, _)) in inner.iter().enumerate() {
        index.push_str(&format!("{num} 1 "));
        entries.extend_from_slice(&[2u8, 0, 4, i as u8]);
    }
    let mut xs = format!("<< /Type /XRef /Size 8 /W [1 2 1] /Index [{}] /Length {} >>\nstream\n", index.trim_end(), entries.len()).into_bytes();
    xs.extend_from_slice(&entries);
    xs.extend_from_slice(b"\nendstream");
    (stm, xs)
}

const FIELD_SIG: &[u8] = b"<< /T (f) /FT /Sig /V 7 0 R >>";
const SIG_DICT: &[u8] = b"<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange [0 10 20 30] /Contents <00> >>";

/// A hybrid file (ISO 32000-2 §7.5.8.4) with object stream 4 and cross-reference stream 5.
/// With `conflict`, the table places objects 3 and 7 at a text field and a harmless dictionary
/// while the /XRefStm places the same numbers at a signed signature field and its signature:
/// cos keeps the table and records the disagreement, another reader may follow the stream.
/// Without it, the table leaves object 3 free and only the stream locates it, which is the
/// well-formed use of a hybrid file. `readable = false` points /XRefStm at a byte that holds
/// no cross-reference stream.
fn hybrid(conflict: bool, readable: bool) -> Vec<u8> {
    let inner: Vec<(u32, &[u8])> = if conflict { vec![(3, FIELD_SIG), (7, SIG_DICT)] } else { vec![(3, b"<< /Type /Outlines /Count 0 >>")] };
    let (stm, xs) = object_stream_parts(&inner);
    let catalog: &[u8] = if conflict {
        b"<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [3 0 R] >> >>"
    } else {
        b"<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R >>"
    };
    let mut bodies: Vec<(u32, &[u8])> = vec![(1, catalog), (2, b"<< /Type /Pages /Kids [6 0 R] /Count 1 /MediaBox [0 0 300 400] >>")];
    if conflict {
        bodies.push((3, b"<< /T (f) /FT /Tx >>"));
        bodies.push((7, b"<< /Lang (x) >>"));
    }
    bodies.push((6, b"<< /Type /Page /Parent 2 0 R >>"));
    bodies.push((4, &stm));
    bodies.push((5, &xs));
    let (mut out, offsets) = objects(b"%PDF-1.5\n", &bodies);
    let table = out.len();
    out.extend_from_slice(b"xref\n0 8\n0000000000 65535 f \n");
    for num in 1..8u32 {
        match offsets.iter().find(|(n, _)| *n == num) {
            Some((_, off)) => out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes()),
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    let xrefstm = if readable { offset_of(&offsets, 5) } else { 5 };
    out.extend_from_slice(format!("trailer << /Size 8 /Root 1 0 R /XRefStm {xrefstm} >>\nstartxref\n{table}\n%%EOF\n").as_bytes());
    out
}

/// One classic section that lists objects 3 and 7 twice: first at harmless definitions, then at
/// a signed signature field and its signature dictionary.
fn duplicate_entries() -> Vec<u8> {
    let bodies: Vec<(u32, &[u8])> = vec![
        (1, b"<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [3 0 R] >> >>"),
        (2, b"<< /Type /Pages /Kids [6 0 R] /Count 1 /MediaBox [0 0 300 400] >>"),
        (3, b"<< /T (f) /FT /Tx >>"),
        (7, b"<< /Lang (x) >>"),
        (6, b"<< /Type /Page /Parent 2 0 R >>"),
        (3, FIELD_SIG),
        (7, SIG_DICT),
    ];
    let (mut out, offsets) = objects(b"%PDF-1.7\n", &bodies);
    let entry = |i: usize| format!("{:010} 00000 n \n", offsets[i].1);
    let table = out.len();
    let mut t = String::from("xref\n0 4\n0000000000 65535 f \n");
    t += &entry(0);
    t += &entry(1);
    t += &entry(2);
    t += "6 2\n";
    t += &entry(4);
    t += &entry(3);
    t += "3 1\n";
    t += &entry(5);
    t += "7 1\n";
    t += &entry(6);
    t += &format!("trailer << /Size 8 /Root 1 0 R >>\nstartxref\n{table}\n%%EOF\n");
    out.extend_from_slice(t.as_bytes());
    out
}

/// A classic file whose trailer /Prev points back at its own section.
fn looping_chain() -> Vec<u8> {
    let bytes = classic(&[], None);
    let text = String::from_utf8(bytes).expect("ascii");
    let table = text.rfind("xref\n0 ").expect("table");
    text.replacen("trailer << /Size 4 /Root 1 0 R >>", &format!("trailer << /Size 4 /Root 1 0 R /Prev {table} >>"), 1).into_bytes()
}

fn diagnosed(bytes: &[u8]) -> (usize, usize, usize) {
    let doc = Document::open(Arc::new(bytes.to_vec())).expect("open");
    (doc.revisions().len(), doc.repair_log().len(), doc.xref_oddities().len())
}

#[test]
fn ambiguous_or_repaired_cross_references_refuse_rotation() {
    let conflict = hybrid(true, true);
    let unreadable = hybrid(true, false);
    let duplicate = duplicate_entries();
    let looping = looping_chain();
    // What cos reports for each, so a later reader change cannot quietly empty these cases.
    let (revisions, repairs, odd) = diagnosed(&conflict);
    assert!(revisions == 1 && repairs == 0 && odd == 2, "hybrid conflict: {revisions} {repairs} {odd}");
    let (revisions, repairs, odd) = diagnosed(&unreadable);
    assert!(revisions == 1 && repairs == 1 && odd == 0, "unreadable /XRefStm: {revisions} {repairs} {odd}");
    let (revisions, repairs, odd) = diagnosed(&duplicate);
    assert!(revisions == 1 && repairs == 0 && odd == 2, "duplicate entries: {revisions} {repairs} {odd}");
    let (revisions, repairs, _) = diagnosed(&looping);
    assert!(revisions >= 1 && repairs >= 1, "looping chain: {revisions} {repairs}");
    for bytes in [&conflict, &unreadable, &duplicate] {
        let doc = Document::open(Arc::new(bytes.clone())).expect("open");
        assert_eq!(doc.get(pdfcraft_cos::ObjRef::new(3, 0)).as_dict().and_then(|d| d.name(b"FT")), Some(b"Tx".as_slice()), "cos sees the text field");
    }
    let cases = vec![
        Case { name: "hybrid table and /XRefStm disagree", bytes: conflict, password: None, sig_num: Some(3), allow: false },
        Case { name: "hybrid /XRefStm unreadable and repaired", bytes: unreadable, password: None, sig_num: Some(3), allow: false },
        Case { name: "one section lists objects twice", bytes: duplicate, password: None, sig_num: Some(3), allow: false },
        Case { name: "cross-reference chain loops", bytes: looping, password: None, sig_num: None, allow: false },
        Case { name: "no cross-reference table", bytes: reconstructed(&[], None), password: None, sig_num: None, allow: false },
        Case { name: "readable xref with a signature dictionary", bytes: classic(&[(9, SIG)], None), password: None, sig_num: Some(9), allow: false },
        Case {
            name: "older revision keeps a signature dictionary",
            bytes: freed_signature(None, write_full(&page_doc(), &SaveOptions::default()).expect("base")).0,
            password: None,
            sig_num: None,
            allow: false,
        },
    ];
    run(cases, "xref-ambiguous.txt");
}

#[test]
fn well_formed_cross_references_still_rotate() {
    let stream_only = write_full(&page_doc(), &SaveOptions::default()).expect("full");
    assert!(
        stream_only.windows(5).any(|w| w == b"/XRef") && !stream_only.windows(6).any(|w| w == b"\nxref\n"),
        "cos writes a cross-reference stream"
    );
    let normal_hybrid = hybrid(false, true);
    let (revisions, repairs, odd) = diagnosed(&normal_hybrid);
    assert!(revisions == 1 && repairs == 0 && odd == 0, "well-formed hybrid: {revisions} {repairs} {odd}");
    let hybrid_doc = Document::open(Arc::new(normal_hybrid.clone())).expect("open");
    assert_eq!(
        hybrid_doc.get(pdfcraft_cos::ObjRef::new(3, 0)).as_dict().and_then(|d| d.name(b"Type")),
        Some(b"Outlines".as_slice()),
        "only the /XRefStm locates object 3"
    );
    let mut incremental = stream_only.clone();
    for n in 0..2 {
        let mut doc = Document::open(Arc::new(incremental)).expect("open");
        let root = doc.root().expect("catalog");
        doc.update_dict(root, |d| d.set(b"Lang".to_vec(), Object::String(PdfString::literal(format!("r{n}"))))).expect("lang");
        incremental = write_incremental(&doc, &SaveOptions::default()).expect("incremental");
    }
    assert_eq!(diagnosed(&incremental), (3, 0, 0));
    let cases = vec![
        Case { name: "classic table", bytes: classic(&[], None), password: None, sig_num: None, allow: true },
        Case { name: "cross-reference stream", bytes: stream_only, password: None, sig_num: None, allow: true },
        Case { name: "well-formed hybrid", bytes: normal_hybrid, password: None, sig_num: None, allow: true },
        Case { name: "two incremental updates", bytes: incremental, password: None, sig_num: None, allow: true },
        Case { name: "encrypted, three revisions", bytes: encrypted_revisions(2), password: Some("pw"), sig_num: None, allow: true },
    ];
    run(cases, "xref-well-formed.txt");
}
