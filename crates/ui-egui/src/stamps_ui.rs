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
    /// Placement percent. Missing in settings written before v0.6.0, which means 100%.
    #[serde(default = "crate::stamp_preset::default_scale")]
    pub scale: f64,
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
#[derive(Clone, Debug, PartialEq)]
pub struct StampDraft {
    pub file: String,
    pub data: Arc<Vec<u8>>,
    pub category: String,
    pub name: String,
    pub chairman: bool,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl Default for StampDraft {
    fn default() -> Self {
        Self {
            file: String::new(),
            data: Arc::new(Vec::new()),
            category: String::new(),
            name: String::new(),
            chairman: false,
            width: 0,
            height: 0,
            scale: 100.0,
        }
    }
}

/// A date stamp being registered. The preview date is not stored; a placement starts on the
/// Japan date of the click. `data` is the base PNG and is not drawn on.
#[derive(Clone, Debug)]
pub struct DateDraft {
    pub editing: Option<usize>,
    pub file: String,
    pub data: Arc<Vec<u8>>,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub region: [f64; 4],
    pub font: f64,
    pub color: [u8; 3],
    /// `Some` for a circle drawn from text. `None` keeps the older PNG template.
    pub drawn: Option<crate::stamp_preset::DrawnDate>,
    pub date_text: String,
    pub show_calendar: bool,
    pub calendar_year: i32,
    pub calendar_month: u32,
    preview: Option<(u32, u32, Vec<u8>)>,
    preview_key: String,
}

impl Default for DateDraft {
    fn default() -> Self {
        let today = crate::stamp_preset::japan_today();
        Self {
            editing: None,
            file: String::new(),
            data: Arc::new(Vec::new()),
            name: String::new(),
            width: 0,
            height: 0,
            scale: 100.0,
            region: crate::stamp_preset::default_region(),
            font: crate::stamp_preset::default_font(),
            color: crate::stamp_preset::default_color(),
            drawn: None,
            date_text: today.format(),
            show_calendar: false,
            calendar_year: today.year,
            calendar_month: today.month,
            preview: None,
            preview_key: String::new(),
        }
    }
}

/// What a finished edit of a percent field did. Typing itself does not produce this.
enum ScaleCommit {
    /// The field is idle, still being typed, or Escape put the old percent back.
    Unchanged,
    /// Enter, leaving the field, or an arrow key accepted a percent inside 25–400.
    Set(f64),
    /// Enter or leaving the field could not be read as 25–400. The old percent is shown again.
    Rejected,
}

/// The percent as typed. A whole number has no decimal, so Ctrl+A and a new number replace it cleanly.
fn format_percent(value: f64) -> String {
    if !value.is_finite() {
        return "100".into();
    }
    let nearest = value.round();
    if (value - nearest).abs() < 0.05 { format!("{nearest:.0}") } else { format!("{value:.1}") }
}

/// `80`, `80%`, and surrounding spaces. Empty and other text are not a number yet.
fn parse_scale_text(text: &str) -> Option<f64> {
    let trimmed = text.trim().trim_end_matches(|c: char| c == '%' || c.is_whitespace()).trim();
    if trimmed.is_empty() {
        return None;
    }
    let value = trimmed.parse::<f64>().ok()?;
    if value.is_finite() { Some(value) } else { None }
}

#[derive(Clone)]
struct ScaleDraft {
    text: String,
    /// [`egui::Context::cumulative_pass_nr`] when this text was saved. A draft from an older pass
    /// is discarded, so reopening a dialog does not commit a previous edit.
    pass: u64,
}

fn select_scale_text(ui: &egui::Ui, id: egui::Id, text: &str) {
    let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) else { return };
    let chars = text.chars().count();
    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::default(), egui::text::CCursor::new(chars))));
    state.store(ui.ctx(), id);
}

/// A percent box. The draft string lives only while the box is focused, so a half-typed `8`
/// is not clamped to 25 and is not replaced by the last committed percent on the next frame.
/// Enter, or clicking elsewhere, commits. Escape restores the committed percent without a notice.
fn scale_percent_field(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, committed: f64, label: egui::Id) -> ScaleCommit {
    let id = ui.id().with(salt);
    let pass = ui.ctx().cumulative_pass_nr();
    // A widget earlier in the tree can take focus before this one runs. The draft saved on the
    // previous pass is still the text the user typed.
    let focused = ui.memory(|mem| mem.has_focus(id));
    let previous = ui.data(|data| data.get_temp::<ScaleDraft>(id));
    let carry = previous.as_ref().is_some_and(|draft| !focused && pass.saturating_sub(draft.pass) <= 1);
    let mut text =
        if focused || carry { previous.map(|draft| draft.text).unwrap_or_else(|| format_percent(committed)) } else { format_percent(committed) };
    let response = ui.add(crate::widgets::line(&mut text).id(id).desired_width(88.0).char_limit(8)).labelled_by(label);
    if response.gained_focus() {
        select_scale_text(ui, response.id, &text);
    }
    let up = response.has_focus() && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
    let down = response.has_focus() && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
    if up != down {
        let base = parse_scale_text(&text).unwrap_or(committed);
        let stepped = (base + if up { 1.0 } else { -1.0 }).clamp(crate::stamp_preset::SCALE_MIN, crate::stamp_preset::SCALE_MAX);
        ui.data_mut(|data| data.insert_temp(id, ScaleDraft { text: format_percent(stepped), pass }));
        return if (stepped - committed).abs() < 1e-9 { ScaleCommit::Unchanged } else { ScaleCommit::Set(stepped) };
    }
    let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
    if escape && (response.has_focus() || response.lost_focus() || carry) {
        ui.data_mut(|data| data.remove::<ScaleDraft>(id));
        ui.memory_mut(|mem| mem.surrender_focus(id));
        return ScaleCommit::Unchanged;
    }
    if response.lost_focus() || carry {
        ui.data_mut(|data| data.remove::<ScaleDraft>(id));
        return match parse_scale_text(&text).and_then(crate::stamp_preset::checked_scale) {
            Some(scale) if (scale - committed).abs() < 1e-9 => ScaleCommit::Unchanged,
            Some(scale) => ScaleCommit::Set(scale),
            None => ScaleCommit::Rejected,
        };
    }
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, ScaleDraft { text, pass }));
    } else {
        ui.data_mut(|data| data.remove::<ScaleDraft>(id));
    }
    ScaleCommit::Unchanged
}

pub(crate) fn create_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> (bool, bool) {
    let categories: Vec<String> = {
        let mut c: Vec<String> = app.custom_stamps.iter().map(|s| s.category.clone()).collect();
        c.sort();
        c.dedup();
        c
    };
    let chairman = app.stamp_draft.chairman;
    let mut scale_rejected = false;
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
        let l = ui.label("既定倍率 (%)");
        let scale = d.scale;
        match scale_percent_field(ui, "stamp-draft-scale", scale, l.id) {
            ScaleCommit::Set(scale) => d.scale = scale,
            ScaleCommit::Rejected => scale_rejected = true,
            ScaleCommit::Unchanged => {}
        }
        ui.end_row();
    });
    ui.label(egui::RichText::new("100%は、これまでクリックで置いていた大きさです。縦横比は維持します。").small().color(t.text_faint));
    ui.add_space(12.0);
    let ok = !d.name.trim().is_empty()
        && (!chairman || (d.width > 0 && d.data.starts_with(b"\x89PNG\r\n\x1a\n")))
        && (chairman || !d.category.trim().is_empty())
        && crate::stamp_preset::checked_scale(d.scale).is_some();
    let (mut save, mut cancel, mut trial) = (false, false, false);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, tl!("OK"), true)).inner.clicked() {
            save = true;
        }
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, "試し置き", false)).inner.clicked() {
            trial = true;
            save = true;
        }
        if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
            cancel = true;
        }
    });
    if trial {
        app.stamp_try = true;
    }
    if scale_rejected {
        app.notify("倍率は25%から400%です。");
    }
    (save, cancel)
}

/// The Custom stamps section of the stamps palette.
pub(crate) fn palette_section(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens) {
    widgets::section_title(ui, tl!("Custom"));
    let mut remove = None;
    let mut scale_edit = None;
    let mut scale_rejected = false;
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
                let l = ui.label("既定倍率 (%)");
                match scale_percent_field(ui, ("custom-menu-scale", i), s.scale, l.id) {
                    ScaleCommit::Set(scale) => scale_edit = Some((i, scale)),
                    ScaleCommit::Rejected => scale_rejected = true,
                    ScaleCommit::Unchanged => {}
                }
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
    if let Some((i, scale)) = scale_edit
        && let Some(stamp) = app.custom_stamps.get_mut(i)
    {
        stamp.scale = scale;
    }
    if scale_rejected {
        app.notify("倍率は25%から400%です。");
    }
    ui.add_space(4.0);
    if widgets::pill_button(ui, tl!("Create custom stamp…"), false).on_hover_text(tl!("From a PDF page or an image")).clicked() {
        app.pick_stamp_file(false);
    }
    date_section(app, ui, t);
    chairman_section(app, ui);
}

fn date_section(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens) {
    ui.add_space(8.0);
    widgets::section_title(ui, "データー印");
    ui.label(
        egui::RichText::new("通常のスタンプです。理事長印の承認、暗号化、使用履歴の対象ではありません。日付は配置を確定した画像に固定されます。")
            .small()
            .color(t.text_muted),
    );
    let mut remove = None;
    let mut edit = None;
    let mut scale_edit = None;
    let mut scale_rejected = false;
    for (i, stamp) in app.date_stamps.iter().enumerate() {
        let active = app.quick_tool == QuickTool::DateStamp(i);
        let label = format!("{}（{:.0}%）", stamp.name, stamp.scale);
        let resp = ui.add(egui::Button::selectable(active, label).min_size(egui::vec2(ui.available_width(), 28.0)));
        if resp.clicked() {
            app.quick_tool = QuickTool::DateStamp(i);
        }
        resp.context_menu(|ui| {
            let l = ui.label("既定倍率 (%)");
            match scale_percent_field(ui, ("date-menu-scale", i), stamp.scale, l.id) {
                ScaleCommit::Set(scale) => scale_edit = Some((i, scale)),
                ScaleCommit::Rejected => scale_rejected = true,
                ScaleCommit::Unchanged => {}
            }
            if ui.button("編集").clicked() {
                edit = Some(i);
                ui.close();
            }
            if ui.button("削除").clicked() {
                remove = Some(i);
                ui.close();
            }
        });
    }
    if app.date_stamps.is_empty() {
        ui.label("登録されたデーター印はありません。");
    }
    if let Some(i) = remove {
        app.date_stamps.remove(i);
        app.quick_tool = QuickTool::Select;
    }
    if let Some((i, scale)) = scale_edit
        && let Some(stamp) = app.date_stamps.get_mut(i)
    {
        stamp.scale = scale;
    }
    if scale_rejected {
        app.notify("倍率は25%から400%です。");
    }
    if let Some(i) = edit {
        app.open_date_draft(i);
    }
    ui.add_space(4.0);
    if widgets::pill_button(ui, "データー印を登録…", false).clicked() {
        app.start_drawn_date();
    }
    if widgets::pill_button(ui, "画像から登録…", false).clicked() {
        app.pick_date_stamp_file();
    }
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
        let label = format!("{}（{} × {}、{:.0}%）", seal.name, seal.width, seal.height, seal.scale);
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
        self.stamp_draft = crate::stamps_ui::StampDraft { file, data: Arc::new(bytes), category, name, chairman, width, height, scale: 100.0 };
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
                scale: crate::stamp_preset::scale_or_hundred(d.scale),
            };
            match self.seal_store.add_seal(seal) {
                Ok(()) => {
                    let index = self.seal_store.library.seals.len().saturating_sub(1);
                    self.quick_tool = QuickTool::ChairmanSeal(index);
                    self.note_stamp_try();
                }
                Err(error) => self.notify(error),
            }
            return;
        }
        self.custom_stamps.push(CustomStamp {
            category: d.category.trim().into(),
            name: d.name.trim().into(),
            file: d.file,
            page: 0,
            data: d.data,
            scale: crate::stamp_preset::scale_or_hundred(d.scale),
        });
        self.quick_tool = QuickTool::CustomStamp(self.custom_stamps.len() - 1);
        self.note_stamp_try();
    }

    fn note_stamp_try(&mut self) {
        if self.stamp_try {
            self.stamp_try = false;
            self.notify("ページをクリックすると仮置きします。確定するまで文書は変わりません。");
        }
    }

    /// Choose a PNG for a date stamp. The file is a normal stamp image, not a chairman seal.
    pub(crate) fn pick_date_stamp_file(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let read = move |app: &mut Self, path: std::path::PathBuf| match std::fs::read(&path) {
                Ok(bytes) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    app.start_date_stamp(name, bytes);
                }
                Err(e) => app.notify_fmt("Couldn't read {name}: {e}", &[("name", &path.display().to_string()), ("e", &e.to_string())]),
            };
            match self.save_override.clone() {
                Some(p) if p.ends_with(".png") => read(self, p.into()),
                Some(_) => {}
                None => {
                    let dialog = rfd::AsyncFileDialog::new().add_filter("PNG", &["png"]).set_title("データー印の元画像");
                    self.ask_one(crate::pickers::Ask::File(dialog), None, read);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.notify_tr("Custom stamps arrive on the web with file pickers for images");
        }
    }

    /// Open the drawn date-stamp editor. No image file is required.
    pub fn start_drawn_date(&mut self) {
        if self.date_stamps.len() >= 32 {
            self.notify("データー印は32件までです。");
            return;
        }
        let today = crate::stamp_preset::japan_today();
        self.date_draft = DateDraft {
            file: "date-stamp.png".into(),
            name: "データー印".into(),
            drawn: Some(crate::stamp_preset::DrawnDate::default()),
            date_text: today.format(),
            calendar_year: today.year,
            calendar_month: today.month,
            ..DateDraft::default()
        };
        self.dialog = Some(Dialog::DateStamp);
    }

    pub fn start_date_stamp(&mut self, file: String, bytes: Vec<u8>) {
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            self.notify("データー印の元画像は透過PNGにしてください。");
            return;
        }
        if bytes.len() > MAX_STAMP_BYTES {
            self.notify_fmt("{file} is too large for a stamp (at most {n} MB)", &[("file", &file), ("n", &(MAX_STAMP_BYTES >> 20).to_string())]);
            return;
        }
        let Some((width, height, _)) = crate::seal_store::preview_rgba(&bytes) else {
            self.notify("この PNG のプレビューを表示できないため、登録しませんでした。");
            return;
        };
        let name = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s).to_string();
        let today = crate::stamp_preset::japan_today();
        self.date_draft = DateDraft {
            file,
            data: Arc::new(bytes),
            name,
            width,
            height,
            date_text: today.format(),
            calendar_year: today.year,
            calendar_month: today.month,
            ..DateDraft::default()
        };
        self.dialog = Some(Dialog::DateStamp);
    }

    pub(crate) fn open_date_draft(&mut self, index: usize) {
        let Some(stamp) = self.date_stamps.get(index) else { return };
        let today = crate::stamp_preset::japan_today();
        let (width, height) = crate::seal_store::preview_rgba(&stamp.data).map(|(w, h, _)| (w, h)).unwrap_or((0, 0));
        self.date_draft = DateDraft {
            editing: Some(index),
            file: stamp.file.clone(),
            data: stamp.data.clone(),
            name: stamp.name.clone(),
            width,
            height,
            scale: stamp.scale,
            region: stamp.region,
            font: stamp.font,
            color: stamp.color,
            drawn: stamp.drawn.clone(),
            date_text: today.format(),
            show_calendar: false,
            calendar_year: today.year,
            calendar_month: today.month,
            preview: None,
            preview_key: String::new(),
        };
        self.dialog = Some(Dialog::DateStamp);
    }

    pub(crate) fn save_date_stamp(&mut self) {
        if self.date_draft.name.trim().is_empty() || crate::stamp_preset::parse_date(&self.date_draft.date_text).is_none() {
            self.notify("名前と日付を確認してください。");
            return;
        }
        let date_text = self.date_draft.date_text.clone();
        if self.date_draft.drawn.as_ref().is_some_and(|design| crate::stamp_preset::blocks_date_save(design, &date_text)) {
            self.notify("この書体に無い文字があるため、登録しませんでした。");
            return;
        }
        if self.date_draft.editing.is_none() && self.date_stamps.len() >= 32 {
            self.notify("データー印は32件までです。");
            return;
        }
        if let Some(index) = self.date_draft.editing
            && self.date_stamps.get(index).is_none()
        {
            self.notify("編集中のデーター印が見つかりません。");
            return;
        }
        let draft = std::mem::take(&mut self.date_draft);
        let mut stamp = crate::stamp_preset::DateStamp {
            name: draft.name.trim().to_string(),
            file: draft.file,
            data: draft.data,
            scale: draft.scale,
            region: draft.region,
            font: draft.font,
            color: draft.color,
            drawn: draft.drawn,
        };
        crate::stamp_preset::normalize_stamp(&mut stamp);
        let index = if let Some(index) = draft.editing {
            if let Some(slot) = self.date_stamps.get_mut(index) {
                *slot = stamp;
                index
            } else {
                return;
            }
        } else {
            self.date_stamps.push(stamp);
            self.date_stamps.len() - 1
        };
        self.quick_tool = QuickTool::DateStamp(index);
        self.note_stamp_try();
    }
}

/// The custom stamps for `persist` (kept small: base64 of each file).
pub(crate) fn encode(stamps: &[CustomStamp]) -> serde_json::Value {
    serde_json::to_value(stamps).unwrap_or_default()
}

/// Read stamps back; malformed entries are dropped.
pub(crate) fn decode(v: &serde_json::Value) -> Vec<CustomStamp> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| serde_json::from_value::<CustomStamp>(x.clone()).ok())
                .filter(|s| s.data.len() <= MAX_STAMP_BYTES)
                .map(|mut s| {
                    s.scale = crate::stamp_preset::scale_or_hundred(s.scale);
                    s
                })
                .collect()
        })
        .unwrap_or_default()
}

fn drawn_date_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> (bool, bool) {
    let mut scale_rejected = false;
    let mut design = app.date_draft.drawn.clone().unwrap_or_default();
    let mut changed = false;
    let mut pick_top = false;
    let mut pick_bottom = false;
    let mut pick_date = false;
    ui.label(egui::RichText::new(if app.date_draft.editing.is_some() { "データー印を編集" } else { "データー印を登録" }).font(theme::semibold(18.0)));
    ui.add_space(4.0);
    ui.label(egui::RichText::new("円と文字で描く通常のスタンプです。理事長印の承認、暗号化、使用履歴の対象ではありません。配置を始めると、その日の日付が入り、確定した画像は後から変わりません。").small().color(t.text_muted));
    egui::Grid::new("drawn-date").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        let l = ui.label("名前");
        if ui.add(crate::widgets::line(&mut app.date_draft.name).desired_width(220.0)).labelled_by(l.id).changed() {
            changed = true;
        }
        ui.end_row();
        changed |= caption_row(
            ui,
            CaptionRow {
                title: "上段",
                text: &mut design.top,
                size: &mut design.top_size,
                bold: &mut design.top_bold,
                font: &mut design.top_font,
                choices: crate::stamp_preset::TOP_PRESETS,
                salt: "date-top",
                pick_label: "上段のフォントを選択…",
                pick: &mut pick_top,
            },
        );
        changed |= caption_row(
            ui,
            CaptionRow {
                title: "下段",
                text: &mut design.bottom,
                size: &mut design.bottom_size,
                bold: &mut design.bottom_bold,
                font: &mut design.bottom_font,
                choices: crate::stamp_preset::BOTTOM_PRESETS,
                salt: "date-bottom",
                pick_label: "下段のフォントを選択…",
                pick: &mut pick_bottom,
            },
        );
        let l = ui.label("日付（プレビュー）");
        if ui.add(crate::widgets::line(&mut app.date_draft.date_text).desired_width(140.0).hint_text("YYYY/M/D")).labelled_by(l.id).changed() {
            changed = true;
        }
        ui.end_row();
        let l = ui.label("日付の大きさ");
        ui.horizontal(|ui| {
            if ui.add(egui::Slider::new(&mut design.date_size, 0.08..=0.28)).labelled_by(l.id).changed() {
                changed = true;
            }
            if ui.add(widgets::checkbox(&mut design.date_bold, "太字")).changed() {
                design.date_font = crate::stamp_preset::id_for_bold(design.date_font.as_deref(), design.date_bold);
                changed = true;
            }
        });
        ui.end_row();
        ui.label(font_caption(design.date_font.as_deref(), design.date_bold));
        if widgets::pill_button(ui, "日付のフォントを選択…", false).clicked() {
            pick_date = true;
        }
        ui.end_row();
        let l = ui.label("既定倍率 (%)");
        let scale = app.date_draft.scale;
        match scale_percent_field(ui, "date-draft-scale", scale, l.id) {
            ScaleCommit::Set(scale) => {
                app.date_draft.scale = scale;
                changed = true;
            }
            ScaleCommit::Rejected => scale_rejected = true,
            ScaleCommit::Unchanged => {}
        }
        ui.end_row();
        ui.label("印影色");
        ui.horizontal(|ui| {
            for (i, label) in ["R", "G", "B"].into_iter().enumerate() {
                if ui.add(egui::DragValue::new(&mut app.date_draft.color[i]).range(0..=255).prefix(label)).changed() {
                    changed = true;
                }
            }
        });
        ui.end_row();
    });
    ui.collapsing("外枠と位置", |ui| {
        if ui.add(egui::Slider::new(&mut design.diameter, 0.70..=0.98).text("円の大きさ")).changed() {
            changed = true;
        }
        if ui.add(egui::Slider::new(&mut design.ring, 0.012..=0.08).text("線の太さ")).changed() {
            changed = true;
        }
        if ui.add(egui::Slider::new(&mut design.top_y, 0.16..=0.44).text("上段の位置")).changed() {
            changed = true;
        }
        if ui.add(egui::Slider::new(&mut design.date_y, 0.38..=0.64).text("日付の位置")).changed() {
            changed = true;
        }
        if ui.add(egui::Slider::new(&mut design.bottom_y, 0.58..=0.88).text("下段の位置")).changed() {
            changed = true;
        }
        if ui.add(widgets::checkbox(&mut design.rules, "区切り線")).changed() {
            changed = true;
        }
    });
    design.clamp();
    app.date_draft.drawn = Some(design);
    if pick_top || pick_bottom || pick_date {
        let row = if pick_top {
            crate::stamp_preset::DateFontRow::Top
        } else if pick_date {
            crate::stamp_preset::DateFontRow::Date
        } else {
            crate::stamp_preset::DateFontRow::Bottom
        };
        suspend_for_date_font(app, row);
        return (false, false);
    }
    let date_ok = crate::stamp_preset::parse_date(&app.date_draft.date_text);
    if date_ok.is_none() {
        ui.label(
            egui::RichText::new("日付が正しくありません。2026/2/30 のような存在しない日は使えません。").color(egui::Color32::from_rgb(176, 48, 48)),
        );
    }
    ui.horizontal(|ui| {
        if widgets::pill_button(ui, "今日", false).clicked() {
            let today = crate::stamp_preset::japan_today();
            app.date_draft.date_text = today.format();
            app.date_draft.calendar_year = today.year;
            app.date_draft.calendar_month = today.month;
            changed = true;
        }
        if widgets::pill_button(ui, "カレンダー", false).clicked() {
            app.date_draft.show_calendar = !app.date_draft.show_calendar;
        }
    });
    if app.date_draft.show_calendar {
        let (year, month) = (app.date_draft.calendar_year, app.date_draft.calendar_month);
        let action = month_grid(ui, year, month);
        let next = matches!(action, MonthAction::Next);
        match action {
            MonthAction::Prev | MonthAction::Next => {
                let (y, m) = shift_month(year, month, next);
                app.date_draft.calendar_year = y;
                app.date_draft.calendar_month = m;
            }
            MonthAction::Day(day) => {
                let date = crate::stamp_preset::StampDate { year, month, day };
                app.date_draft.date_text = date.format();
            }
            MonthAction::None => {}
        }
    }
    let _ = changed;
    paint_date_preview(ui, app);
    let date_text = app.date_draft.date_text.clone();
    let blocked = app.date_draft.drawn.as_ref().is_some_and(|design| crate::stamp_preset::blocks_date_save(design, &date_text));
    let ok =
        !blocked && !app.date_draft.name.trim().is_empty() && date_ok.is_some() && crate::stamp_preset::checked_scale(app.date_draft.scale).is_some();
    let (save, cancel) = date_actions(ui, app, ok);
    if scale_rejected {
        app.notify("倍率は25%から400%です。");
    }
    (save, cancel)
}

struct CaptionRow<'a> {
    title: &'a str,
    text: &'a mut String,
    size: &'a mut f64,
    bold: &'a mut bool,
    font: &'a mut Option<String>,
    choices: &'a [&'a str],
    salt: &'a str,
    pick_label: &'a str,
    pick: &'a mut bool,
}

fn caption_row(ui: &mut egui::Ui, row: CaptionRow<'_>) -> bool {
    let mut changed = false;
    let l = ui.label(row.title);
    ui.horizontal(|ui| {
        if ui.add(crate::widgets::line(row.text).desired_width(150.0)).labelled_by(l.id).changed() {
            changed = true;
        }
        egui::ComboBox::from_id_salt(row.salt).selected_text("候補").width(64.0).show_ui(ui, |ui| {
            for choice in row.choices {
                let label = if choice.is_empty() { "空欄" } else { *choice };
                if ui.selectable_label(*row.text == *choice, label).clicked() {
                    *row.text = (*choice).to_string();
                    changed = true;
                }
            }
        });
    });
    ui.end_row();
    ui.label(font_caption(row.font.as_deref(), *row.bold));
    if widgets::pill_button(ui, row.pick_label, false).clicked() {
        *row.pick = true;
    }
    ui.end_row();
    ui.label("");
    ui.horizontal(|ui| {
        if ui.add(egui::Slider::new(row.size, 0.05..=0.22).text("大きさ")).changed() {
            changed = true;
        }
        if ui.add(widgets::checkbox(row.bold, "太字")).changed() {
            *row.font = crate::stamp_preset::id_for_bold(row.font.as_deref(), *row.bold);
            changed = true;
        }
    });
    ui.end_row();
    changed
}

fn font_caption(id: Option<&str>, bold: bool) -> String {
    let Some(id) = id else {
        return if bold { "BIZ UDPゴシック 太字（既定）".into() } else { "BIZ UDPゴシック 標準（既定）".into() };
    };
    match pdfcraft_fonts::catalog::get(id) {
        Some(face) if face.available() && face.can_preview() => face.display_name.to_string(),
        _ => "指定フォントが利用できない".into(),
    }
}

fn note_sentence(note: &crate::stamp_preset::LineNote) -> String {
    match note {
        crate::stamp_preset::LineNote::Substituted { row } => {
            format!("{row}: 指定フォントが利用できないため、BIZ UDPゴシックで表示しています。フォントIDは変えていません。")
        }
        crate::stamp_preset::LineNote::Missing { row, chars } => {
            format!("{row}: この書体には「{chars}」がありません。別の書体を選ぶまで登録できません。")
        }
        crate::stamp_preset::LineNote::Unreadable { row, stored: true } => format!("{row}: 指定フォントが利用できないため、この文字は描けません。"),
        crate::stamp_preset::LineNote::Unreadable { row, stored: false } => format!("{row}: この文字を描けるフォントがありません。"),
    }
}

pub(crate) fn suspend_for_date_font(app: &mut PdfCraftApp, row: crate::stamp_preset::DateFontRow) {
    let Some(drawn) = app.date_draft.drawn.clone() else { return };
    let (current, text) = match row {
        crate::stamp_preset::DateFontRow::Top => (drawn.top_font, drawn.top),
        crate::stamp_preset::DateFontRow::Date => (drawn.date_font, app.date_draft.date_text.clone()),
        crate::stamp_preset::DateFontRow::Bottom => (drawn.bottom_font, drawn.bottom),
    };
    let text: String = text.chars().take(32).collect();
    app.date_font_row = Some(row);
    app.font_return = Some(Dialog::DateStamp);
    app.font_choice = None;
    app.font_hold = true;
    app.font_picker =
        crate::font_picker::State::opening_for(current.as_deref(), pdfcraft_fonts::catalog::Offer { raster: true, text, standard: false });
}

pub(crate) fn apply_date_font_choice(app: &mut PdfCraftApp) {
    let Some(row) = app.date_font_row.take() else { return };
    let Some(choice) = app.font_choice.take() else { return };
    let text = match row {
        crate::stamp_preset::DateFontRow::Top => app.date_draft.drawn.as_ref().map(|design| design.top.clone()).unwrap_or_default(),
        crate::stamp_preset::DateFontRow::Date => app.date_draft.date_text.clone(),
        crate::stamp_preset::DateFontRow::Bottom => app.date_draft.drawn.as_ref().map(|design| design.bottom.clone()).unwrap_or_default(),
    };
    let offer = pdfcraft_fonts::catalog::Offer { raster: true, text, standard: false };
    let Some(face) = pdfcraft_fonts::catalog::get(&choice).filter(|face| pdfcraft_fonts::catalog::face_offered(face, &offer)) else { return };
    let bold = face.weight == 700;
    let Some(drawn) = app.date_draft.drawn.as_mut() else { return };
    match row {
        crate::stamp_preset::DateFontRow::Top => {
            drawn.top_font = Some(choice);
            drawn.top_bold = bold;
        }
        crate::stamp_preset::DateFontRow::Date => {
            drawn.date_font = Some(choice);
            drawn.date_bold = bold;
        }
        crate::stamp_preset::DateFontRow::Bottom => {
            drawn.bottom_font = Some(choice);
            drawn.bottom_bold = bold;
        }
    }
}

fn paint_date_preview(ui: &mut egui::Ui, app: &mut PdfCraftApp) {
    let d = &mut app.date_draft;
    let key = format!("{}|{:?}|{:?}", d.date_text, d.color, d.drawn);
    if d.preview_key != key {
        d.preview_key = key;
        let drawn = d.drawn.clone();
        let color = d.color;
        let region = d.region;
        let font = d.font;
        let base = d.data.clone();
        let date_text = d.date_text.clone();
        d.preview = crate::stamp_preset::parse_date(&date_text).and_then(|date| {
            let overlay = crate::stamp_preset::DateOverlay {
                date,
                entry: date.format(),
                region,
                font,
                color,
                base: base.to_vec(),
                drawn,
                show_calendar: false,
                calendar_year: date.year,
                calendar_month: date.month,
            };
            crate::stamp_preset::render_overlay(&base, &overlay)
        });
    }
    if d.preview.is_none() && crate::stamp_preset::parse_date(&d.date_text).is_some() {
        let notes = d.drawn.as_ref().map(|design| crate::stamp_preset::line_notes(design, &d.date_text)).unwrap_or_default();
        if notes.is_empty() {
            ui.label(egui::RichText::new("この文字を描ける日本語フォントがありません。").color(egui::Color32::from_rgb(176, 48, 48)));
        }
    }
    if let Some(design) = d.drawn.clone() {
        for note in crate::stamp_preset::line_notes(&design, &d.date_text) {
            ui.label(egui::RichText::new(note_sentence(&note)).color(egui::Color32::from_rgb(176, 48, 48)));
        }
    }
    if let Some((width, height, pixels)) = d.preview.clone()
        && pixels.len() == (width as usize).saturating_mul(height as usize).saturating_mul(4)
    {
        let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &pixels);
        let texture = ui.ctx().load_texture("date-stamp-preview", image, egui::TextureOptions::LINEAR);
        let fit = 280.0 / (width.max(height) as f32).max(1.0);
        ui.image((texture.id(), egui::vec2(width as f32 * fit, height as f32 * fit)));
    }
}

fn date_actions(ui: &mut egui::Ui, app: &mut PdfCraftApp, ok: bool) -> (bool, bool) {
    let (mut save, mut cancel, mut trial) = (false, false, false);
    ui.add_space(8.0);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, "登録", true)).inner.clicked() {
            save = true;
        }
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, "試し置き", false)).inner.clicked() {
            trial = true;
            save = true;
        }
        if widgets::pill_button(ui, "キャンセル", false).clicked() {
            cancel = true;
        }
    });
    if trial {
        app.stamp_try = true;
    }
    (save, cancel)
}

pub(crate) fn date_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> (bool, bool) {
    if app.date_draft.drawn.is_some() {
        return drawn_date_body(ui, app, t);
    }
    let mut scale_rejected = false;
    let d = &mut app.date_draft;
    ui.label(egui::RichText::new(if d.editing.is_some() { "データー印を編集" } else { "データー印を登録" }).font(theme::semibold(18.0)));
    ui.add_space(4.0);
    ui.label(egui::RichText::new("通常のスタンプです。理事長印の承認や暗号化は行いません。元の画像は変更しません。").small().color(t.text_muted));
    ui.label(egui::RichText::new(format!("元画像: {}", d.file)).small().color(t.text_faint));
    let mut changed = false;
    egui::Grid::new("date-stamp").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        let l = ui.label("名前");
        if ui.add(crate::widgets::line(&mut d.name).desired_width(220.0)).labelled_by(l.id).changed() {
            changed = true;
        }
        ui.end_row();
        let l = ui.label("既定倍率 (%)");
        let scale = d.scale;
        match scale_percent_field(ui, "date-draft-scale", scale, l.id) {
            ScaleCommit::Set(scale) => d.scale = scale,
            ScaleCommit::Rejected => scale_rejected = true,
            ScaleCommit::Unchanged => {}
        }
        ui.end_row();
        let l = ui.label("日付（プレビュー）");
        if ui.add(crate::widgets::line(&mut d.date_text).desired_width(140.0).hint_text("YYYY/M/D")).labelled_by(l.id).changed() {
            changed = true;
        }
        ui.end_row();
        let l = ui.label("文字の大きさ");
        if ui.add(egui::Slider::new(&mut d.font, 0.05..=0.50).text("画像の高さに対する割合")).labelled_by(l.id).changed() {
            changed = true;
        }
        ui.end_row();
        for (i, label) in ["位置 X", "位置 Y", "幅", "高さ"].into_iter().enumerate() {
            let l = ui.label(label);
            if ui.add(egui::Slider::new(&mut d.region[i], 0.0..=1.0)).labelled_by(l.id).changed() {
                changed = true;
            }
            ui.end_row();
        }
        ui.label("文字色");
        ui.horizontal(|ui| {
            for (i, label) in ["R", "G", "B"].into_iter().enumerate() {
                if ui.add(egui::DragValue::new(&mut d.color[i]).range(0..=255).prefix(label)).changed() {
                    changed = true;
                }
            }
        });
        ui.end_row();
    });
    d.region = crate::stamp_preset::clamp_region(d.region);
    d.font = crate::stamp_preset::clamp_font(d.font);
    let date_ok = crate::stamp_preset::parse_date(&d.date_text);
    if date_ok.is_none() {
        ui.label(
            egui::RichText::new("日付が正しくありません。2026/2/30 のような存在しない日は使えません。").color(egui::Color32::from_rgb(176, 48, 48)),
        );
    }
    ui.horizontal(|ui| {
        if widgets::pill_button(ui, "今日", false).clicked() {
            let today = crate::stamp_preset::japan_today();
            d.date_text = today.format();
            d.calendar_year = today.year;
            d.calendar_month = today.month;
            changed = true;
        }
        if widgets::pill_button(ui, "カレンダー", false).clicked() {
            d.show_calendar = !d.show_calendar;
        }
    });
    if d.show_calendar {
        let (year, month) = (d.calendar_year, d.calendar_month);
        let action = month_grid(ui, year, month);
        match action {
            MonthAction::Prev | MonthAction::Next => {
                let (y, m) = shift_month(year, month, matches!(action, MonthAction::Next));
                d.calendar_year = y;
                d.calendar_month = m;
            }
            MonthAction::Day(day) => {
                let date = crate::stamp_preset::StampDate { year, month, day };
                d.date_text = date.format();
                changed = true;
            }
            MonthAction::None => {}
        }
    }
    let date_ok = crate::stamp_preset::parse_date(&d.date_text);
    let key = format!("{}|{:.4}|{:?}|{:?}|{}", d.date_text, d.font, d.region, d.color, d.data.len());
    if changed || d.preview_key != key {
        d.preview_key = key;
        d.preview = date_ok.and_then(|date| {
            let overlay = crate::stamp_preset::DateOverlay {
                date,
                entry: date.format(),
                region: d.region,
                font: d.font,
                color: d.color,
                base: d.data.to_vec(),
                drawn: d.drawn.clone(),
                show_calendar: false,
                calendar_year: date.year,
                calendar_month: date.month,
            };
            crate::stamp_preset::render_overlay(&d.data, &overlay)
        });
    }
    if let Some((width, height, pixels)) = d.preview.clone()
        && pixels.len() == (width as usize).saturating_mul(height as usize).saturating_mul(4)
    {
        let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &pixels);
        let texture = ui.ctx().load_texture("date-stamp-preview", image, egui::TextureOptions::LINEAR);
        let fit = 160.0 / (width.max(height) as f32).max(1.0);
        ui.image((texture.id(), egui::vec2(width as f32 * fit, height as f32 * fit)));
    }
    ui.add_space(8.0);
    let ok = !d.name.trim().is_empty() && date_ok.is_some() && crate::stamp_preset::checked_scale(d.scale).is_some();
    let (mut save, mut cancel, mut trial) = (false, false, false);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, "登録", true)).inner.clicked() {
            save = true;
        }
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, "試し置き", false)).inner.clicked() {
            trial = true;
            save = true;
        }
        if widgets::pill_button(ui, "キャンセル", false).clicked() {
            cancel = true;
        }
    });
    if trial {
        app.stamp_try = true;
    }
    if scale_rejected {
        app.notify("倍率は25%から400%です。");
    }
    (save, cancel)
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
    /// Percent of [`Self::nominal`]. Updated by the slider and by a corner drag.
    pub scale: f64,
    /// Width and height, in points, at 100%.
    pub nominal: (f64, f64),
    pub source: PlaceSource,
    pub circular: bool,
    pub date_overlay: Option<crate::stamp_preset::DateOverlay>,
}

/// Which library entry a placement came from, so 「この倍率を既定にする」 can store it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceSource {
    Custom(usize),
    Chairman(usize),
    Date(usize),
}

struct Armed {
    name: String,
    file: String,
    data: Arc<Vec<u8>>,
    source_page: usize,
    chairman: bool,
    scale: f64,
    source: PlaceSource,
    overlay: Option<crate::stamp_preset::DateOverlay>,
}

#[derive(Clone, Copy, Debug)]
pub enum StampDrag {
    Move { from: egui::Pos2, origin: [f64; 4] },
    Resize { handle: (i8, i8), from: egui::Pos2, origin: [f64; 4] },
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
                    place.scale = crate::stamp_preset::percent_of(place.rect, place.nominal.0);
                }
            }
            None => {}
        }
    }
    let consumed = place.drag.is_some();
    if !down && let Some(place) = view.stamp_place.as_mut() {
        let redraw = place.drag.is_some() && place.date_overlay.as_ref().is_some_and(|overlay| overlay.drawn.is_some());
        place.drag = None;
        if redraw {
            let _ = rebake_date(place);
        }
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

/// The stamp armed from the palette, when this document may receive one.
fn armed_stamp_name(app: &PdfCraftApp) -> Option<String> {
    if !app.active_ids().is_some_and(|(_, id)| app.session.allows_stamp(id)) {
        return None;
    }
    match app.quick_tool {
        QuickTool::CustomStamp(i) => app.custom_stamps.get(i).map(|stamp| stamp.name.clone()),
        QuickTool::ChairmanSeal(i) => app.seal_store.library.seals.get(i).map(|seal| seal.name.clone()),
        QuickTool::DateStamp(i) => app.date_stamps.get(i).map(|stamp| format!("{}（データー印）", stamp.name)),
        _ => None,
    }
}

pub(crate) fn stamp_bar(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let Some(index) = app.active else { return };
    if app.views.get(index).is_some_and(|view| view.stamp_place.is_some()) {
        place_bar(app, ui, index);
        return;
    }
    if let Some(name) = armed_stamp_name(app) {
        let t = Tokens::get(ui.ctx());
        egui::Panel::top("stamp_arm_bar").exact_size(44.0).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 4))).show(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("配置中: {name}"));
                    ui.label("ページをクリックすると仮置きします。");
                    if widgets::pill_button(ui, "キャンセル", false).clicked() {
                        app.quick_tool = QuickTool::Select;
                    }
                });
            },
        );
        return;
    }
    owned_stamp_opacity(app, ui, index);
}

fn place_bar(app: &mut PdfCraftApp, ui: &mut egui::Ui, index: usize) {
    let t = Tokens::get(ui.ctx());
    let name = app.views[index].stamp_place.as_ref().map(|place| place.name.clone()).unwrap_or_default();
    let dated = app.views[index].stamp_place.as_ref().is_some_and(|place| place.date_overlay.is_some());
    let calendar = app.views[index].stamp_place.as_ref().is_some_and(|place| place.date_overlay.as_ref().is_some_and(|date| date.show_calendar));
    let height = if calendar {
        248.0
    } else if dated {
        108.0
    } else {
        72.0
    };
    let mut confirm = false;
    let mut cancel = false;
    let mut save_scale = false;
    let mut notice: Option<String> = None;
    let mut date_bad = false;
    egui::Panel::top("stamp_place_bar").exact_size(height).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 4))).show(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("配置中: {name}"));
                let mut opacity = app.views[index].stamp_place.as_ref().map(|place| place.opacity).unwrap_or(1.0);
                if ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("透明度")).changed()
                    && let Some(place) = app.views[index].stamp_place.as_mut()
                {
                    place.opacity = opacity.clamp(0.0, 1.0);
                }
                let committed = app.views[index].stamp_place.as_ref().map(|place| place.scale).unwrap_or(100.0);
                let scale_label = ui.label("倍率");
                match scale_percent_field(ui, ("place-scale", index), committed, scale_label.id) {
                    ScaleCommit::Set(scale) => {
                        if let Some(place) = app.views[index].stamp_place.as_mut() {
                            place.rect = crate::stamp_preset::rect_at_scale(place.rect, place.nominal, scale);
                            place.scale = scale;
                            if place.date_overlay.as_ref().is_some_and(|overlay| overlay.drawn.is_some()) && !rebake_date(place) {
                                notice = Some("日付を画像にできませんでした。".into());
                            }
                        }
                    }
                    ScaleCommit::Rejected => notice = Some("倍率は25%から400%です。".into()),
                    ScaleCommit::Unchanged => {}
                }
                if let Some(place) = app.views[index].stamp_place.as_ref() {
                    ui.label(crate::stamp_preset::size_label(place.rect, place.circular));
                }
                if widgets::pill_button(ui, "この倍率を既定にする", false).clicked() {
                    save_scale = true;
                }
                if widgets::pill_button(ui, "確定", true).clicked() {
                    confirm = true;
                }
                if widgets::pill_button(ui, "キャンセル", false).clicked() {
                    cancel = true;
                }
            });
            if dated {
                ui.horizontal(|ui| {
                    ui.label("日付");
                    let mut text = app.views[index]
                        .stamp_place
                        .as_ref()
                        .and_then(|place| place.date_overlay.as_ref().map(|date| date.entry.clone()))
                        .unwrap_or_default();
                    let edited = ui.add(crate::widgets::line(&mut text).desired_width(110.0).hint_text("YYYY/M/D"));
                    if edited.changed() {
                        let parsed = crate::stamp_preset::parse_date(&text);
                        if let Some(place) = app.views[index].stamp_place.as_mut()
                            && let Some(overlay) = place.date_overlay.as_mut()
                        {
                            overlay.entry = text.clone();
                            if let Some(date) = parsed {
                                overlay.date = date;
                                overlay.calendar_year = date.year;
                                overlay.calendar_month = date.month;
                            }
                        }
                        if parsed.is_some()
                            && let Some(place) = app.views[index].stamp_place.as_mut()
                            && !rebake_date(place)
                        {
                            notice = Some("日付を画像にできませんでした。".into());
                        }
                    }
                    date_bad = crate::stamp_preset::parse_date(&text).is_none();
                    if date_bad {
                        ui.label(egui::RichText::new("日付が正しくありません").color(egui::Color32::from_rgb(176, 48, 48)));
                    }
                    if widgets::pill_button(ui, "今日", false).clicked() {
                        let today = crate::stamp_preset::japan_today();
                        if let Some(place) = app.views[index].stamp_place.as_mut()
                            && let Some(overlay) = place.date_overlay.as_mut()
                        {
                            overlay.date = today;
                            overlay.entry = today.format();
                            overlay.calendar_year = today.year;
                            overlay.calendar_month = today.month;
                        }
                        if let Some(place) = app.views[index].stamp_place.as_mut()
                            && !rebake_date(place)
                        {
                            notice = Some("日付を画像にできませんでした。".into());
                        }
                    }
                    if widgets::pill_button(ui, "カレンダー", false).clicked()
                        && let Some(place) = app.views[index].stamp_place.as_mut()
                        && let Some(overlay) = place.date_overlay.as_mut()
                    {
                        overlay.show_calendar = !overlay.show_calendar;
                    }
                });
                if calendar
                    && let Some((year, month)) = app.views[index]
                        .stamp_place
                        .as_ref()
                        .and_then(|place| place.date_overlay.as_ref().map(|date| (date.calendar_year, date.calendar_month)))
                {
                    let action = month_grid(ui, year, month);
                    match action {
                        MonthAction::Prev | MonthAction::Next => {
                            let (y, m) = shift_month(year, month, matches!(action, MonthAction::Next));
                            if let Some(place) = app.views[index].stamp_place.as_mut()
                                && let Some(overlay) = place.date_overlay.as_mut()
                            {
                                overlay.calendar_year = y;
                                overlay.calendar_month = m;
                            }
                        }
                        MonthAction::Day(day) => {
                            let date = crate::stamp_preset::StampDate { year, month, day };
                            if let Some(place) = app.views[index].stamp_place.as_mut()
                                && let Some(overlay) = place.date_overlay.as_mut()
                            {
                                overlay.date = date;
                                overlay.entry = date.format();
                            }
                            if let Some(place) = app.views[index].stamp_place.as_mut()
                                && !rebake_date(place)
                            {
                                notice = Some("日付を画像にできませんでした。".into());
                            }
                        }
                        MonthAction::None => {}
                    }
                }
            }
        },
    );
    if let Some(notice) = notice {
        app.notify(notice);
    }
    if save_scale {
        app.save_place_scale();
    }
    if confirm {
        if date_bad {
            app.notify("日付が正しくありません。");
        } else {
            app.ask_stamp_confirm();
        }
    }
    if cancel {
        app.views[index].stamp_place = None;
        app.quick_tool = QuickTool::Select;
    }
}

enum MonthAction {
    None,
    Prev,
    Next,
    Day(u32),
}

fn shift_month(year: i32, month: u32, next: bool) -> (i32, u32) {
    if next {
        if month >= 12 { (year.saturating_add(1), 1) } else { (year, month + 1) }
    } else if month <= 1 {
        (year.saturating_sub(1), 12)
    } else {
        (year, month - 1)
    }
}

fn month_grid(ui: &mut egui::Ui, year: i32, month: u32) -> MonthAction {
    let mut action = MonthAction::None;
    ui.horizontal(|ui| {
        if ui.button("前の月").clicked() {
            action = MonthAction::Prev;
        }
        ui.label(format!("{year}年{month}月"));
        if ui.button("次の月").clicked() {
            action = MonthAction::Next;
        }
    });
    ui.horizontal(|ui| {
        for label in ["日", "月", "火", "水", "木", "金", "土"] {
            ui.add_sized([28.0, 18.0], egui::Label::new(label));
        }
    });
    let first = crate::stamp_preset::weekday(crate::stamp_preset::StampDate { year, month, day: 1 });
    let days = crate::stamp_preset::days_in_month(year, month);
    let mut day = 1u32;
    for week in 0..6 {
        if day > days {
            break;
        }
        ui.horizontal(|ui| {
            for col in 0..7u32 {
                if (week > 0 || col >= first) && day <= days {
                    let n = day;
                    if ui.add_sized([28.0, 22.0], egui::Button::new(n.to_string())).clicked() {
                        action = MonthAction::Day(n);
                    }
                    day += 1;
                } else {
                    ui.add_sized([28.0, 22.0], egui::Label::new(""));
                }
            }
        });
    }
    action
}

fn rebake_date(place: &mut StampPlace) -> bool {
    let Some(overlay) = place.date_overlay.as_ref() else { return true };
    match crate::stamp_preset::render_placed(overlay, place.rect) {
        Some(pixels) => {
            place.pixels = Some(pixels);
            true
        }
        None => false,
    }
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

pub(crate) struct ArmedInputs<'a> {
    pub tool: QuickTool,
    pub custom_stamps: &'a [CustomStamp],
    pub seals: &'a [crate::seal_store::SealImage],
    pub date_stamps: &'a [crate::stamp_preset::DateStamp],
    pub seal_locked: bool,
}

pub(crate) fn build_stamp_place(inputs: ArmedInputs<'_>, page: usize, at: [f64; 2], rotation: u16) -> Result<StampPlace, String> {
    let armed = match inputs.tool {
        QuickTool::CustomStamp(i) => inputs.custom_stamps.get(i).map(|stamp| Armed {
            name: stamp.name.clone(),
            file: stamp.file.clone(),
            data: stamp.data.clone(),
            source_page: stamp.page,
            chairman: false,
            scale: stamp.scale,
            source: PlaceSource::Custom(i),
            overlay: None,
        }),
        QuickTool::ChairmanSeal(i) => inputs.seals.get(i).map(|seal| Armed {
            name: seal.name.clone(),
            file: seal.file.clone(),
            data: std::sync::Arc::new(seal.data.clone()),
            source_page: 0,
            chairman: true,
            scale: seal.scale,
            source: PlaceSource::Chairman(i),
            overlay: None,
        }),
        QuickTool::DateStamp(i) => inputs.date_stamps.get(i).map(|stamp| {
            let today = crate::stamp_preset::japan_today();
            Armed {
                name: stamp.name.clone(),
                file: stamp.file.clone(),
                data: stamp.data.clone(),
                source_page: 0,
                chairman: false,
                scale: stamp.scale,
                source: PlaceSource::Date(i),
                overlay: Some(crate::stamp_preset::DateOverlay {
                    date: today,
                    entry: today.format(),
                    region: stamp.region,
                    font: stamp.font,
                    color: stamp.color,
                    base: stamp.data.as_ref().clone(),
                    drawn: stamp.drawn.clone(),
                    show_calendar: false,
                    calendar_year: today.year,
                    calendar_month: today.month,
                }),
            }
        }),
        _ => None,
    };
    let Some(armed) = armed else { return Err("スタンプが選ばれていません。".into()) };
    if armed.chairman && inputs.seal_locked {
        return Err("印影ファイルを復号できないため、理事長印は使えません。".into());
    }
    let drawn = armed.overlay.as_ref().is_some_and(|overlay| overlay.drawn.is_some());
    let pixels = if drawn { None } else { crate::seal_store::preview_rgba(&armed.data) };
    let dims =
        if drawn { Some((crate::stamp_preset::DESIGN_SIDE, crate::stamp_preset::DESIGN_SIDE)) } else { pixels.as_ref().map(|(w, h, _)| (*w, *h)) };
    let nominal = crate::stamp_preset::nominal_size(rotation, dims);
    let scale = crate::stamp_preset::scale_or_hundred(armed.scale);
    let circular = drawn || pixels.as_ref().is_some_and(|(w, h, rgba)| crate::stamp_preset::looks_circular(*w, *h, rgba));
    let mut place = StampPlace {
        page,
        rect: crate::stamp_preset::place_rect(at, nominal, scale),
        name: armed.name,
        file: armed.file,
        data: armed.data,
        source_page: armed.source_page,
        opacity: 1.0,
        chairman: armed.chairman,
        pixels,
        drag: None,
        scale,
        nominal,
        source: armed.source,
        circular,
        date_overlay: armed.overlay,
    };
    if place.date_overlay.is_some() && !rebake_date(&mut place) {
        return Err("日付を画像にできませんでした。".into());
    }
    Ok(place)
}

impl PdfCraftApp {
    /// Put the armed stamp on the page without changing the document.
    #[cfg(test)]
    pub(crate) fn begin_stamp_place(&mut self, view_index: usize, page: usize, at: [f64; 2], rotation: u16) -> Result<(), String> {
        if let Some(why) = self.stamp_refusal(view_index) {
            return Err(why);
        }
        let place = build_stamp_place(
            ArmedInputs {
                tool: self.quick_tool,
                custom_stamps: &self.custom_stamps,
                seals: &self.seal_store.library.seals,
                date_stamps: &self.date_stamps,
                seal_locked: self.seal_store.locked(),
            },
            page,
            at,
            rotation,
        )?;
        let Some(view) = self.views.get_mut(view_index) else { return Err("ページがありません".into()) };
        view.stamp_place = Some(place);
        Ok(())
    }

    pub(crate) fn save_place_scale(&mut self) {
        let Some(index) = self.active else { return };
        let Some(place) = self.views[index].stamp_place.clone() else { return };
        let Some(scale) = crate::stamp_preset::checked_scale(place.scale) else {
            self.notify("倍率は25%から400%です。このサイズは既定にできません。");
            return;
        };
        match place.source {
            PlaceSource::Custom(i) => {
                if let Some(stamp) = self.custom_stamps.get_mut(i) {
                    stamp.scale = scale;
                    self.notify("この倍率を既定にしました。");
                }
            }
            PlaceSource::Chairman(i) => match self.seal_store.set_scale(i, scale) {
                Ok(()) => self.notify("この倍率を既定にしました。"),
                Err(error) => self.notify(error),
            },
            PlaceSource::Date(i) => {
                if let Some(stamp) = self.date_stamps.get_mut(i) {
                    stamp.scale = scale;
                    self.notify("この倍率を既定にしました。");
                }
            }
        }
    }

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
        let Some(mut place) = self.views[index].stamp_place.clone() else { return };
        if let Some(why) = self.stamp_refusal(index) {
            self.notify(why);
            return;
        }
        if place.date_overlay.as_ref().is_some_and(|overlay| overlay.drawn.is_some()) && !rebake_date(&mut place) {
            self.notify("日付を画像にできませんでした。");
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
        let bytes = if place.date_overlay.is_some() {
            let Some((width, height, pixels)) = place.pixels.clone() else {
                self.notify("日付を画像にできませんでした。");
                return;
            };
            let Some(png) = crate::stamp_preset::encode_rgba_png(width, height, &pixels) else {
                self.notify("日付を画像にできませんでした。");
                return;
            };
            std::sync::Arc::new(png)
        } else {
            place.data.clone()
        };
        let edit = pdfcraft_engine::Edit::AddCustomStamp {
            page: place.page,
            rect: place.rect,
            name: place.name.clone(),
            file: pdfcraft_engine::MarkFile { name: place.file.clone(), bytes, page: place.source_page },
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
        self.quick_tool = QuickTool::Select;
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
    ///
    /// `false` means that write failed and the document change was rolled back (or could not be).
    /// The caller must not announce the undo or redo as successful. A general stamp, which has no
    /// seal-log row, returns `true` and keeps the document change.
    pub(crate) fn note_seal_history_step(&mut self, id: pdfcraft_engine::DocId, undo: bool, label: &str) -> bool {
        let deletion = match label {
            "Add stamp" => false,
            "Delete comment" => true,
            _ => return true,
        };
        let present = self.session.present_owned_stamps(id);
        let Some(use_index) = self.seal_uses.iter().rposition(|use_| {
            let here = present.iter().any(|refer| refer.num == use_.num && refer.generation == use_.generation);
            if deletion { use_.active != undo && here == undo } else { use_.active == undo && here != undo }
        }) else {
            return true;
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
            return false;
        }
        if let Some(use_) = self.seal_uses.get_mut(use_index) {
            use_.active = if deletion { undo } else { !undo };
        }
        true
    }
}

#[cfg(test)]
mod scale_field_tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use sha2::Digest;

    struct PolicyRestore((bool, bool, bool));

    impl Drop for PolicyRestore {
        fn drop(&mut self) {
            crate::output_policy::set_blocked(self.0.0);
            crate::output_policy::set_save_as_allowed(self.0.1);
            crate::output_policy::set_print_allowed(self.0.2);
        }
    }

    fn policy() -> PolicyRestore {
        PolicyRestore((crate::output_policy::blocked(), crate::output_policy::save_as_allowed(), crate::output_policy::print_allowed()))
    }

    fn tiny_png() -> Vec<u8> {
        let mut png = Vec::new();
        image::RgbImage::from_pixel(80, 40, image::Rgb([20, 120, 220]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        png
    }

    fn harness() -> Harness<'static, crate::PdfCraftApp> {
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
            let mut app = crate::PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            let bytes = app.session.create_blank(200.0, 300.0, 1).unwrap();
            app.open_bytes("opened.pdf", None, bytes.to_vec()).unwrap();
            app.set_view_only(true);
            app.grant_stamp();
            app.grant_edit_text();
            app.grant_save_as();
            app.set_option("zoom", "100").unwrap();
            app
        });
        h.run_steps(4);
        h
    }

    fn percent_field<'a>(h: &'a Harness<'static, crate::PdfCraftApp>, label: &'a str) -> egui_kittest::Node<'a> {
        let role = egui::accesskit::Role::TextInput;
        if let Some(node) = h.query_by_role_and_label(role, label) {
            return node;
        }
        let spin = egui::accesskit::Role::SpinButton;
        if let Some(node) = h.query_by_role_and_label(spin, label) {
            return node;
        }
        // The current spin button has no accessible name. Its text is the percent, with or without the % suffix.
        h.get_by(move |n| {
            n.role() == spin
                && n.label().is_none()
                && n.value().is_some_and(|v| {
                    let t = v.trim_end_matches('%');
                    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit() || c == '.')
                })
        })
    }

    fn shown(h: &Harness<'static, crate::PdfCraftApp>, label: &str) -> String {
        percent_field(h, label).value().unwrap_or_default()
    }

    fn focus_percent(h: &mut Harness<'static, crate::PdfCraftApp>, label: &str) {
        let field = percent_field(h, label);
        field.click();
        field.focus();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(1);
    }

    fn type_chars(h: &mut Harness<'static, crate::PdfCraftApp>, label: &str, text: &str) {
        for ch in text.chars() {
            percent_field(h, label).type_text(&ch.to_string());
            h.run_steps(2);
        }
    }

    /// A single digit on the way to 80 must stay on screen and must not become 25 or 100.
    #[test]
    fn a_partial_percent_is_not_clamped_in_the_register_dialog() {
        let _policy = policy();
        let mut h = harness();
        h.state_mut().start_custom_stamp("logo.png".into(), tiny_png());
        h.run_steps(3);
        focus_percent(&mut h, "既定倍率 (%)");
        type_chars(&mut h, "既定倍率 (%)", "8");
        let scale = h.state().stamp_draft.scale;
        let text = shown(&h, "既定倍率 (%)");
        assert!((scale - 100.0).abs() < 1e-6, "typing 8 committed {scale}; field shows {text}");
        assert_eq!(text, "8", "partial input was replaced by {text}");
        type_chars(&mut h, "既定倍率 (%)", "0");
        let scale = h.state().stamp_draft.scale;
        let text = shown(&h, "既定倍率 (%)");
        assert!((scale - 100.0).abs() < 1e-6, "80 was committed early as {scale}; field shows {text}");
        assert_eq!(text, "80", "field shows {text}");
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        let scale = h.state().stamp_draft.scale;
        assert!((scale - 80.0).abs() < 1e-6, "Enter left {scale}");
        assert_eq!(shown(&h, "既定倍率 (%)"), "80");
    }

    /// The placement bar reloads the committed percent every frame. A typed 8 must survive that.
    #[test]
    fn a_partial_percent_stays_on_the_place_bar_until_enter() {
        let _policy = policy();
        let mut h = harness();
        h.state_mut().start_custom_stamp("logo.png".into(), tiny_png());
        h.state_mut().save_custom_stamp();
        h.state_mut().dialog = None;
        h.state_mut().quick_tool = crate::QuickTool::CustomStamp(0);
        h.state_mut().begin_stamp_place(0, 0, [100.0, 150.0], 0).unwrap();
        h.run_steps(3);
        focus_percent(&mut h, "倍率");
        type_chars(&mut h, "倍率", "8");
        let scale = h.state().views[0].stamp_place.as_ref().unwrap().scale;
        let text = shown(&h, "倍率");
        assert!((scale - 100.0).abs() < 1e-6, "typing 8 changed the placement to {scale}; field shows {text}");
        assert_eq!(text, "8", "partial input was replaced by {text}");
        type_chars(&mut h, "倍率", "0");
        let scale = h.state().views[0].stamp_place.as_ref().unwrap().scale;
        let text = shown(&h, "倍率");
        assert!((scale - 100.0).abs() < 1e-6, "80 was applied early as {scale}; field shows {text}");
        assert_eq!(text, "80");
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        let place = h.state().views[0].stamp_place.as_ref().unwrap();
        assert!((place.scale - 80.0).abs() < 1e-6, "{}", place.scale);
        assert!((place.rect[2] - place.rect[0] - 64.0).abs() < 1e-6, "{:?}", place.rect);
        assert_eq!(shown(&h, "倍率"), "80");
    }

    fn commit_percent(h: &mut Harness<'static, crate::PdfCraftApp>, label: &str, text: &str) {
        focus_percent(h, label);
        type_chars(h, label, text);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
    }

    fn close_enough(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-6
    }

    /// Arrows, an empty box, Ctrl+A, Backspace, Enter, blur, the saved default, a corner's percent,
    /// date stamps, chairman seals, undo and the original file.
    #[test]
    fn typed_percent_commits_for_arrows_blur_dates_seals_and_undo() {
        let _policy = policy();
        let dir = std::env::temp_dir().join(format!("swiftpdf-scale-type-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opened.pdf");
        let bytes = {
            let app = crate::PdfCraftApp::new();
            app.session.create_blank(200.0, 300.0, 1).unwrap().to_vec()
        };
        std::fs::write(&path, &bytes).unwrap();
        let hash = sha2::Sha256::digest(&bytes);
        let path_s = path.to_string_lossy().into_owned();
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
            let mut app = crate::PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("opened.pdf", Some(path_s), bytes).unwrap();
            app.set_view_only(true);
            app.grant_stamp();
            app.grant_edit_text();
            app.grant_save_as();
            app.set_option("zoom", "100").unwrap();
            app
        });
        h.run_steps(4);
        let png = tiny_png();
        h.state_mut().start_custom_stamp("logo.png".into(), png.clone());
        h.run_steps(3);

        focus_percent(&mut h, "既定倍率 (%)");
        h.key_press(egui::Key::ArrowUp);
        h.run_steps(2);
        assert!(close_enough(h.state().stamp_draft.scale, 101.0), "{}", h.state().stamp_draft.scale);
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(2);
        assert!(close_enough(h.state().stamp_draft.scale, 100.0));

        focus_percent(&mut h, "既定倍率 (%)");
        h.key_press(egui::Key::Backspace);
        h.run_steps(2);
        assert_eq!(shown(&h, "既定倍率 (%)"), "", "Backspace must leave the box empty");
        assert!(close_enough(h.state().stamp_draft.scale, 100.0));
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(close_enough(h.state().stamp_draft.scale, 100.0));
        assert_eq!(shown(&h, "既定倍率 (%)"), "100");

        focus_percent(&mut h, "既定倍率 (%)");
        type_chars(&mut h, "既定倍率 (%)", "120");
        h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Name:").click();
        h.run_steps(3);
        assert!(close_enough(h.state().stamp_draft.scale, 120.0), "blur should commit 120, got {}", h.state().stamp_draft.scale);

        commit_percent(&mut h, "既定倍率 (%)", "10");
        assert!(close_enough(h.state().stamp_draft.scale, 120.0));
        assert_eq!(shown(&h, "既定倍率 (%)"), "120");
        let toast = h.state().toast.as_ref().map(|(text, _)| text.clone()).unwrap_or_default();
        assert!(toast.contains("25"), "{toast}");
        commit_percent(&mut h, "既定倍率 (%)", "401");
        assert!(close_enough(h.state().stamp_draft.scale, 120.0));

        for text in ["50", "100", "120", "200", "400"] {
            commit_percent(&mut h, "既定倍率 (%)", text);
            let scale = h.state().stamp_draft.scale;
            assert!(close_enough(scale, text.parse().unwrap()), "{text} committed as {scale}");
            assert_eq!(shown(&h, "既定倍率 (%)"), text);
        }
        commit_percent(&mut h, "既定倍率 (%)", "80");
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert!(close_enough(h.state().custom_stamps[0].scale, 80.0));
        let saved = h.state().persist();
        let mut again = crate::PdfCraftApp::new();
        again.restore(&saved);
        assert!(close_enough(again.custom_stamps[0].scale, 80.0));
        assert_eq!(again.custom_stamps[0].data.as_ref(), png.as_slice());

        h.state_mut().quick_tool = crate::QuickTool::CustomStamp(0);
        h.state_mut().begin_stamp_place(0, 0, [100.0, 150.0], 0).unwrap();
        h.run_steps(3);
        assert_eq!(shown(&h, "倍率"), "80");
        commit_percent(&mut h, "倍率", "50");
        let width = {
            let place = h.state().views[0].stamp_place.as_ref().unwrap();
            assert!(close_enough(place.scale, 50.0));
            place.rect[2] - place.rect[0]
        };
        assert!((width - 40.0).abs() < 1e-6, "{width}");
        focus_percent(&mut h, "倍率");
        h.key_press(egui::Key::Backspace);
        h.run_steps(2);
        assert_eq!(shown(&h, "倍率"), "");
        assert!(close_enough(h.state().views[0].stamp_place.as_ref().unwrap().scale, 50.0));
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(close_enough(h.state().views[0].stamp_place.as_ref().unwrap().scale, 50.0));

        {
            let place = h.state_mut().views[0].stamp_place.as_mut().unwrap();
            place.rect = crate::stamp_preset::rect_at_scale(place.rect, place.nominal, 150.0);
            place.scale = crate::stamp_preset::percent_of(place.rect, place.nominal.0);
        }
        h.run_steps(2);
        assert_eq!(shown(&h, "倍率"), "150", "a corner drag updates the percent that is not being typed");
        h.get_by_label("この倍率を既定にする").click();
        h.run_steps(2);
        assert!(close_enough(h.state().custom_stamps[0].scale, 150.0));

        h.get_by_label("確定").click();
        h.run_steps(3);
        assert!(h.state().views[0].stamp_place.is_none());
        let id = h.state().views[0].id;
        assert_eq!(h.state().session.get(id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count(), 1);
        assert!(h.state_mut().execute("edit.undo"));
        h.run_steps(1);
        assert_eq!(h.state().session.get(id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count(), 0);
        assert!(h.state_mut().execute("edit.redo"));
        h.run_steps(1);
        assert_eq!(h.state().session.get(id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count(), 1);
        assert_eq!(sha2::Sha256::digest(std::fs::read(&path).unwrap()).as_slice(), hash.as_slice());

        h.state_mut().start_date_stamp("date.png".into(), png.clone());
        h.run_steps(3);
        commit_percent(&mut h, "既定倍率 (%)", "200");
        assert!(close_enough(h.state().date_draft.scale, 200.0));
        h.get_by_label("登録").click();
        h.run_steps(3);
        assert!(close_enough(h.state().date_stamps[0].scale, 200.0));
        assert_eq!(h.state().date_stamps[0].data.as_ref(), png.as_slice());
        let saved = h.state().persist();
        let mut again = crate::PdfCraftApp::new();
        again.restore(&saved);
        assert!(close_enough(again.date_stamps[0].scale, 200.0));
        assert_eq!(again.date_stamps[0].data.as_ref(), png.as_slice());

        h.state_mut().start_chairman_seal("seal.png".into(), png);
        h.run_steps(3);
        commit_percent(&mut h, "既定倍率 (%)", "80");
        assert!(close_enough(h.state().stamp_draft.scale, 80.0));
        assert!(h.state().stamp_draft.chairman);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(h.state().seal_store.library.seals.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod date_font_tests {
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn harness(simple: bool) -> Harness<'static, crate::PdfCraftApp> {
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(|_cc| {
            let mut app = crate::PdfCraftApp::new();
            if simple {
                app.set_ui_mode(crate::UiMode::Simple);
            }
            app
        });
        h.run_steps(2);
        h
    }

    fn open_drawn(h: &mut Harness<'static, crate::PdfCraftApp>) {
        h.state_mut().start_drawn_date();
        h.state_mut().date_draft.name = "保持確認".into();
        if let Some(drawn) = h.state_mut().date_draft.drawn.as_mut() {
            drawn.top = "東京".into();
            drawn.bottom = "理事会".into();
        }
        h.run_steps(4);
    }

    fn search(h: &mut Harness<'static, crate::PdfCraftApp>, text: &str) {
        let field = h.get_by_role_and_label(Role::TextInput, "検索");
        field.click();
        field.focus();
        h.run_steps(2);
        h.get_by_role_and_label(Role::TextInput, "検索").type_text(text);
        h.run_steps(3);
    }

    #[test]
    fn each_row_keeps_its_own_face_and_the_draft() {
        let mut h = harness(false);
        open_drawn(&mut h);
        h.get_by_label("上段のフォントを選択…").click();
        h.run_steps(3);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::FontPicker)));
        assert_eq!(h.state().date_draft.name, "保持確認");
        assert!(h.query_by_label("Helvetica 標準").is_none(), "a date stamp cannot pick a standard-14 name");
        search(&mut h, "Inter");
        assert!(h.query_by_label("該当するフォントがありません。").is_some());
        h.get_by_label("キャンセル").click();
        h.run_steps(3);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::DateStamp)));
        assert!(h.state().date_draft.drawn.as_ref().unwrap().top_font.is_none());
        assert_eq!(h.state().date_draft.name, "保持確認");
        assert_eq!(h.state().date_draft.drawn.as_ref().unwrap().bottom, "理事会");

        let mincho = pdfcraft_fonts::catalog::get("craft:shippori-mincho:regular").is_some_and(|face| face.available());
        h.get_by_label("上段のフォントを選択…").click();
        h.run_steps(3);
        if mincho {
            search(&mut h, "明朝");
            h.get_by_label("しっぽり明朝 標準").click();
            h.run_steps(2);
            h.get_by_label("決定").click();
            h.run_steps(3);
            assert_eq!(h.state().date_draft.drawn.as_ref().unwrap().top_font.as_deref(), Some("craft:shippori-mincho:regular"));
            assert!(!h.state().date_draft.drawn.as_ref().unwrap().top_bold);
        } else {
            h.key_press(egui::Key::Escape);
            h.run_steps(2);
        }
        assert!(h.state().date_draft.drawn.as_ref().unwrap().bottom_font.is_none());

        h.get_by_label("日付のフォントを選択…").click();
        h.run_steps(3);
        search(&mut h, "semibold");
        h.get_by_label("Inter セミボールド").click();
        h.run_steps(1);
        h.get_by_label("決定").click();
        h.run_steps(3);
        let drawn = h.state().date_draft.drawn.clone().unwrap();
        assert_eq!(drawn.date_font.as_deref(), Some("bundled:inter:semibold"));
        assert!(!drawn.date_bold);
        assert_eq!(drawn.bottom_font.as_deref(), None);
        if mincho {
            assert_eq!(drawn.top_font.as_deref(), Some("craft:shippori-mincho:regular"));
        }
        assert_eq!(h.state().date_draft.name, "保持確認");

        h.get_by_label("下段のフォントを選択…").click();
        h.run_steps(2);
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::DateStamp)));
        assert!(h.state().date_draft.drawn.as_ref().unwrap().bottom_font.is_none());
        assert_eq!(h.state().date_draft.drawn.as_ref().unwrap().date_font.as_deref(), Some("bundled:inter:semibold"));
    }

    #[test]
    fn simple_mode_opens_the_same_font_window() {
        let mut h = harness(true);
        assert_eq!(h.state().ui_mode(), crate::UiMode::Simple);
        open_drawn(&mut h);
        h.get_by_label("下段のフォントを選択…").click();
        h.run_steps(3);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::FontPicker)));
        assert_eq!(h.state().date_draft.name, "保持確認");
        h.get_by_label("キャンセル").click();
        h.run_steps(2);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::DateStamp)));
        assert!(h.state().date_draft.drawn.as_ref().unwrap().bottom_font.is_none());
        assert_eq!(h.state().date_draft.drawn.as_ref().unwrap().bottom, "理事会");
    }
}
