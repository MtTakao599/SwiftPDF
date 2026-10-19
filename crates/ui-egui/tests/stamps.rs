//! Add a stamp: the palette and placing stamps on the page.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{PdfCraftApp, QuickTool};

#[test]
fn choosing_and_placing_stamps() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app.set_option("zoom", "150").unwrap();
        app
    });
    h.run_steps(6);
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(2);
    h.get_by_label("Add a stamp");
    h.get_all_by_label("APPROVED").next().expect("dynamic approved").click();
    h.run_steps(2);
    assert_eq!(h.state().quick_tool, QuickTool::Stamp(pdfcraft_engine::StampKind::DynApproved));
    let p = {
        let r = h.state().views[0].page_screen_rect(0).unwrap();
        r.min + egui::vec2(r.width() * 0.5, r.height() * 0.85)
    };
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    let stamps: Vec<_> = doc.info.annotations.iter().filter(|a| a.subtype == "Stamp").collect();
    assert_eq!(stamps.len(), 1);
    assert_eq!(s.quick_tool, QuickTool::Select, "back to selecting after one stamp");
    for _ in 0..30 {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if let Ok(dir) = std::env::var("PDFCRAFT_SHOTS") {
        h.render().unwrap().save(format!("{dir}/stamps.png")).unwrap();
    }
}

#[test]
fn natural_image_stamps_keep_displayed_bounds_on_rotated_pages() {
    for degrees in [0, 90, 180, 270] {
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
            let id = app.views[0].id;
            app.session.apply(id, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees }).unwrap();
            app.set_option("zoom", "100").unwrap();
            app
        });
        h.run_steps(6);
        assert!(h.state_mut().execute("comment.stamp"));
        let mut png = Vec::new();
        image::RgbImage::from_pixel(80, 40, image::Rgb([20, 120, 220]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        h.state_mut().start_custom_stamp("rectangle.png".into(), png);
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(2);
        let page = h.state().views[0].page_screen_rect(0).unwrap();
        let at = page.min + egui::vec2(page.width() * 0.5, page.height() * 0.6);
        h.hover_at(at);
        h.run_steps(1);
        h.drag_at(at);
        h.run_steps(1);
        h.drop_at(at);
        h.run_steps(4);
        let app = h.state();
        let doc = app.session.get(app.views[0].id).unwrap();
        assert_eq!(app.quick_tool, QuickTool::Select);
        let stamp = doc.info.annotations.iter().find(|a| a.subtype == "Stamp").unwrap();
        let info = &doc.info.pages[0];
        let a = info.user_to_view(stamp.rect[0], stamp.rect[1]);
        let b = info.user_to_view(stamp.rect[2], stamp.rect[3]);
        assert!(((a[0] - b[0]).abs() - 80.0).abs() < 0.001, "rotation {degrees}: {a:?}, {b:?}");
        assert!(((a[1] - b[1]).abs() - 40.0).abs() < 0.001, "rotation {degrees}: {a:?}, {b:?}");
        assert_eq!(doc.can_undo(), Some("Add stamp"));
    }
}

#[test]
fn creating_placing_and_keeping_custom_stamps() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app.set_option("zoom", "150").unwrap();
        app
    });
    h.run_steps(6);
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(2);
    // An image becomes a stamp: Create ▸ category and name.
    let mut png = Vec::new();
    image::RgbImage::from_pixel(60, 20, image::Rgb([20, 120, 220])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    h.state_mut().start_custom_stamp("logo.png".into(), png);
    h.run_steps(2);
    h.get_by_label("Create Custom Stamp");
    h.state_mut().stamp_draft.category = "Company".into();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert_eq!(h.state().quick_tool, QuickTool::CustomStamp(0));
    h.get_by_label("Company");
    h.get_by_label("logo");
    // Place it.
    let p = {
        let r = h.state().views[0].page_screen_rect(0).unwrap();
        r.min + egui::vec2(r.width() * 0.5, r.height() * 0.85)
    };
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    let stamps: Vec<_> = doc.info.annotations.iter().filter(|a| a.subtype == "Stamp").collect();
    assert_eq!(stamps.len(), 1);
    let r = stamps[0].rect;
    assert!(((r[2] - r[0]) / (r[3] - r[1]) - 3.0).abs() < 0.05, "keeps the picture's shape: {r:?}");
    assert_eq!(doc.can_undo(), Some("Add stamp"));
    // The library is kept with the app's settings.
    let saved = s.persist();
    let mut again = PdfCraftApp::new();
    again.set_option("language", "en").unwrap();
    again.restore(&saved);
    assert_eq!(again.custom_stamps.len(), 1);
    assert_eq!((again.custom_stamps[0].category.as_str(), again.custom_stamps[0].name.as_str()), ("Company", "logo"));
    assert_eq!(again.custom_stamps[0].data, s.custom_stamps[0].data);
}

fn png(alpha: u8) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(12, 8, image::Rgba([20, 80, 160, alpha]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

fn click_at(h: &mut Harness<'static, PdfCraftApp>, p: egui::Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(3);
}

fn page_point(h: &Harness<'static, PdfCraftApp>) -> egui::Pos2 {
    let r = h.state().views[0].page_screen_rect(0).expect("page on screen");
    r.min + egui::vec2(r.width() * 0.42, r.height() * 0.28)
}

fn register(h: &mut Harness<'static, PdfCraftApp>, file: &str, alpha: u8) -> Vec<u8> {
    let bytes = png(alpha);
    h.state_mut().start_custom_stamp(file.into(), bytes.clone());
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    bytes
}

/// View-only: clicking a registered image stamp selects that stamp, shows its name, and a page
/// click only previews it. Cancel leaves the document alone. Confirm places that stamp.
#[test]
fn a_registered_stamp_click_enters_placement_until_cancel_or_confirm() {
    let policy = (
        pdfcraft_ui_egui::output_policy::blocked(),
        pdfcraft_ui_egui::output_policy::save_as_allowed(),
        pdfcraft_ui_egui::output_policy::print_allowed(),
    );
    let restore = PolicyRestore(policy);
    for mode in [pdfcraft_ui_egui::UiMode::Simple, pdfcraft_ui_egui::UiMode::Detailed] {
        let dir = std::env::temp_dir().join(format!("swiftpdf-stamp-place-{}-{}", std::process::id(), mode.as_str()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join("opened.pdf");
        let bytes = include_bytes!("data/form.pdf").to_vec();
        std::fs::write(&original, &bytes).unwrap();
        let hash = sha256(&bytes);
        let original_s = original.to_string_lossy().into_owned();
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.set_view_only(true);
            app.grant_stamp();
            app.set_ui_mode(mode);
            app.open_bytes("opened.pdf", Some(original_s), bytes).unwrap();
            app.set_option("zoom", "100").unwrap();
            app
        });
        h.run_steps(8);
        assert!(h.state().session.view_only(), "{mode:?}");
        assert!(h.state_mut().execute("comment.stamp"), "{mode:?}: stamp palette");
        h.run_steps(3);
        let red = register(&mut h, "red.png", 128);
        let blue = register(&mut h, "blue.png", 200);
        h.get_by_label("red").click();
        h.run_steps(3);
        assert_eq!(h.state().quick_tool, QuickTool::CustomStamp(0), "{mode:?}: the clicked stamp stays selected");
        assert_eq!(h.state().custom_stamps[0].data.as_slice(), red.as_slice());
        h.get_by_label("配置中: red");
        let before = stamp_count(&h);
        let at = page_point(&h);
        click_at(&mut h, at);
        h.get_by_label("配置中: red");
        let place = h.state().views[0].stamp_place.clone().expect("a page click previews the stamp");
        assert_eq!(place.name, "red");
        assert_eq!(place.data.as_slice(), red.as_slice(), "the preview keeps the registered image");
        let alpha = place.pixels.as_ref().and_then(|(_, _, px)| px.get(3)).copied();
        assert_eq!(alpha, Some(128), "the transparent pixel is kept");
        assert_eq!(stamp_count(&h), before, "a preview does not change the document");
        assert_eq!(h.state().quick_tool, QuickTool::CustomStamp(0), "{mode:?}: placement stays until cancel");
        h.get_by_label("キャンセル").click();
        h.run_steps(2);
        assert!(h.state().views[0].stamp_place.is_none());
        assert_eq!(h.state().quick_tool, QuickTool::Select);
        assert_eq!(stamp_count(&h), before);
        h.get_by_label("blue").click();
        h.run_steps(2);
        assert_eq!(h.state().quick_tool, QuickTool::CustomStamp(1), "{mode:?}: the second stamp is the one selected");
        h.get_by_label("配置中: blue");
        let at = page_point(&h);
        click_at(&mut h, at);
        let place = h.state().views[0].stamp_place.clone().expect("blue preview");
        assert_eq!(place.data.as_slice(), blue.as_slice());
        h.get_by_label("確定").click();
        h.run_steps(3);
        assert!(h.state().views[0].stamp_place.is_none());
        assert_eq!(h.state().quick_tool, QuickTool::Select, "confirm ends placement");
        assert_eq!(stamp_count(&h), before + 1);
        let app = h.state();
        let doc = app.session.get(app.views[0].id).unwrap();
        assert_eq!(doc.can_undo(), Some("Add stamp"));
        assert!(!app.command_allowed("file.save"));
        assert!(!app.command_allowed("print.dialog"), "stamps do not grant printing");
        assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
        if mode == pdfcraft_ui_egui::UiMode::Simple {
            h.state_mut().grant_save_as();
            let dest = dir.join("copy.pdf");
            h.state_mut().save_override = Some(dest.to_string_lossy().into_owned());
            assert!(h.state_mut().execute("file.save_as"));
            h.run_steps(2);
            assert!(h.state_mut().confirm_save_copy());
            h.run_steps(2);
            assert_eq!(sha256(&std::fs::read(&original).unwrap()), hash);
            assert!(dest.is_file());
            h.state_mut().grant_print();
            assert!(h.state_mut().execute("print.dialog"));
            h.run_steps(2);
            assert!(matches!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Print)));
            h.state_mut().dialog = None;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    drop(restore);
}

fn stamp_count(h: &Harness<'static, PdfCraftApp>) -> usize {
    let app = h.state();
    app.session.get(app.views[0].id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count()
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

struct PolicyRestore((bool, bool, bool));

impl Drop for PolicyRestore {
    fn drop(&mut self) {
        pdfcraft_ui_egui::output_policy::set_blocked(self.0.0);
        pdfcraft_ui_egui::output_policy::set_save_as_allowed(self.0.1);
        pdfcraft_ui_egui::output_policy::set_print_allowed(self.0.2);
    }
}
