//! SwiftPDF's detailed interface while view-only, and the zoom menu both interfaces share.
//!
//! Editing workspaces, the tool catalogue, the recommended tools and the community card are not
//! shown. Pages, bookmarks, search, properties and signatures stay. The percent on the rail opens
//! presets and a number, and neither changes the PDF.
//!
//! [`PdfCraftApp::set_view_only`] also sets the process-wide output policy, so these tests take
//! [`LOCK`] and run one at a time.

use std::sync::{Mutex, MutexGuard, PoisonError};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::canvas::Fit;
use pdfcraft_ui_egui::theme::ThemePreference;
use pdfcraft_ui_egui::{Mode, PdfCraftApp, UiMode, ZOOM_ENTRY_INVALID, ZOOM_PRESETS};

static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn pdf() -> Vec<u8> {
    let body = "BT /F1 24 Tf 20 150 Td (Hello zoom) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 /MediaBox [0 0 300 300] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 6 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 6 0 R >> >> >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn harness(mode: UiMode, tall: bool, open: bool) -> Harness<'static, PdfCraftApp> {
    let size = if tall { egui::vec2(1100.0, 1600.0) } else { egui::vec2(1400.0, 900.0) };
    let mut h = Harness::builder().with_size(size).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.run_inline = true;
        app.set_view_only(true);
        app.set_ui_mode(mode);
        if open {
            app.open_bytes("a.pdf", None, pdf()).unwrap();
            app.open_bytes("b.pdf", None, pdf()).unwrap();
        }
        app
    });
    h.run_steps(4);
    h
}

fn zoom_of(h: &Harness<'static, PdfCraftApp>, tab: usize) -> f32 {
    h.state().views[tab].zoom
}

fn open_zoom(h: &mut Harness<'static, PdfCraftApp>) {
    h.get_by(|n| n.label().is_some_and(|l| l.starts_with("Zoom ") && l.ends_with('%'))).click();
    h.run_steps(3);
}

fn type_zoom(h: &mut Harness<'static, PdfCraftApp>, text: &str) {
    open_zoom(h);
    if !text.is_empty() {
        h.get_by(|n| n.role() == Role::TextInput && n.label().as_deref() == Some("Zoom")).type_text(text);
        h.run_steps(2);
    }
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
}

#[test]
fn a_view_only_detailed_window_keeps_reading_and_hides_editing() {
    let _serial = serial();
    let mut h = harness(UiMode::Detailed, false, true);
    assert_eq!(h.state().mode, Mode::AllTools);
    for gone in ["Edit", "Convert", "E-Sign", "Organize pages", "Fill & Sign", "Join the ArtCraft community", "Recommended tools"] {
        assert!(h.query_by_label(gone).is_none(), "{gone} is not shown");
    }
    for kept in ["All tools", "Read", "Page thumbnails", "Bookmarks", "Signatures", "Select (V)", "Hand (H)"] {
        assert!(h.query_by_label(kept).is_some(), "{kept} stays");
    }
    assert!(h.query_by(|n| n.label().is_some_and(|l| l.starts_with("Document properties"))).is_some(), "properties stay");
    assert!(h.state_mut().execute("edit.find"));
    h.run_steps(2);
    assert!(h.state().views[1].find.is_some(), "search still opens");
    let doc = h.state().session.get(h.state().views[1].id).unwrap();
    assert!(!doc.dirty, "finding did not change the PDF");

    // Home, still detailed: no community card and no recommended tools, and Open file stays.
    h.get_by_label("Home").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("ArtCraft community").is_none());
    assert!(h.query_by_label_contains("Recommended tools").is_none());
    assert!(h.query_by_label_contains("Welcome to SwiftPDF").is_some(), "the detailed home names SwiftPDF");
    assert!(h.query_by_label_contains("Welcome to PdfCraft").is_none(), "the detailed home does not name PdfCraft");
    assert!(h.query_by_label("Open file").is_some());
    assert_eq!(h.state().views.len(), 2, "Home closes nothing");
}

#[test]
fn the_detailed_home_names_swiftpdf_in_japanese() {
    let _serial = serial();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "ja").unwrap();
        app.run_inline = true;
        app.set_view_only(true);
        app.set_ui_mode(UiMode::Detailed);
        app
    });
    h.run_steps(4);
    assert!(h.query_by_label_contains("SwiftPDF へようこそ").is_some(), "Japanese detailed home names SwiftPDF");
    assert!(h.query_by_label_contains("PdfCraft へようこそ").is_none());
}

#[test]
fn preferences_cannot_offer_an_editing_workspace() {
    let _serial = serial();
    let mut h = harness(UiMode::Detailed, true, false);
    assert!(h.state_mut().execute("app.preferences"));
    h.run_steps(4);
    assert!(h.query_by_label("Default workspace mode").is_some(), "the setting is on screen");
    for gone in ["Edit", "Convert", "E-Sign"] {
        assert_eq!(h.query_all_by_label(gone).count(), 0, "{gone} is not a choice");
    }
    assert!(h.query_all_by_label("Read").count() >= 2, "the mode tab and the Read choice");
    assert!(h.query_all_by_label("All tools").count() >= 2, "the mode tab and the All tools choice");
}

#[test]
fn a_saved_editing_workspace_is_read_only_while_view_only() {
    let _serial = serial();
    let mut app = PdfCraftApp::new();
    app.set_view_only(true);
    app.mode = Mode::Sign;
    app.restore(r#"{"default_mode":"edit","theme":"Dark"}"#);
    assert_eq!(app.default_mode, Mode::Read);
    assert_eq!(app.mode, Mode::Read, "the session was already in an editing workspace");
    assert_eq!(app.theme_preference, ThemePreference::Dark, "the other settings are still read");
    for saved in ["convert", "sign", "edit"] {
        app.restore(&format!(r#"{{"default_mode":"{saved}"}}"#));
        assert_eq!(app.default_mode, Mode::Read, "{saved}");
    }
    app.restore(r#"{"default_mode":"all"}"#);
    assert_eq!(app.default_mode, Mode::AllTools, "All tools is a viewing workspace");
    app.set_option("default-mode", "edit").unwrap();
    assert_eq!(app.default_mode, Mode::Read);
    app.set_option("mode", "convert").unwrap();
    assert_eq!(app.mode, Mode::Read);
    app.set_option("default-mode", "read").unwrap();
    assert_eq!(app.default_mode, Mode::Read);

    // Not view-only: the same settings still select Edit.
    let mut full = PdfCraftApp::new();
    full.set_view_only(false);
    full.set_option("default-mode", "edit").unwrap();
    assert_eq!(full.default_mode, Mode::Edit);
    full.restore(r#"{"default_mode":"sign"}"#);
    assert_eq!(full.default_mode, Mode::Sign);
}

#[test]
fn zoom_presets_include_an_exact_hundred_and_do_not_change_the_pdf() {
    let _serial = serial();
    let mut h = harness(UiMode::Simple, false, true);
    let id = h.state().views[1].id;
    let other = (zoom_of(&h, 0), h.state().views[0].fit);
    for percent in ZOOM_PRESETS {
        open_zoom(&mut h);
        h.get_by_label(&format!("{percent}%")).click();
        h.run_steps(3);
        let view = &h.state().views[1];
        let expect = if percent == 100 { 1.0 } else { f32::from(percent) / 100.0 };
        assert_eq!(view.zoom, expect, "{percent}%");
        assert_eq!(view.fit, Fit::None, "{percent}% leaves fit mode");
        assert!(!h.state().session.get(id).unwrap().dirty, "{percent}% does not edit the PDF");
        assert_eq!((zoom_of(&h, 0), h.state().views[0].fit), other, "{percent}% does not touch the other tab");
    }
}

#[test]
fn a_typed_zoom_accepts_a_number_and_refuses_junk() {
    let _serial = serial();
    let mut h = harness(UiMode::Detailed, false, true);
    type_zoom(&mut h, "150");
    assert_eq!(h.state().views[1].zoom, 1.5);
    assert_eq!(h.state().views[1].fit, Fit::None);
    type_zoom(&mut h, "100");
    assert_eq!(h.state().views[1].zoom, 1.0, "typed 100% is exactly 100%");
    type_zoom(&mut h, "12.5");
    assert!((zoom_of(&h, 1) - 0.125).abs() < 1.0e-4);
    // Past either end: the existing clamp, not a new limit.
    type_zoom(&mut h, "1");
    assert_eq!(zoom_of(&h, 1), 0.08);
    type_zoom(&mut h, "9000");
    assert_eq!(zoom_of(&h, 1), 64.0);
    let before = (zoom_of(&h, 1), h.state().views[1].fit);
    for junk in ["abc", "0", "-5", ""] {
        h.state_mut().toast = None;
        type_zoom(&mut h, junk);
        assert_eq!((zoom_of(&h, 1), h.state().views[1].fit), before, "{junk:?} changes nothing");
        assert_eq!(h.state().toast.as_ref().map(|t| t.0.as_str()), Some(ZOOM_ENTRY_INVALID), "{junk:?}");
    }
    assert!(!h.state().session.get(h.state().views[1].id).unwrap().dirty);
}

#[test]
fn each_tab_keeps_its_zoom_and_so_does_switching_interface() {
    let _serial = serial();
    let mut h = harness(UiMode::Simple, false, true);
    open_zoom(&mut h);
    h.get_by_label("200%").click();
    h.run_steps(3);
    assert_eq!(zoom_of(&h, 1), 2.0);
    h.get_by_label("a.pdf").click();
    h.run_steps(3);
    open_zoom(&mut h);
    h.get_by_label("50%").click();
    h.run_steps(3);
    assert_eq!(zoom_of(&h, 0), 0.5);
    assert_eq!(zoom_of(&h, 1), 2.0, "b.pdf is unchanged");
    h.state_mut().views[0].go_to_page(1);
    h.run_steps(3);
    let ids: Vec<_> = h.state().views.iter().map(|v| v.id).collect();
    h.get_by_label("詳細").click();
    h.run_steps(4);
    assert_eq!(h.state().ui_mode(), UiMode::Detailed);
    assert_eq!(h.state().active, Some(0));
    assert_eq!(h.state().views.iter().map(|v| v.id).collect::<Vec<_>>(), ids);
    assert_eq!((zoom_of(&h, 0), h.state().views[0].current, h.state().views[0].fit), (0.5, 1, Fit::None));
    assert_eq!(zoom_of(&h, 1), 2.0);
    h.get_by_label("簡単").click();
    h.run_steps(4);
    assert_eq!((h.state().ui_mode(), h.state().active, zoom_of(&h, 0), zoom_of(&h, 1)), (UiMode::Simple, Some(0), 0.5, 2.0));
}

#[test]
fn fit_width_and_fit_page_still_work_from_the_rail_and_the_menu() {
    let _serial = serial();
    let mut h = harness(UiMode::Simple, false, true);
    open_zoom(&mut h);
    h.get_by_label("100%").click();
    h.run_steps(2);
    assert_eq!((zoom_of(&h, 1), h.state().views[1].fit), (1.0, Fit::None));
    h.get_by_label("Toggle fit page / fit width").click();
    h.run_steps(2);
    assert_eq!(h.state().views[1].fit, Fit::Width, "the rail's fit button");
    open_zoom(&mut h);
    h.get_by_label("Zoom to page level").click();
    h.run_steps(2);
    assert_eq!(h.state().views[1].fit, Fit::Page);
    open_zoom(&mut h);
    h.get_by_label("Fit to width").click();
    h.run_steps(2);
    assert_eq!(h.state().views[1].fit, Fit::Width);
    assert!(!h.state().session.get(h.state().views[1].id).unwrap().dirty);
}

#[test]
fn an_app_that_can_edit_still_shows_the_editing_workspaces() {
    let _serial = serial();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_view_only(false);
        app.set_ui_mode(UiMode::Detailed);
        app
    });
    h.run_steps(4);
    for tab in ["All tools", "Read", "Edit", "Convert", "E-Sign"] {
        assert!(h.query_all_by_label(tab).count() >= 1, "{tab}");
    }
    assert!(h.query_by_label_contains("Recommended tools").is_some());
    assert!(h.query_by_label_contains("ArtCraft community").is_some());
    h.get_by_label("Edit").click();
    h.run_steps(3);
    assert_eq!(h.state().mode, Mode::Edit);
}
