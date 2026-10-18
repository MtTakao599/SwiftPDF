//! Windows only: embed the SwiftPDF icon and version info (VERSIONINFO) into `SwiftPDF.exe`, so
//! it shows in Explorer, the taskbar, the Start menu and Alt-Tab. Resource 1 is the application
//! icon. Resource 2 is the same artwork, so a PDF association (`-2`) does not use the PdfCraft
//! document icon.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `PDFCRAFT_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

#[path = "src/windows_manifest.rs"]
mod windows_manifest;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/swiftpdf.ico");
    println!("cargo:rerun-if-changed=../../docs/images/SwiftPDFicon.png");
    println!("cargo:rerun-if-changed=src/windows_manifest.rs");
    println!("cargo:rerun-if-env-changed=PDFCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/swiftpdf.ico")
        .set_icon_with_id("../../assets/app-icon/swiftpdf.ico", "2")
        .set_manifest(windows_manifest::WINDOWS_MANIFEST)
        .set("ProductName", "SwiftPDF")
        .set("FileDescription", "SwiftPDF")
        .set("LegalCopyright", "Copyright (c) the PdfCraft contributors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "SwiftPDF.exe")
        .set("InternalName", "SwiftPDF");
    if let Err(e) = res.compile() {
        if std::env::var_os("PDFCRAFT_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=SwiftPDF.exe built without icon/version resources: {e}");
    }
}
