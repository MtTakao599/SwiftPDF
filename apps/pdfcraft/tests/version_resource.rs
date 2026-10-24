//! `SwiftPDF.exe` carries a Windows version resource (`build.rs`), so Explorer, PowerShell's
//! `VersionInfo` and inventory tools see the SwiftPDF product name and the PdfCraft copyright.

#![cfg(windows)]

/// `s` as UTF-16LE, the encoding of every string in a version resource.
fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The value of string-table entry `key`: the key and its terminator, zero padding to the next
/// 32-bit boundary, then the value and its terminator.
fn string_value(resource: &[u8], key: &str) -> Option<String> {
    let after_key = find(resource, &utf16(&format!("{key}\0")))? + utf16(key).len() + 2;
    let start = after_key + resource[after_key..].iter().take_while(|&&b| b == 0).count();
    let units: Vec<u16> = resource[start..].as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).take_while(|&u| u != 0).collect();
    String::from_utf16(&units).ok()
}

#[test]
fn the_exe_names_swiftpdf_and_keeps_the_pdfcraft_copyright() {
    let exe = std::fs::read(env!("CARGO_BIN_EXE_SwiftPDF")).unwrap();
    let start = find(&exe, &utf16("VS_VERSION_INFO")).expect("no version resource");
    let resource = &exe[start..];
    for (key, value) in [
        ("ProductName", "SwiftPDF"),
        ("FileDescription", "SwiftPDF"),
        ("FileVersion", env!("CARGO_PKG_VERSION")),
        ("ProductVersion", env!("CARGO_PKG_VERSION")),
        ("LegalCopyright", "Copyright (c) the PdfCraft contributors. MIT OR Apache-2.0."),
        ("OriginalFilename", "SwiftPDF.exe"),
        ("InternalName", "SwiftPDF"),
    ] {
        assert_eq!(string_value(resource, key).as_deref(), Some(value), "{key}");
    }
}
