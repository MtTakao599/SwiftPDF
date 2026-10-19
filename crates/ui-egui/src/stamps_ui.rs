//! Custom stamps (Add a stamp ▸ Custom stamps ▸ Create): a PDF page or an image saved in the
//! user's stamp library under a category and a name, then placed like any stamp.

use std::sync::Arc;

use egui::{Align, Layout};
use serde::{Deserialize, Serialize};

use crate::theme::{self, Tokens};
use crate::{Dialog, PdfCraftApp, QuickTool, widgets};

/// Stamp files larger than this aren't kept in the library (it lives in the app's settings).
pub const MAX_STAMP_BYTES: usize = 4 << 20;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomStamp {
    pub category: String,
    pub name: String,
    /// The source file's name (its extension tells images from PDFs).
    pub file: String,
    /// The page of a PDF (0-based).
    #[serde(default)]
    pub page: usize,
    #[serde(with = "b64")]
    pub data: Arc<Vec<u8>>,
}

mod b64 {
    use std::sync::Arc;

    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v.as_slice()))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(s).map(Arc::new).map_err(serde::de::Error::custom)
    }
}

/// The Create Custom Stamp dialog: the chosen file, its category and name.
/// A chairman seal uses the same draft and never enters [`CustomStamp`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StampDraft {
    pub file: String,
    pub data: Arc<Vec<u8>>,
    pub category: String,
    pub name: String,
    pub chairman: bool,
    pub width: u32,
    pub height: u32,
}

pub(crate) fn create_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> (bool, bool) {
    let categories: Vec<String> = {
        let mut c: Vec<String> = app.custom_stamps.iter().map(|s| s.category.clone()).collect();
        c.sort();
        c.dedup();
        c
    };
    let chairman = app.stamp_draft.chairman;
    let d = &mut app.stamp_draft;
    let title = if chairman { "理事長印を登録" } else { tl!("Create Custom Stamp") };
    ui.label(egui::RichText::new(title).font(theme::semibold(18.0)));
    ui.add_space(8.0);
    ui.label(egui::RichText::new(crate::i18n::fmt(tl!("From {file}"), &[("file", &d.file)])).color(t.text_muted));
    let size =
        if d.width > 0 && d.height > 0 { format!("{} × {} px", d.width, d.height) } else { "サイズを読み取れませんでした".into() };
    ui.label(egui::RichText::new(size).color(t.text_muted));
    paint_draft_preview(ui, d);
    if chairman {
        ui.label(egui::RichText::new("透過部分はそのまま保持します。印影はアプリの設定には保存しません。").small().color(t.text_faint));
        ui.label(
            egui::RichText::new(
                "元の画像ファイルや、OS のバックアップに残ったデータには、この暗号化は及びません。同じ Windows ユーザーはこの PC で復号できます。管理者によるアクセスを完全に防ぐものではありません。",
            )
            .small()
            .color(t.text_faint),
        );
    }
    ui.add_space(8.0);
    egui::Grid::new("stamp-create").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        if !chairman {
            let l = ui.label(tl!("Category:"));
            ui.horizontal(|ui| {
                ui.add(crate::widgets::line(&mut d.category).desired_width(200.0).hint_text(tl!("e.g. My stamps"))).labelled_by(l.id);
                if !categories.is_empty() {
                    egui::ComboBox::from_id_salt("stamp-categories").selected_text("").width(24.0).show_ui(ui, |ui| {
                        for c in &categories {
                            if ui.selectable_label(d.category == *c, c).clicked() {
                                d.category = c.clone();
                            }
                        }
                    });
                }
            });
            ui.end_row();
        }
        let l = ui.label(if chairman { "名前" } else { tl!("Name:") });
        ui.add(crate::widgets::line(&mut d.name).desired_width(200.0)).labelled_by(l.id);
        ui.end_row();
    });
    ui.add_space(12.0);
    let ok = !d.name.trim().is_empty()
        && (!chairman || (d.width > 0 && d.data.starts_with(b"\x89PNG\r\n\x1a\n")))
        && (chairman || !d.category.trim().is_empty());
    let (mut save, mut cancel) = (false, false);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, tl!("OK"), true)).inner.clicked() {
            save = true;
        }
        if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
            cancel = true;
        }
    });
    (save, cancel)
}

/// The Custom stamps section of the stamps palette.
pub(crate) fn palette_section(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens) {
    widgets::section_title(ui, tl!("Custom"));
    let mut remove = None;
    let mut categories: Vec<String> = app.custom_stamps.iter().map(|s| s.category.clone()).collect();
    categories.sort();
    categories.dedup();
    for c in categories {
        ui.label(egui::RichText::new(&c).small().color(t.text_muted));
        for (i, s) in app.custom_stamps.iter().enumerate().filter(|(_, s)| s.category == c) {
            let active = app.quick_tool == QuickTool::CustomStamp(i);
            let resp = ui.add(egui::Button::selectable(active, s.name.as_str()).min_size(egui::vec2(ui.available_width(), 28.0)));
            if resp.clicked() {
                app.quick_tool = QuickTool::CustomStamp(i);
            }
            resp.context_menu(|ui| {
                if ui.button(tl!("Delete stamp")).clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
        }
    }
    if let Some(i) = remove {
        app.custom_stamps.remove(i);
        app.quick_tool = QuickTool::Select;
    }
    ui.add_space(4.0);
    if widgets::pill_button(ui, tl!("Create custom stamp…"), false).on_hover_text(tl!("From a PDF page or an image")).clicked() {
        app.pick_stamp_file(false);
    }
    chairman_section(app, ui);
}

fn chairman_section(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    widgets::section_title(ui, "理事長印");
    if let Some(error) = &app.seal_store.error {
        ui.label(egui::RichText::new(error).color(egui::Color32::from_rgb(176, 48, 48)));
        ui.label("復号できない印影は表示も削除もしません。暗号化ファイルは変更していません。");
        return;
    }
    ui.label(
        egui::RichText::new(
            "この PC の現在の Windows ユーザーだけが復号できます。他のユーザーや他の PC では開けません。元の画像ファイル自体は暗号化されません。",
        )
        .small(),
    );
    let mut remove = None;
    for (i, seal) in app.seal_store.library.seals.iter().enumerate() {
        let active = app.quick_tool == QuickTool::ChairmanSeal(i);
        let label = format!("{}（{} × {}）", seal.name, seal.width, seal.height);
        if ui.add(egui::Button::selectable(active, label).min_size(egui::vec2(ui.available_width(), 28.0))).clicked() {
            app.quick_tool = QuickTool::ChairmanSeal(i);
        }
    }
    if app.seal_store.library.seals.is_empty() {
        ui.label("登録された理事長印はありません。");
    }
    ui.add_space(4.0);
    if widgets::pill_button(ui, "理事長印を登録…", false).clicked() {
        app.pick_stamp_file(true);
    }
    let can_delete = matches!(app.quick_tool, QuickTool::ChairmanSeal(i) if i < app.seal_store.library.seals.len());
    if ui.add_enabled_ui(can_delete, |ui| widgets::pill_button(ui, "選択中の理事長印を削除…", false)).inner.clicked() {
        remove = Some(());
    }
    if remove.is_some() {
        app.seal_ack = false;
        app.dialog = Some(Dialog::DeleteSeal);
    }
}

impl PdfCraftApp {
    /// Create ▸ choose a PDF or an image for a new custom stamp.
    pub(crate) fn pick_stamp_file(&mut self, chairman: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let read = move |app: &mut Self, path: std::path::PathBuf| match std::fs::read(&path) {
                Ok(bytes) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    if chairman {
                        app.start_chairman_seal(name, bytes);
                    } else {
                        app.start_custom_stamp(name, bytes);
                    }
                }
                Err(e) => app.notify_fmt("Couldn't read {name}: {e}", &[("name", &path.display().to_string()), ("e", &e.to_string())]),
            };
            match self.save_override.clone() {
                Some(p) if [".png", ".jpg", ".pdf"].iter().any(|e| p.ends_with(e)) => read(self, p.into()),
                Some(_) => {}
                None => {
                    let dialog = rfd::AsyncFileDialog::new()
                        .add_filter(tl!("PDF or image"), &["pdf", "png", "jpg", "jpeg", "tif", "tiff", "gif", "bmp", "jp2", "j2k", "jpx"])
                        .set_title(tl!("Select a file for the stamp"));
                    self.ask_one(crate::pickers::Ask::File(dialog), None, read);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = chairman;
            self.notify_tr("Custom stamps arrive on the web with file pickers for images");
        }
    }

    /// Open the Create Custom Stamp dialog for a file.
    pub fn start_custom_stamp(&mut self, file: String, bytes: Vec<u8>) {
        self.open_stamp_draft(file, bytes, false);
    }

    /// Register a chairman seal. The bytes stay out of the custom-stamp library.
    pub fn start_chairman_seal(&mut self, file: String, bytes: Vec<u8>) {
        if self.seal_store.locked() {
            self.notify("印影ファイルを復号できないため、登録も削除もできません。");
            return;
        }
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            self.notify("理事長印は透過PNGにしてください。");
            return;
        }
        self.open_stamp_draft(file, bytes, true);
    }

    fn open_stamp_draft(&mut self, file: String, bytes: Vec<u8>, chairman: bool) {
        let limit = if chairman { crate::seal_store::MAX_SEAL_BYTES } else { MAX_STAMP_BYTES };
        if bytes.len() > limit {
            self.notify_fmt("{file} is too large for a stamp (at most {n} MB)", &[("file", &file), ("n", &(limit >> 20).to_string())]);
            return;
        }
        let (width, height) = crate::seal_store::preview_rgba(&bytes).map(|(w, h, _)| (w, h)).unwrap_or((0, 0));
        if chairman && (width == 0 || height == 0) {
            self.notify("この PNG のプレビューを表示できないため、登録しませんでした。");
            return;
        }
        let name = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s).to_string();
        let category =
            if chairman { String::new() } else { self.custom_stamps.last().map(|s| s.category.clone()).unwrap_or_else(|| "My stamps".into()) };
        self.stamp_draft = crate::stamps_ui::StampDraft { file, data: Arc::new(bytes), category, name, chairman, width, height };
        self.dialog = Some(Dialog::CreateStamp);
    }

    /// Save the dialog's stamp in the library and choose it.
    pub(crate) fn save_custom_stamp(&mut self) {
        let d = std::mem::take(&mut self.stamp_draft);
        if d.chairman {
            let seal = crate::seal_store::SealImage {
                name: d.name.trim().to_string(),
                file: d.file,
                width: d.width,
                height: d.height,
                data: d.data.to_vec(),
            };
            match self.seal_store.add_seal(seal) {
                Ok(()) => {
                    let index = self.seal_store.library.seals.len().saturating_sub(1);
                    self.quick_tool = QuickTool::ChairmanSeal(index);
                }
                Err(error) => self.notify(error),
            }
            return;
        }
        self.custom_stamps.push(CustomStamp { category: d.category.trim().into(), name: d.name.trim().into(), file: d.file, page: 0, data: d.data });
        self.quick_tool = QuickTool::CustomStamp(self.custom_stamps.len() - 1);
    }
}

/// The custom stamps for `persist` (kept small: base64 of each file).
pub(crate) fn encode(stamps: &[CustomStamp]) -> serde_json::Value {
    serde_json::to_value(stamps).unwrap_or_default()
}

/// Read stamps back; malformed entries are dropped.
pub(crate) fn decode(v: &serde_json::Value) -> Vec<CustomStamp> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| serde_json::from_value::<CustomStamp>(x.clone()).ok()).filter(|s| s.data.len() <= MAX_STAMP_BYTES).collect())
        .unwrap_or_default()
}

fn paint_draft_preview(ui: &mut egui::Ui, draft: &StampDraft) {
    let Some((width, height, pixels)) = crate::seal_store::preview_rgba(&draft.data) else { return };
    if pixels.len() != (width as usize).saturating_mul(height as usize).saturating_mul(4) {
        return;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &pixels);
    let texture = ui.ctx().load_texture("stamp-register-preview", image, egui::TextureOptions::LINEAR);
    let fit = 140.0 / (width.max(height) as f32).max(1.0);
    ui.image((texture.id(), egui::vec2(width as f32 * fit, height as f32 * fit)));
}

/// A stamp the user has positioned but not confirmed. It is not in the document and not in undo.
#[derive(Clone, Debug)]
pub struct StampPlace {
    pub page: usize,
    pub rect: [f64; 4],
    pub name: String,
    pub file: String,
    pub data: Arc<Vec<u8>>,
    pub source_page: usize,
    pub opacity: f64,
    pub chairman: bool,
    pub pixels: Option<(u32, u32, Vec<u8>)>,
    pub drag: Option<StampDrag>,
}

#[derive(Clone, Copy, Debug)]
pub enum StampDrag {
    Move { from: egui::Pos2, origin: [f64; 4] },
    Resize { handle: (i8, i8), from: egui::Pos2, origin: [f64; 4] },
}

pub(crate) fn place_at(info: &pdfcraft_render::PageInfo, at: [f64; 2], pixels: Option<(u32, u32)>) -> [f64; 4] {
    let (sw, sh) = pixels.map(|(w, h)| (w as f64, h as f64)).unwrap_or((120.0, 120.0));
    let (sw, sh) = (sw.max(1.0), sh.max(1.0));
    let scale = (200.0 / sw.max(sh)).min(1.0);
    let (w, h) = if matches!(info.rotation, 90 | 270) { (sh * scale, sw * scale) } else { (sw * scale, sh * scale) };
    [at[0] - w / 2.0, at[1] - h / 2.0, at[0] + w / 2.0, at[1] + h / 2.0]
}

fn screen_to_user(xf: &crate::canvas::PageXform, info: &pdfcraft_render::DocInfo, page: usize, p: egui::Pos2) -> [f64; 2] {
    let (vx, vy) = xf.screen_to_view(p);
    let user = info.pages.get(page).map(|page| page.view_to_user(vx, vy)).unwrap_or([0.0, 0.0]);
    [f64::from(user[0]), f64::from(user[1])]
}

fn user_rect_to_screen(xf: &crate::canvas::PageXform, info: &pdfcraft_render::DocInfo, page: usize, rect: [f64; 4]) -> egui::Rect {
    xf.user_rect(info, page, [rect[0] as f32, rect[1] as f32, rect[2] as f32, rect[3] as f32])
}

fn screen_rect_to_user(xf: &crate::canvas::PageXform, info: &pdfcraft_render::DocInfo, page: usize, rect: egui::Rect) -> [f64; 4] {
    let a = screen_to_user(xf, info, page, rect.left_top());
    let b = screen_to_user(xf, info, page, rect.right_bottom());
    [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])]
}

const CORNERS: [(i8, i8); 4] = [(-1, -1), (1, -1), (1, 1), (-1, 1)];

/// Draw and drag the unconfirmed stamp. Returns true when this click belongs to the draft.
pub(crate) fn page_draft(
    painter: &egui::Painter,
    view: &mut crate::canvas::DocView,
    info: &pdfcraft_render::DocInfo,
    page: usize,
    xf: &crate::canvas::PageXform,
) -> bool {
    let Some(place) = view.stamp_place.as_mut() else { return false };
    if place.page != page {
        return false;
    }
    let screen = user_rect_to_screen(xf, info, page, place.rect);
    if let Some((width, height, pixels)) = place.pixels.clone()
        && pixels.len() == (width as usize).saturating_mul(height as usize).saturating_mul(4)
    {
        let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &pixels);
        let texture = painter.ctx().load_texture("stamp-place-preview", image, egui::TextureOptions::LINEAR);
        let tint = egui::Color32::WHITE.gamma_multiply(place.opacity.clamp(0.0, 1.0) as f32);
        painter.image(texture.id(), screen, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
    }
    painter.rect_stroke(screen, egui::CornerRadius::ZERO, egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 110, 180)), egui::StrokeKind::Outside);
    for handle in CORNERS {
        let at = crate::comments::handle_pos(screen, handle);
        painter.circle(at, 4.5, egui::Color32::WHITE, egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 110, 180)));
    }
    let pointer = painter.ctx().input(|input| input.pointer.hover_pos());
    let pressed = painter.ctx().input(|input| input.pointer.primary_pressed());
    let down = painter.ctx().input(|input| input.pointer.primary_down());
    let handle_at = |p: egui::Pos2| CORNERS.into_iter().find(|handle| crate::comments::handle_pos(screen, *handle).distance(p) <= 8.0);
    if pressed && let Some(p) = pointer {
        if let Some(handle) = handle_at(p) {
            place.drag = Some(StampDrag::Resize { handle, from: p, origin: place.rect });
        } else if screen.expand(2.0).contains(p) {
            place.drag = Some(StampDrag::Move { from: p, origin: place.rect });
        }
    }
    if let Some(p) = pointer {
        match place.drag {
            Some(StampDrag::Move { from, origin }) => {
                let (a, b) = (screen_to_user(xf, info, page, from), screen_to_user(xf, info, page, p));
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                place.rect = [origin[0] + dx, origin[1] + dy, origin[2] + dx, origin[3] + dy];
            }
            Some(StampDrag::Resize { handle, from, origin }) => {
                let from_screen = user_rect_to_screen(xf, info, page, origin);
                let ratio = (from_screen.height() > 0.0)
                    .then(|| from_screen.width() / from_screen.height())
                    .filter(|ratio| ratio.is_finite() && *ratio > 0.0);
                let next = crate::comments::resized(from_screen, handle, p - from, ratio);
                if next.width() >= 4.0 && next.height() >= 4.0 {
                    place.rect = screen_rect_to_user(xf, info, page, next);
                }
            }
            None => {}
        }
    }
    let consumed = place.drag.is_some();
    if !down && let Some(place) = view.stamp_place.as_mut() {
        place.drag = None;
    }
    consumed
}

/// Opacity of a stamp already placed in this session. The slider does not write until it is
/// released, and then it is one undoable style edit of that stamp only.
fn owned_stamp_opacity(app: &mut PdfCraftApp, ui: &mut egui::Ui, index: usize) {
    let Some(view) = app.views.get(index) else { return };
    let Some((page, ann)) = view.comments.selected else { return };
    let id = view.id;
    if !app.session.owns_stamp(id, page, ann) {
        return;
    }
    let current = app.session.get(id).and_then(|doc| doc.comment_props(page, ann)).map(|props| props.opacity).unwrap_or(1.0);
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("stamp_opacity_bar").exact_size(44.0).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 4))).show(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                ui.label("スタンプの透明度");
                let memory = ui.id().with(("owned-stamp-opacity", page, ann));
                let mut opacity = ui.data(|data| data.get_temp::<f64>(memory)).unwrap_or(current);
                let response = ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("透明度"));
                if response.dragged() {
                    ui.data_mut(|data| data.insert_temp(memory, opacity));
                }
                let released = response.drag_stopped() || (response.changed() && !response.dragged());
                if released {
                    ui.data_mut(|data| data.remove::<f64>(memory));
                    let opacity = opacity.clamp(0.0, 1.0);
                    if (opacity - current).abs() > 0.001 {
                        app.apply_edit(pdfcraft_engine::Edit::StyleAnnotation {
                            page,
                            index: ann,
                            color: None,
                            opacity: Some(opacity),
                            width: None,
                            endings: None,
                        });
                    }
                }
            });
        },
    );
}

pub(crate) fn stamp_bar(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let Some(index) = app.active else { return };
    if app.views.get(index).is_none_or(|view| view.stamp_place.is_none()) {
        owned_stamp_opacity(app, ui, index);
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("stamp_place_bar").exact_size(44.0).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 4))).show(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                ui.label("スタンプの位置");
                let mut opacity = app.views[index].stamp_place.as_ref().map(|place| place.opacity).unwrap_or(1.0);
                if ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("透明度")).changed()
                    && let Some(place) = app.views[index].stamp_place.as_mut()
                {
                    place.opacity = opacity.clamp(0.0, 1.0);
                }
                if widgets::pill_button(ui, "確定", true).clicked() {
                    app.ask_stamp_confirm();
                }
                if widgets::pill_button(ui, "キャンセル", false).clicked() {
                    app.views[index].stamp_place = None;
                }
            });
        },
    );
}

pub(crate) fn confirm_seal_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> (bool, bool) {
    let Some(index) = app.active else { return (false, true) };
    let Some(place) = app.views[index].stamp_place.clone() else { return (false, true) };
    let doc_name = app.session.get(app.views[index].id).map(|doc| doc.name.clone()).unwrap_or_default();
    ui.label(egui::RichText::new("理事長印の確認").font(theme::semibold(18.0)));
    ui.add_space(6.0);
    ui.label(format!("印影: {}", place.name));
    ui.label(format!("ファイル: {doc_name}"));
    ui.label(format!("ページ: {}", place.page.saturating_add(1)));
    ui.label("これはこの文書への押印確認です。アプリが承認を証明するものではありません。確定は、保存や印刷が済んだという意味ではありません。");
    paint_place_preview(ui, &place);
    ui.add_space(8.0);
    ui.add(widgets::checkbox(&mut app.seal_ack, "この文書への押印が承認されていることを確認しました"));
    ui.add_space(8.0);
    let (mut ok, mut cancel) = (false, false);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(app.seal_ack, |ui| widgets::pill_button(ui, "確定", true)).inner.clicked() {
            ok = true;
        }
        if widgets::pill_button(ui, "キャンセル", false).clicked() {
            cancel = true;
        }
    });
    (ok, cancel)
}

fn paint_place_preview(ui: &mut egui::Ui, place: &StampPlace) {
    let Some((width, height, pixels)) = &place.pixels else { return };
    if pixels.len() != (*width as usize).saturating_mul(*height as usize).saturating_mul(4) {
        return;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([*width as usize, *height as usize], pixels);
    let texture = ui.ctx().load_texture("stamp-confirm-preview", image, egui::TextureOptions::LINEAR);
    let fit = 120.0 / (*width.max(height) as f32).max(1.0);
    ui.add(
        egui::Image::from_texture(&texture)
            .max_size(egui::vec2(*width as f32 * fit, *height as f32 * fit))
            .tint(egui::Color32::WHITE.gamma_multiply(place.opacity.clamp(0.0, 1.0) as f32)),
    );
}

pub(crate) fn delete_seal_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> (bool, bool) {
    let QuickTool::ChairmanSeal(index) = app.quick_tool else { return (false, true) };
    let Some(seal) = app.seal_store.library.seals.get(index) else { return (false, true) };
    ui.label(egui::RichText::new("理事長印を削除").font(theme::semibold(18.0)));
    ui.add_space(6.0);
    ui.label(format!("「{}」を暗号化ファイルから削除します。", seal.name));
    ui.label("元の画像ファイルや、OS のバックアップに残ったデータは削除されません。");
    if app.seal_store.locked() {
        ui.label("印影ファイルを復号できないため、削除できません。");
        return (false, widgets::pill_button(ui, "閉じる", false).clicked());
    }
    ui.add_space(8.0);
    let (mut ok, mut cancel) = (false, false);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if widgets::pill_button(ui, "削除", true).clicked() {
            ok = true;
        }
        if widgets::pill_button(ui, "キャンセル", false).clicked() {
            cancel = true;
        }
    });
    (ok, cancel)
}

impl PdfCraftApp {
    pub(crate) fn ask_stamp_confirm(&mut self) {
        let Some(index) = self.active else { return };
        let Some(place) = self.views[index].stamp_place.clone() else { return };
        if let Some(why) = self.stamp_refusal(index) {
            self.notify(why);
            return;
        }
        if place.chairman {
            self.seal_ack = false;
            self.dialog = Some(Dialog::ConfirmSeal);
            return;
        }
        self.commit_stamp_place();
    }

    pub(crate) fn commit_stamp_place(&mut self) {
        let Some(index) = self.active else { return };
        let Some(place) = self.views[index].stamp_place.clone() else { return };
        if let Some(why) = self.stamp_refusal(index) {
            self.notify(why);
            return;
        }
        let id = self.views[index].id;
        let file_name = self.session.get(id).map(|doc| doc.name.clone()).unwrap_or_default();
        if place.chairman {
            let event = crate::seal_store::SealEvent {
                at: crate::seal_store::utc_now(),
                file: file_name.clone(),
                page: place.page.saturating_add(1) as u32,
                seal: place.name.clone(),
                event: "confirmed".into(),
            };
            if let Err(error) = self.seal_store.append_event(event) {
                self.notify(format!("使用履歴を記録できないため、押印しませんでした。{error}"));
                return;
            }
        }
        let edit = pdfcraft_engine::Edit::AddCustomStamp {
            page: place.page,
            rect: place.rect,
            name: place.name.clone(),
            file: pdfcraft_engine::MarkFile { name: place.file.clone(), bytes: place.data.clone(), page: place.source_page },
            author: self.comment_prefs.author.clone(),
            opacity: place.opacity.clamp(0.0, 1.0),
        };
        if !self.apply_edit(edit) {
            if place.chairman
                && let Err(error) = self.seal_store.pop_event()
            {
                self.notify(format!("押印は文書に入っていません。使用履歴の取り消しに失敗しました。{error}"));
            }
            return;
        }
        if place.chairman
            && let Some(refer) = self.session.newest_owned_stamp(id)
        {
            self.seal_uses.push(crate::seal_store::SealUse {
                num: refer.num,
                generation: refer.generation,
                name: place.name,
                page: place.page.saturating_add(1) as u32,
                file_name,
                active: true,
            });
        }
        if let Some(view) = self.views.get_mut(index) {
            view.stamp_place = None;
        }
    }

    pub(crate) fn delete_selected_seal(&mut self) {
        if self.seal_store.locked() {
            self.notify("印影ファイルを復号できないため、削除できません。");
            return;
        }
        let QuickTool::ChairmanSeal(index) = self.quick_tool else { return };
        match self.seal_store.delete_seal(index) {
            Ok(()) => {
                self.quick_tool = QuickTool::Select;
                if let Some(i) = self.active {
                    self.views[i].stamp_place = None;
                }
            }
            Err(error) => self.notify(error),
        }
    }

    pub(crate) fn stamp_refusal(&self, index: usize) -> Option<String> {
        let id = self.views.get(index)?.id;
        let doc = self.session.get(id)?;
        if doc.read_only_reason.is_some() {
            return Some("この PDF の署名状態を確認できないため、スタンプは配置できません。".into());
        }
        if doc.is_signed() {
            return Some("電子署名がある PDF には、一般スタンプも理事長印も押せません。署名は削除していません。".into());
        }
        if doc.permissions().is_some_and(|permissions| !permissions.annotate()) {
            return Some("この PDF は注釈を禁止しているため、スタンプを配置できません。".into());
        }
        None
    }

    /// After undo or redo, keep the chairman-seal log aligned with whether that seal is still in
    /// the working document. A failed log write puts the document back.
    pub(crate) fn note_seal_history_step(&mut self, id: pdfcraft_engine::DocId, undo: bool, label: &str) {
        let deletion = match label {
            "Add stamp" => false,
            "Delete comment" => true,
            _ => return,
        };
        let present = self.session.present_owned_stamps(id);
        let Some(use_index) = self.seal_uses.iter().rposition(|use_| {
            let here = present.iter().any(|refer| refer.num == use_.num && refer.generation == use_.generation);
            if deletion { use_.active != undo && here == undo } else { use_.active == undo && here != undo }
        }) else {
            return;
        };
        // Adding: undo removes it (`undone`), redo puts it back (`redone`).
        // Deleting: undo puts it back (`removal-undone`), redo takes it off again (`removed`).
        let event_name = if deletion {
            if undo { "removal-undone" } else { "removed" }
        } else if undo {
            "undone"
        } else {
            "redone"
        };
        let event = crate::seal_store::SealEvent {
            at: crate::seal_store::utc_now(),
            file: self.seal_uses[use_index].file_name.clone(),
            page: self.seal_uses[use_index].page,
            seal: self.seal_uses[use_index].name.clone(),
            event: event_name.into(),
        };
        if self.seal_store.append_event(event).is_err() {
            let restored = if undo { self.session.redo(id).is_ok() } else { self.session.undo(id).is_ok() };
            if restored {
                self.notify("使用履歴を記録できないため、文書の変更を元に戻しました。");
            } else {
                self.notify("使用履歴を記録できず、文書の状態も戻せませんでした。保存や印刷の前に内容を確認してください。");
            }
            return;
        }
        if let Some(use_) = self.seal_uses.get_mut(use_index) {
            use_.active = if deletion { undo } else { !undo };
        }
    }
}
