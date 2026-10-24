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
fn scale_percent_field(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    committed: f64,
    label: egui::Id,
    accept_keys: bool,
) -> ScaleCommit {
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
    if !accept_keys {
        return ScaleCommit::Unchanged;
    }
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
        match scale_percent_field(ui, "stamp-draft-scale", scale, l.id, true) {
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
                match scale_percent_field(ui, ("custom-menu-scale", i), s.scale, l.id, true) {
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
            match scale_percent_field(ui, ("date-menu-scale", i), stamp.scale, l.id, true) {
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
    let warn = egui::Color32::from_rgb(176, 48, 48);
    let running = app.seals.running();
    match app.seals.state.clone() {
        crate::seal_service::SealState::NoStore => {
            ui.label("この環境では理事長印を使えません。");
            return;
        }
        crate::seal_service::SealState::Loading => {
            ui.label("理事長印のデータを読み込んでいます…");
            return;
        }
        crate::seal_service::SealState::Failed(reason) => {
            ui.label(egui::RichText::new(format!("理事長印は使えません。{reason}")).color(warn));
            ui.label("読めないデータは表示も変更もしません。ファイルはそのままにしてあります。");
            if ui.add_enabled_ui(!running, |ui| widgets::pill_button(ui, "もう一度読み込む", false)).inner.clicked() {
                app.reload_seals();
            }
            return;
        }
        crate::seal_service::SealState::NeedsMigration { seals, events, error } => {
            ui.label(egui::RichText::new("理事長印のデータが旧形式です。使う前に新しい形式へ移す必要があります。").color(warn));
            ui.label(format!("旧形式のデータ：理事長印 {seals} 件、使用履歴 {events} 件"));
            if let Some(error) = error {
                ui.label(egui::RichText::new(error).color(warn));
            }
            if ui.add_enabled_ui(!running, |ui| widgets::pill_button(ui, "移行について確認…", false)).inner.clicked() {
                app.dialog = Some(Dialog::SealMigrate);
            }
            return;
        }
        crate::seal_service::SealState::Deferred { seals, events } => {
            ui.label("理事長印のデータの移行を延期しています。移行するまで理事長印は使えません。");
            ui.label(format!("旧形式のデータ：理事長印 {seals} 件、使用履歴 {events} 件"));
            if ui.add_enabled_ui(!running, |ui| widgets::pill_button(ui, "移行について確認…", false)).inner.clicked() {
                app.dialog = Some(Dialog::SealMigrate);
            }
            return;
        }
        crate::seal_service::SealState::Ready => {}
    }
    ui.label(
        egui::RichText::new(
            "この PC の現在の Windows ユーザーだけが復号できます。他のユーザーや他の PC では開けません。元の画像ファイル自体は暗号化されません。",
        )
        .small(),
    );
    for notice in app.seals.warnings.clone() {
        let text = match notice {
            crate::seal_store_v2::Notice::ManyEvents { events } => {
                format!("使用履歴が {events} 件あります。記録が多いと読み書きに時間がかかります。")
            }
            crate::seal_store_v2::Notice::NearFull { bytes, limit } => {
                format!("理事長印の記録が上限に近づいています（{} / {} MiB）。上限を超える押印は行いません。", bytes >> 20, limit >> 20)
            }
            _ => continue,
        };
        ui.label(egui::RichText::new(text).color(warn));
    }
    let open = app.seals.open_uses();
    if !open.is_empty() {
        ui.label(egui::RichText::new(format!("確定も取り消しもされていない押印準備が {} 件あります。", open.len())).color(warn));
        for op in &open {
            ui.label(egui::RichText::new(format!("操作 ID: {op}")).small());
        }
        ui.label(egui::RichText::new("別のウィンドウで押印中の場合もあるため、自動では取り消しません。").small());
    }
    let mut picked = None;
    for (i, seal) in app.seals.seals().iter().enumerate() {
        let active = app.quick_tool == QuickTool::ChairmanSeal(i);
        let label = format!("{}（{} × {}、{:.0}%）", seal.name, seal.width, seal.height, seal.scale);
        if ui.add(egui::Button::selectable(active, label).min_size(egui::vec2(ui.available_width(), 28.0))).clicked() {
            picked = Some(i);
        }
    }
    if let Some(i) = picked {
        app.quick_tool = QuickTool::ChairmanSeal(i);
    }
    if app.seals.seals().is_empty() {
        ui.label("登録された理事長印はありません。");
    }
    ui.add_space(4.0);
    if ui.add_enabled_ui(!running, |ui| widgets::pill_button(ui, "理事長印を登録…", false)).inner.clicked() {
        app.pick_stamp_file(true);
    }
    let can_delete = !running && matches!(app.quick_tool, QuickTool::ChairmanSeal(i) if i < app.seals.seals().len());
    if ui.add_enabled_ui(can_delete, |ui| widgets::pill_button(ui, "選択中の理事長印を削除…", false)).inner.clicked() {
        app.seal_approval = None;
        app.seal_delete = match app.quick_tool {
            QuickTool::ChairmanSeal(i) => app.seals.seals().get(i).map(crate::seal_service::SealRef::of),
            _ => None,
        };
        app.dialog = Some(Dialog::DeleteSeal);
    }
    if ui.add_enabled_ui(!running, |ui| widgets::pill_button(ui, "最新の内容を読み込む", false)).inner.clicked() {
        app.reload_seals();
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
        if !self.seals.usable() {
            self.notify(self.seals.unusable_reason());
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
            self.begin_seal_register(d);
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

    /// The Create Stamp OK button (`dialogs::show`) dismisses `Dialog::CreateStamp` and then
    /// calls [`Self::save_custom_stamp`]. The save itself leaves the dialog up, because the
    /// dialog frame has already closed it. A test that saves directly has to do both, or the
    /// registration window stays in front and the stamp waits.
    #[cfg(test)]
    pub(crate) fn accept_stamp_registration(&mut self) {
        assert!(matches!(self.dialog, Some(Dialog::CreateStamp)), "registration OK only closes CreateStamp, got {:?}", self.dialog);
        self.dialog = None;
        self.save_custom_stamp();
    }

    pub(crate) fn note_stamp_try(&mut self) {
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
        match scale_percent_field(ui, "date-draft-scale", scale, l.id, true) {
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
        match scale_percent_field(ui, "date-draft-scale", scale, l.id, true) {
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
    /// Page rotation, in degrees, used to swap effect padding the same way [`Self::nominal`] swaps.
    pub rotation: u16,
    /// Chairman seals only. Absent for a general stamp or a date stamp.
    pub seal: Option<SealDraft>,
    /// The stored chairman seal this placement was made from: its id, revision and image hash.
    /// Approval and the stamp record name this, never a list position.
    pub seal_ref: Option<crate::seal_service::SealRef>,
    /// Bumped only when [`Self::pixels`] is replaced. The page reuses the preview texture while this stays put.
    pub preview_gen: u64,
}

/// The chairman-seal preview. The registered PNG and the source pixels are not written by effects.
#[derive(Clone, Debug)]
pub struct SealDraft {
    /// Decoded registration image. Effects read it and do not change it.
    pub source: (u32, u32, Vec<u8>),
    pub effects: crate::seal_store::SealEffects,
    pub seed: u64,
    /// Degrees actually used for this seed. Zero when effects are off.
    pub angle_deg: f64,
    /// Output size divided by the source size, in image space.
    pub axis: (f64, f64),
    /// Ink rectangle before padding. Scale and millimetres use this, and it stays centred in [`StampPlace::rect`].
    pub ink_rect: [f64; 4],
    /// Set when the last preview failed. Confirm stays unavailable until the settings work or are turned off.
    pub error: Option<String>,
    /// Settings and seed of the pixels now in [`StampPlace::pixels`]. A repeat refresh with the same key does nothing.
    applied: Option<EffectStamp>,
    /// Identity of [`Self::source`]. The before-image texture uses it and does not follow effect rebuilds.
    source_gen: u64,
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
    effects: crate::seal_store::SealEffects,
    seal_ref: Option<crate::seal_service::SealRef>,
}

#[derive(Clone, Copy, Debug)]
pub enum StampDrag {
    Move { from: egui::Pos2, origin: [f64; 4], ink: [f64; 4] },
    Resize { handle: (i8, i8), from: egui::Pos2, origin: [f64; 4], ink: [f64; 4] },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EffectStamp {
    enabled: bool,
    manual: bool,
    rotation: u64,
    manual_deg: u64,
    fade: u64,
    bleed: u64,
    seed: u64,
}

fn effect_stamp(effects: &crate::seal_store::SealEffects, seed: u64) -> EffectStamp {
    EffectStamp {
        enabled: effects.enabled,
        manual: effects.manual,
        rotation: effects.rotation_deg.to_bits(),
        manual_deg: effects.manual_deg.to_bits(),
        fade: effects.fade.to_bits(),
        // Spread is stored and ignored, so it must not rebuild the preview.
        bleed: 0,
        seed,
    }
}

fn fresh_preview_gen() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn assign_pixels(place: &mut StampPlace, pixels: Option<(u32, u32, Vec<u8>)>) {
    place.pixels = pixels;
    place.preview_gen = fresh_preview_gen();
}

/// Upload the placement image once per [`StampPlace::preview_gen`]. Opacity is a tint, so it does not upload again.
fn preview_texture(ctx: &egui::Context, cache: &str, generation: u64, width: u32, height: u32, pixels: &[u8]) -> egui::TextureHandle {
    let key = egui::Id::new(cache);
    if let Some((cached, texture)) = ctx.data(|data| data.get_temp::<(u64, egui::TextureHandle)>(key))
        && cached == generation
    {
        return texture;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], pixels);
    let texture = ctx.load_texture("stamp-place-preview", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(key, (generation, texture.clone())));
    texture
}

/// Draw and drag the unconfirmed stamp. Returns true when this click belongs to the draft.
///
/// `interactive` is false while a dialog covers the page. The image is still drawn, and a drag
/// that was already running is dropped, so the effects window can sit beside the seal without
/// moving it or rebuilding it.
pub(crate) fn page_draft(
    painter: &egui::Painter,
    view: &mut crate::canvas::DocView,
    info: &pdfcraft_render::DocInfo,
    page: usize,
    xf: &crate::canvas::PageXform,
    interactive: bool,
) -> bool {
    let Some(place) = view.stamp_place.as_mut() else { return false };
    if place.page != page {
        return false;
    }
    let screen = user_rect_to_screen(xf, info, page, place.rect);
    if let Some((width, height, pixels)) = place.pixels.as_ref()
        && pixels.len() == (*width as usize).saturating_mul(*height as usize).saturating_mul(4)
    {
        let texture = preview_texture(painter.ctx(), "stamp-place-preview", place.preview_gen, *width, *height, pixels);
        let tint = egui::Color32::WHITE.gamma_multiply(place.opacity.clamp(0.0, 1.0) as f32);
        painter.image(texture.id(), screen, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
    }
    painter.rect_stroke(screen, egui::CornerRadius::ZERO, egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 110, 180)), egui::StrokeKind::Outside);
    for handle in CORNERS {
        let at = crate::comments::handle_pos(screen, handle);
        painter.circle(at, 4.5, egui::Color32::WHITE, egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 110, 180)));
    }
    if !interactive {
        place.drag = None;
        return false;
    }
    let pointer = painter.ctx().input(|input| input.pointer.hover_pos());
    let pressed = painter.ctx().input(|input| input.pointer.primary_pressed());
    let down = painter.ctx().input(|input| input.pointer.primary_down());
    let handle_at = |p: egui::Pos2| CORNERS.into_iter().find(|handle| crate::comments::handle_pos(screen, *handle).distance(p) <= 8.0);
    if pressed && let Some(p) = pointer {
        let ink = place.seal.as_ref().map(|seal| seal.ink_rect).unwrap_or(place.rect);
        if let Some(handle) = handle_at(p) {
            place.drag = Some(StampDrag::Resize { handle, from: p, origin: place.rect, ink });
        } else if screen.expand(2.0).contains(p) {
            place.drag = Some(StampDrag::Move { from: p, origin: place.rect, ink });
        }
    }
    if let Some(p) = pointer {
        match place.drag {
            Some(StampDrag::Move { from, origin, ink }) => {
                let (a, b) = (screen_to_user(xf, info, page, from), screen_to_user(xf, info, page, p));
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                place.rect = shift_rect(origin, dx, dy);
                if let Some(seal) = place.seal.as_mut() {
                    seal.ink_rect = shift_rect(ink, dx, dy);
                }
            }
            Some(StampDrag::Resize { handle, from, origin, .. }) => {
                let from_screen = user_rect_to_screen(xf, info, page, origin);
                let ratio = (from_screen.height() > 0.0)
                    .then(|| from_screen.width() / from_screen.height())
                    .filter(|ratio| ratio.is_finite() && *ratio > 0.0);
                let next = crate::comments::resized(from_screen, handle, p - from, ratio);
                if next.width() >= 4.0 && next.height() >= 4.0 {
                    place.rect = screen_rect_to_user(xf, info, page, next);
                    if let Some(seal) = place.seal.as_mut() {
                        seal.ink_rect = ink_from_display(place.rect, seal.axis, place.rotation);
                        let width = seal.ink_rect;
                        place.scale = crate::stamp_preset::percent_of(width, place.nominal.0);
                    } else {
                        place.scale = crate::stamp_preset::percent_of(place.rect, place.nominal.0);
                    }
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
        QuickTool::ChairmanSeal(i) => app.seals.seals().get(i).map(|seal| seal.name.clone()),
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
    // A dialog in front owns Enter and Escape. The bar stays visible, but it must not confirm,
    // cancel, or commit a number from those keys.
    let keys = app.front_prompt().is_none();
    let name = app.views[index].stamp_place.as_ref().map(|place| place.name.clone()).unwrap_or_default();
    let dated = app.views[index].stamp_place.as_ref().is_some_and(|place| place.date_overlay.is_some());
    let calendar = app.views[index].stamp_place.as_ref().is_some_and(|place| place.date_overlay.as_ref().is_some_and(|date| date.show_calendar));
    let chairman = app.views[index].stamp_place.as_ref().is_some_and(|place| place.chairman);
    let height = if calendar {
        248.0
    } else if dated || chairman {
        112.0
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
                let opacity_changed = ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("透明度")).changed();
                if keys
                    && opacity_changed
                    && let Some(place) = app.views[index].stamp_place.as_mut()
                {
                    place.opacity = opacity.clamp(0.0, 1.0);
                    app.seal_approval = None;
                }
                let committed = app.views[index].stamp_place.as_ref().map(|place| place.scale).unwrap_or(100.0);
                let scale_label = ui.label("倍率");
                let scale_commit = scale_percent_field(ui, ("place-scale", index), committed, scale_label.id, keys);
                if keys {
                    match scale_commit {
                        ScaleCommit::Set(scale) => {
                            if let Some(place) = app.views[index].stamp_place.as_mut() {
                                apply_place_scale(place, scale);
                                app.seal_approval = None;
                                if place.date_overlay.as_ref().is_some_and(|overlay| overlay.drawn.is_some()) && !rebake_date(place) {
                                    notice = Some("日付を画像にできませんでした。".into());
                                }
                            }
                        }
                        ScaleCommit::Rejected => notice = Some("倍率は25%から400%です。".into()),
                        ScaleCommit::Unchanged => {}
                    }
                }
                if let Some(place) = app.views[index].stamp_place.as_ref() {
                    ui.label(crate::stamp_preset::size_label(measured_rect(place), place.circular));
                }
                let save_scale_clicked = widgets::pill_button(ui, "この倍率を既定にする", false).clicked();
                if keys && save_scale_clicked {
                    save_scale = true;
                }
                let blocked = app.views[index].stamp_place.as_ref().and_then(commit_block_message);
                let confirm_clicked = ui.add_enabled_ui(blocked.is_none(), |ui| widgets::pill_button(ui, "確定", true)).inner.clicked();
                if keys && confirm_clicked {
                    confirm = true;
                }
                let cancel_clicked = widgets::pill_button(ui, "キャンセル", false).clicked();
                if keys && cancel_clicked {
                    cancel = true;
                }
            });
            if chairman {
                ui.horizontal(|ui| {
                    let effects = widgets::pill_button(ui, "印影効果…", false).clicked();
                    if keys && effects {
                        app.dialog = Some(Dialog::SealEffects);
                    }
                    if let Some(place) = app.views[index].stamp_place.as_ref()
                        && let Some(error) = place.seal.as_ref().and_then(|seal| seal.error.clone())
                    {
                        ui.label(egui::RichText::new(error).color(egui::Color32::from_rgb(176, 48, 48)));
                    }
                });
            }
            if dated {
                ui.horizontal(|ui| {
                    ui.label("日付");
                    let mut text = app.views[index]
                        .stamp_place
                        .as_ref()
                        .and_then(|place| place.date_overlay.as_ref().map(|date| date.entry.clone()))
                        .unwrap_or_default();
                    let edited = ui.add(crate::widgets::line(&mut text).desired_width(110.0).hint_text("YYYY/M/D"));
                    if keys && edited.changed() {
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
                    let today_clicked = widgets::pill_button(ui, "今日", false).clicked();
                    if keys && today_clicked {
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
                    let calendar_clicked = widgets::pill_button(ui, "カレンダー", false).clicked();
                    if keys
                        && calendar_clicked
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
                    let action = if keys { month_grid(ui, year, month) } else { MonthAction::None };
                    if !keys {
                        let _ = month_grid(ui, year, month);
                    }
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
            assign_pixels(place, Some(pixels));
            true
        }
        None => false,
    }
}

/// Rectangle the millimetre label measures. Effect padding stays out of this rectangle.
pub(crate) fn measured_rect(place: &StampPlace) -> [f64; 4] {
    place.seal.as_ref().map(|seal| seal.ink_rect).unwrap_or(place.rect)
}

fn shift_rect(rect: [f64; 4], dx: f64, dy: f64) -> [f64; 4] {
    [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy]
}

fn page_axis(axis: (f64, f64), rotation: u16) -> (f64, f64) {
    let (sx, sy) = if matches!(rotation, 90 | 270) { (axis.1, axis.0) } else { axis };
    let finite = |value: f64| if value.is_finite() && value > 0.0 { value } else { 1.0 };
    (finite(sx), finite(sy))
}

/// Grow `ink` about its centre by the processed image's extra margin.
fn display_rect(ink: [f64; 4], axis: (f64, f64), rotation: u16) -> [f64; 4] {
    let (sx, sy) = page_axis(axis, rotation);
    let cx = (ink[0] + ink[2]) * 0.5;
    let cy = (ink[1] + ink[3]) * 0.5;
    let w = (ink[2] - ink[0]).abs() * sx;
    let h = (ink[3] - ink[1]).abs() * sy;
    if !cx.is_finite() || !cy.is_finite() || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return ink;
    }
    [cx - w * 0.5, cy - h * 0.5, cx + w * 0.5, cy + h * 0.5]
}

fn ink_from_display(display: [f64; 4], axis: (f64, f64), rotation: u16) -> [f64; 4] {
    let (sx, sy) = page_axis(axis, rotation);
    let cx = (display[0] + display[2]) * 0.5;
    let cy = (display[1] + display[3]) * 0.5;
    let w = (display[2] - display[0]).abs() / sx;
    let h = (display[3] - display[1]).abs() / sy;
    if !cx.is_finite() || !cy.is_finite() || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return display;
    }
    [cx - w * 0.5, cy - h * 0.5, cx + w * 0.5, cy + h * 0.5]
}

fn apply_place_scale(place: &mut StampPlace, scale: f64) {
    if let Some(seal) = place.seal.as_mut() {
        seal.ink_rect = crate::stamp_preset::rect_at_scale(seal.ink_rect, place.nominal, scale);
        let (axis, ink) = (seal.axis, seal.ink_rect);
        place.rect = display_rect(ink, axis, place.rotation);
    } else {
        place.rect = crate::stamp_preset::rect_at_scale(place.rect, place.nominal, scale);
    }
    place.scale = scale;
}

fn next_effect_seed(previous: u64) -> u64 {
    let tick =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)).unwrap_or(1);
    let mixed = previous.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(tick).wrapping_add(0xA5A5);
    if mixed == previous { previous.wrapping_add(1) } else { mixed }
}

fn effect_failure(error: crate::seal_effect::EffectError) -> String {
    match error {
        crate::seal_effect::EffectError::TooLarge => "加工後の印影が大きすぎます。設定を下げるか、キャンセルしてください。".into(),
        crate::seal_effect::EffectError::BadSize | crate::seal_effect::EffectError::BadLength => {
            "印影を加工できません。設定を見直すか、キャンセルしてください。".into()
        }
    }
}

/// Rebuild the chairman preview from the stored source, settings and seed. The source and the PNG are not written.
pub(crate) fn refresh_seal_preview(place: &mut StampPlace) {
    let Some(draft) = place.seal.as_ref() else { return };
    if draft.source.0 == 0 || draft.source.1 == 0 || draft.source.2.is_empty() {
        if let Some(draft) = place.seal.as_mut() {
            draft.error = Some("印影を表示できません。設定を見直すか、キャンセルしてください。".into());
            draft.applied = None;
        }
        return;
    }
    let key = effect_stamp(&draft.effects, draft.seed);
    if draft.applied == Some(key) && draft.error.is_none() && place.pixels.is_some() {
        return;
    }
    let applied = crate::seal_effect::apply(&draft.source.2, draft.source.0, draft.source.1, &draft.effects, draft.seed);
    let Some(draft) = place.seal.as_mut() else { return };
    match applied {
        Ok(crate::seal_effect::Applied::Unchanged) => {
            let pixels = (draft.source.0, draft.source.1, draft.source.2.clone());
            place.rect = draft.ink_rect;
            draft.angle_deg = 0.0;
            draft.axis = (1.0, 1.0);
            draft.error = None;
            draft.applied = Some(key);
            assign_pixels(place, Some(pixels));
        }
        Ok(crate::seal_effect::Applied::Image(image)) => {
            let axis = image.axis_scale();
            draft.angle_deg = image.angle_deg;
            draft.axis = axis;
            draft.error = None;
            draft.applied = Some(key);
            place.rect = display_rect(draft.ink_rect, axis, place.rotation);
            assign_pixels(place, Some((image.width, image.height, image.rgba)));
        }
        Err(error) => {
            draft.error = Some(effect_failure(error));
            draft.applied = None;
        }
    }
}

pub(crate) fn set_place_effects(place: &mut StampPlace, effects: crate::seal_store::SealEffects) -> Option<String> {
    let draft = place.seal.as_mut()?;
    draft.effects = effects;
    refresh_seal_preview(place);
    place.seal.as_ref().and_then(|seal| seal.error.clone())
}

fn commit_block_message(place: &StampPlace) -> Option<String> {
    place.seal.as_ref().and_then(|seal| seal.error.clone())
}

/// What the chairman-seal checkbox confirmed: the PNG that will be written, where, and on which document.
/// A ticked box without this snapshot does not allow the stamp.
#[derive(Clone, Debug)]
pub(crate) struct SealApproval {
    pub doc: pdfcraft_engine::DocId,
    pub page: usize,
    pub rect: [f64; 4],
    pub opacity: f64,
    /// Preview generation at approval. The dialog uses it so it does not re-encode every frame.
    pub preview_gen: u64,
    /// SHA-256 of the PNG bytes that commit will pass to the stamp.
    pub sha256: [u8; 32],
}

fn sha256_bytes(message: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
        0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
        0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
        0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
        0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let bit_len = u64::try_from(message.len()).unwrap_or(u64::MAX).saturating_mul(8);
    let mut data = Vec::with_capacity(message.len().saturating_add(72));
    data.extend_from_slice(message);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    let mut state = [0x6a09e667u32, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    for chunk in data.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (i, bytes) in chunk.chunks_exact(4).enumerate() {
            let Ok(bytes) = <[u8; 4]>::try_from(bytes) else { continue };
            if let Some(slot) = words.get_mut(i) {
                *slot = u32::from_be_bytes(bytes);
            }
        }
        for i in 16..64 {
            let earlier = words[i - 15];
            let recent = words[i - 2];
            let small = earlier.rotate_right(7) ^ earlier.rotate_right(18) ^ (earlier >> 3);
            let large = recent.rotate_right(17) ^ recent.rotate_right(19) ^ (recent >> 10);
            words[i] = words[i - 16].wrapping_add(small).wrapping_add(words[i - 7]).wrapping_add(large);
        }
        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];
        for i in 0..64 {
            let high = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let first = h.wrapping_add(high).wrapping_add(choose).wrapping_add(K[i]).wrapping_add(words[i]);
            let low = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(low.wrapping_add(majority));
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
    let mut out = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        let start = i.saturating_mul(4);
        if let Some(slot) = out.get_mut(start..start + 4) {
            slot.copy_from_slice(&word.to_be_bytes());
        }
    }
    out
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in sha256_bytes(bytes) {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

pub(crate) fn rect_same(left: [f64; 4], right: [f64; 4]) -> bool {
    left.iter().zip(right).all(|(a, b)| (a - b).abs() <= 1.0e-6)
}

/// The preview pixels differ from the registered PNG. A random range that happened to draw 0°,
/// with no wear, does not: the original PNG is stamped and is not encoded again.
fn seal_writes_processed(seal: &SealDraft) -> bool {
    if seal.effects.legacy_off() {
        return false;
    }
    if seal.error.is_some() {
        return true;
    }
    seal.effects.fade > 0.0 || seal.angle_deg.abs() > 1.0e-6
}

fn rotation_mode_name(effects: &crate::seal_store::SealEffects) -> &'static str {
    if effects.manual {
        "manual"
    } else if effects.legacy_off() {
        ""
    } else {
        "random"
    }
}

/// PNG bytes the stamp edit will receive. A processed chairman seal is encoded from the preview
/// RGBA. Anything else keeps the bytes it already had. A failed encode does not fall back.
fn stamp_image_bytes(place: &StampPlace) -> Result<Arc<Vec<u8>>, String> {
    if place.date_overlay.is_some() {
        let (width, height, pixels) = place.pixels.as_ref().ok_or("日付を画像にできませんでした。")?;
        let png = crate::stamp_preset::encode_rgba_png(*width, *height, pixels).ok_or("日付を画像にできませんでした。")?;
        return Ok(Arc::new(png));
    }
    let processed = place.chairman && place.seal.as_ref().is_some_and(seal_writes_processed);
    if processed {
        if let Some(error) = place.seal.as_ref().and_then(|seal| seal.error.clone()) {
            return Err(error);
        }
        let (width, height, pixels) = place.pixels.as_ref().ok_or_else(|| "加工済み印影を用意できないため、押印しませんでした。".to_string())?;
        let png = crate::stamp_preset::encode_rgba_png(*width, *height, pixels)
            .ok_or_else(|| "加工済み印影を画像にできませんでした。押印しませんでした。".to_string())?;
        return Ok(Arc::new(png));
    }
    Ok(Arc::clone(&place.data))
}

fn approval_matches(approval: &SealApproval, doc: pdfcraft_engine::DocId, place: &StampPlace) -> bool {
    approval.doc == doc
        && approval.page == place.page
        && approval.preview_gen == place.preview_gen
        && rect_same(approval.rect, place.rect)
        && (approval.opacity - place.opacity.clamp(0.0, 1.0)).abs() <= 1.0e-9
}

/// Record the current chairman placement as approved. A failed encode clears any earlier snapshot.
pub(crate) fn approve_current_seal(app: &mut PdfCraftApp) -> Result<(), String> {
    let Some(index) = app.active else { return Err("仮置きがありません。".into()) };
    let Some(doc) = app.views.get(index).map(|view| view.id) else { return Err("仮置きがありません。".into()) };
    let Some(place) = app.views[index].stamp_place.clone() else { return Err("仮置きがありません。".into()) };
    if !place.chairman {
        return Err("理事長印ではありません。".into());
    }
    let bytes = match stamp_image_bytes(&place) {
        Ok(bytes) => bytes,
        Err(error) => {
            app.seal_approval = None;
            return Err(error);
        }
    };
    app.seal_approval = Some(SealApproval {
        doc,
        page: place.page,
        rect: place.rect,
        opacity: place.opacity.clamp(0.0, 1.0),
        preview_gen: place.preview_gen,
        sha256: sha256_bytes(bytes.as_ref()),
    });
    Ok(())
}

pub(crate) fn confirm_seal_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> (bool, bool) {
    let Some(index) = app.active else { return (false, true) };
    let Some(place) = app.views[index].stamp_place.clone() else { return (false, true) };
    let doc = app.views[index].id;
    let doc_name = app.session.get(doc).map(|doc| doc.name.clone()).unwrap_or_default();
    if app.seal_approval.as_ref().is_some_and(|approval| !approval_matches(approval, doc, &place)) {
        app.seal_approval = None;
    }
    let processed = place.seal.as_ref().is_some_and(seal_writes_processed);
    ui.label(egui::RichText::new("理事長印の確認").font(theme::semibold(18.0)));
    ui.add_space(6.0);
    ui.label(format!("印影: {}", place.name));
    ui.label(format!("ファイル: {doc_name}"));
    ui.label(format!("ページ: {}", place.page.saturating_add(1)));
    ui.label(format!("加工: {}", if processed { "あり" } else { "なし" }));
    if let Some(seal) = place.seal.as_ref() {
        if seal.effects.manual || seal.effects.legacy_off() {
            let angle = if seal.effects.legacy_off() { 0.0 } else { seal.effects.manual_deg };
            ui.label(format!("回転: 手動 {angle:.1}°"));
        } else {
            ui.label(format!("回転: ランダム ±{:.1}°", seal.effects.rotation_deg));
        }
        ui.label(format!("実際の回転: {:.2}°", seal.angle_deg));
        ui.label(format!("かすれ: {:.0}", seal.effects.fade));
    }
    ui.label(format!("倍率: {:.0}%", place.scale));
    ui.label(format!("透明度: {:.0}%", place.opacity.clamp(0.0, 1.0) * 100.0));
    ui.label("これはこの文書への押印確認です。アプリが承認を証明するものではありません。確定は、保存や印刷が済んだという意味ではありません。");
    paint_place_preview(ui, &place);
    ui.add_space(8.0);
    let mut checked = app.seal_approval.is_some();
    if ui.add(widgets::checkbox(&mut checked, "この文書への押印が承認されていることを確認しました")).changed() {
        if checked {
            if let Err(error) = approve_current_seal(app) {
                app.seal_approval = None;
                app.notify(error);
            }
        } else {
            app.seal_approval = None;
        }
    }
    let current = place.seal_ref.as_ref().is_some_and(|seal| app.seals.matches(seal));
    if !current {
        ui.label(
            egui::RichText::new("この理事長印は仮置きの後に変更または削除されました。押印するには、仮置きをやり直してください。")
                .color(egui::Color32::from_rgb(176, 48, 48)),
        );
    }
    ui.add_space(8.0);
    let (mut ok, mut cancel) = (false, false);
    let approved = current && !app.seals.running() && app.seal_approval.as_ref().is_some_and(|approval| approval_matches(approval, doc, &place));
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(approved, |ui| widgets::pill_button(ui, "確定", true)).inner.clicked() {
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
    let texture = preview_texture(ui.ctx(), "stamp-place-preview", place.preview_gen, *width, *height, pixels);
    let fit = 120.0 / (*width.max(height) as f32).max(1.0);
    ui.add(
        egui::Image::from_texture(&texture)
            .max_size(egui::vec2(*width as f32 * fit, *height as f32 * fit))
            .tint(egui::Color32::WHITE.gamma_multiply(place.opacity.clamp(0.0, 1.0) as f32)),
    );
}

/// Ink settings for the chairman seal being placed. Changes update the preview and are not saved
/// until 「この加工設定を既定にする」.
pub(crate) fn seal_effects_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> bool {
    let Some(index) = app.active else {
        ui.label("仮置きがありません。");
        return widgets::pill_button(ui, "閉じる", false).clicked();
    };
    let chairman = app.views.get(index).and_then(|view| view.stamp_place.as_ref()).is_some_and(|place| place.chairman && place.seal.is_some());
    if !chairman {
        ui.label("印影効果は理事長印の仮置きだけで使えます。");
        return widgets::pill_button(ui, "閉じる", false).clicked();
    }
    let mut effects = app.views[index]
        .stamp_place
        .as_ref()
        .and_then(|place| place.seal.as_ref().map(|seal| seal.effects))
        .unwrap_or_else(crate::seal_store::SealEffects::off);
    // An old switched-off file is shown as manual 0°. The stored file is not rewritten until the
    // user changes a control or saves the default.
    if effects.legacy_off() {
        let bleed = effects.bleed;
        effects = crate::seal_store::SealEffects::off();
        effects.bleed = bleed;
    }
    let mut store = false;
    let mut refresh = false;
    ui.label("回転方式");
    ui.horizontal(|ui| {
        if ui.selectable_label(effects.manual, "手動回転").clicked() && !effects.manual {
            effects.manual = true;
            effects.mark_enabled();
            store = true;
            refresh = true;
        }
        if ui.selectable_label(!effects.manual, "ランダム回転").clicked() && effects.manual {
            effects.manual = false;
            if effects.rotation_deg == 0.0 && effects.fade == 0.0 {
                let bleed = effects.bleed;
                let kept = effects.manual_deg;
                effects = crate::seal_store::SealEffects::enabled_preset();
                effects.bleed = bleed;
                effects.manual_deg = kept;
            }
            effects.mark_enabled();
            store = true;
            refresh = true;
        }
    });
    let (turn_store, turn_refresh) = if effects.manual {
        effect_number(ui, &mut effects.manual_deg, -30.0..=30.0, 0.1, "角度", "°")
    } else {
        effect_number(ui, &mut effects.rotation_deg, 0.0..=30.0, 0.1, "範囲", "°")
    };
    if effects.manual {
        ui.label(egui::RichText::new("0°は回転なし。マイナスは左、プラスは右です").small().weak());
    } else {
        ui.label(egui::RichText::new("左右にこの角度まで回転します").small().weak());
    }
    let (fade_store, fade_refresh) = effect_number(ui, &mut effects.fade, 0.0..=30.0, 1.0, "かすれ", "");
    ui.label(egui::RichText::new("押印ムラ・欠け・薄れを調整します").small().weak());
    store |= turn_store || fade_store;
    refresh |= turn_refresh || fade_refresh;
    if store || refresh {
        effects.mark_enabled();
    }
    ui.add_space(8.0);
    let mut another = false;
    let mut revert = false;
    let mut save = false;
    ui.horizontal(|ui| {
        another = widgets::pill_button(ui, "別パターンを生成", false).clicked();
        revert = widgets::pill_button(ui, "加工前に戻す", false).clicked();
    });
    ui.horizontal(|ui| {
        save = widgets::pill_button(ui, "この加工設定を既定にする", false).clicked();
    });
    let changed_error = if refresh {
        app.views.get_mut(index).and_then(|view| view.stamp_place.as_mut()).and_then(|place| set_place_effects(place, effects))
    } else if store {
        if let Some(place) = app.views.get_mut(index).and_then(|view| view.stamp_place.as_mut())
            && let Some(seal) = place.seal.as_mut()
        {
            seal.effects = effects;
        }
        None
    } else {
        None
    };
    if let Some(error) = changed_error {
        app.notify(error);
    }
    if store || refresh || another || revert {
        app.seal_approval = None;
    }
    if another {
        let error = app.views.get_mut(index).and_then(|view| view.stamp_place.as_mut()).and_then(|place| {
            if let Some(seal) = place.seal.as_mut() {
                seal.seed = next_effect_seed(seal.seed);
            }
            refresh_seal_preview(place);
            place.seal.as_ref().and_then(|seal| seal.error.clone())
        });
        if let Some(error) = error {
            app.notify(error);
        }
    }
    let revert_error = if revert {
        app.views
            .get_mut(index)
            .and_then(|view| view.stamp_place.as_mut())
            .and_then(|place| set_place_effects(place, crate::seal_store::SealEffects::off()))
    } else {
        None
    };
    if let Some(error) = revert_error {
        app.notify(error);
    }
    if save {
        // The window may be showing a migrated manual 0° while the draft still has an old
        // switched-off record. Saving stores the settings on screen.
        let _ = app.views.get_mut(index).and_then(|view| view.stamp_place.as_mut()).and_then(|place| set_place_effects(place, effects));
        app.save_place_effects();
    }
    if let Some(place) = app.views.get(index).and_then(|view| view.stamp_place.as_ref()) {
        ui.add_space(8.0);
        seal_compare(ui, place);
    }
    ui.add_space(8.0);
    widgets::pill_button(ui, "閉じる", true).clicked()
}

/// 0 fits the seal, 1 is one source pixel per point, 2 is twice that.
const COMPARE_FIT: u8 = 0;
const COMPARE_100: u8 = 1;
const COMPARE_200: u8 = 2;

/// Before and after, at the same scale and centre. Zoom is display-only and does not rebuild the ink.
fn seal_compare(ui: &mut egui::Ui, place: &StampPlace) {
    let Some(seal) = place.seal.as_ref() else { return };
    let (sw, sh, source) = &seal.source;
    if *sw == 0 || *sh == 0 || source.len() != (*sw as usize).saturating_mul(*sh as usize).saturating_mul(4) {
        return;
    }
    let (aw, ah, after) = place.pixels.as_ref().map(|(w, h, px)| (*w, *h, px.as_slice())).unwrap_or((*sw, *sh, source.as_slice()));
    if after.len() != (aw as usize).saturating_mul(ah as usize).saturating_mul(4) {
        return;
    }
    let zoom_id = ui.id().with("seal-compare-zoom");
    let mut zoom = ui.data(|data| data.get_temp::<u8>(zoom_id)).unwrap_or(COMPARE_FIT);
    ui.horizontal(|ui| {
        ui.label("比較");
        if ui.selectable_label(zoom == COMPARE_FIT, "全体").clicked() {
            zoom = COMPARE_FIT;
        }
        if ui.selectable_label(zoom == COMPARE_100, "100%").clicked() {
            zoom = COMPARE_100;
        }
        if ui.selectable_label(zoom == COMPARE_200, "200%").clicked() {
            zoom = COMPARE_200;
        }
    });
    ui.data_mut(|data| data.insert_temp(zoom_id, zoom));
    let pad_x = aw.saturating_sub(*sw) / 2;
    let pad_y = ah.saturating_sub(*sh) / 2;
    let avail = ui.available_width().max(1.0);
    let side_by_side = avail >= 460.0;
    let cell = if side_by_side { (avail - 16.0) * 0.5 } else { avail };
    let scale = match zoom {
        COMPARE_100 => 1.0,
        COMPARE_200 => 2.0,
        _ => (cell / (aw as f32)).min(168.0 / (ah as f32)).clamp(0.02, 8.0),
    };
    let frame = egui::vec2(aw as f32 * scale, ah as f32 * scale);
    let before_tex = preview_texture(ui.ctx(), "seal-compare-before", seal.source_gen, *sw, *sh, source);
    let after_tex = preview_texture(ui.ctx(), "stamp-place-preview", place.preview_gen, aw, ah, after);
    let paint = |ui: &mut egui::Ui| {
        let one = |ui: &mut egui::Ui, caption: &str, texture: &egui::TextureHandle, inset: bool| {
            ui.vertical(|ui| {
                ui.label(caption);
                let (rect, _) = ui.allocate_exact_size(frame, egui::Sense::hover());
                ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::WHITE);
                let image_rect = if inset {
                    egui::Rect::from_min_size(
                        rect.min + egui::vec2(pad_x as f32 * scale, pad_y as f32 * scale),
                        egui::vec2(*sw as f32 * scale, *sh as f32 * scale),
                    )
                } else {
                    rect
                };
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(texture.id(), image_rect, uv, egui::Color32::WHITE);
            });
        };
        if side_by_side {
            ui.horizontal(|ui| {
                one(ui, "加工前", &before_tex, true);
                one(ui, "加工後", &after_tex, false);
            });
        } else {
            one(ui, "加工前", &before_tex, true);
            one(ui, "加工後", &after_tex, false);
        }
    };
    if zoom == COMPARE_FIT {
        paint(ui);
    } else {
        egui::ScrollArea::both().max_height(300.0).auto_shrink([false, false]).show(ui, paint);
    }
}

/// A slider or number field stores its value while the pointer is down and rebuilds the preview
/// when the gesture ends, or immediately for a click or a typed number.
fn control_commits(changed: bool, dragged: bool, stopped: bool, other_live: bool) -> (bool, bool) {
    let live = dragged || other_live;
    (changed || dragged, stopped || (changed && !live))
}

fn effect_number(ui: &mut egui::Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>, speed: f64, label: &str, suffix: &str) -> (bool, bool) {
    let mut store = false;
    let mut refresh = false;
    ui.horizontal(|ui| {
        let slider = ui.add(egui::Slider::new(value, range.clone()).text(label));
        let mut drag = egui::DragValue::new(value).range(range.clone()).speed(speed);
        if !suffix.is_empty() {
            drag = drag.suffix(suffix);
        }
        let typed = ui.add(drag);
        let (slider_store, slider_refresh) = control_commits(slider.changed(), slider.dragged(), slider.drag_stopped(), typed.dragged());
        let (typed_store, typed_refresh) = control_commits(typed.changed(), typed.dragged(), typed.drag_stopped(), slider.dragged());
        store = slider_store || typed_store;
        refresh = slider_refresh || typed_refresh;
    });
    let low = *range.start();
    let high = *range.end();
    *value = if value.is_finite() { value.clamp(low, high) } else { 0.0 };
    (store, refresh)
}

/// What migrating the format-1 seal file means, before anything is changed. Returns
/// (migrate, postpone).
pub(crate) fn seal_migrate_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> (bool, bool) {
    let (seals, events, error) = match &app.seals.state {
        crate::seal_service::SealState::NeedsMigration { seals, events, error } => (*seals, *events, error.clone()),
        crate::seal_service::SealState::Deferred { seals, events } => (*seals, *events, None),
        _ => return (false, widgets::pill_button(ui, "閉じる", false).clicked()),
    };
    ui.set_width(520.0);
    ui.label(egui::RichText::new("理事長印のデータの移行").font(theme::semibold(18.0)));
    ui.add_space(6.0);
    for line in seal_migration_disclosures(seals, events) {
        ui.label(line);
    }
    if let Some(error) = error {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(error).color(egui::Color32::from_rgb(176, 48, 48)));
    }
    ui.add_space(8.0);
    let (mut migrate, mut later) = (false, false);
    let idle = !app.seals.running();
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(idle, |ui| widgets::pill_button(ui, "移行する", true)).inner.clicked() {
            migrate = true;
        }
        if widgets::pill_button(ui, "延期", false).clicked() {
            later = true;
        }
    });
    (migrate, later)
}

/// Everything the user is told before agreeing to migrate.
pub(crate) fn seal_migration_disclosures(seals: usize, events: usize) -> [String; 6] {
    [
        format!("旧形式の理事長印のデータがあります（理事長印 {seals} 件、使用履歴 {events} 件）。"),
        "このバージョンで理事長印を使うには、新しい形式への変換が必要です。".into(),
        "移行後のデータは、以前の SwiftPDF では使えません。".into(),
        "元の暗号化ファイルは削除せず、バックアップとして同じフォルダーに残します。".into(),
        "移行は一方向です。新しい形式から旧形式へは戻しません。".into(),
        "今は移行せずに延期できます。延期しても、データは変更しません。".into(),
    ]
}

pub(crate) fn delete_seal_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> (bool, bool) {
    let Some(target) = app.seal_delete.clone() else { return (false, true) };
    let current = app.seals.seals().iter().find(|seal| seal.id == target.id);
    let name = current.map(|seal| seal.name.clone()).unwrap_or_default();
    let same = current.is_some_and(|seal| seal.revision == target.revision && seal.sha256 == target.sha256);
    ui.label(egui::RichText::new("理事長印を削除").font(theme::semibold(18.0)));
    ui.add_space(6.0);
    ui.label(format!("「{name}」を暗号化ファイルから削除します。使用履歴は削除しません。"));
    ui.label("元の画像ファイルや、OS のバックアップに残ったデータは削除されません。");
    if !app.seals.usable() || !same {
        let why = if app.seals.usable() {
            "この理事長印は確認の後に変更または削除されました。".to_string()
        } else {
            app.seals.unusable_reason()
        };
        ui.label(why);
        return (false, widgets::pill_button(ui, "閉じる", false).clicked());
    }
    ui.add_space(8.0);
    let (mut ok, mut cancel) = (false, false);
    let idle = !app.seals.running();
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(idle, |ui| widgets::pill_button(ui, "削除", true)).inner.clicked() {
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
    pub seals: &'a [crate::seal_store_v2::Seal],
    pub date_stamps: &'a [crate::stamp_preset::DateStamp],
    /// Why chairman seals cannot be used now, if they cannot.
    pub seal_unusable: Option<String>,
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
            effects: crate::seal_store::SealEffects::off(),
            seal_ref: None,
        }),
        QuickTool::ChairmanSeal(i) => {
            if let Some(why) = inputs.seal_unusable {
                return Err(why);
            }
            let Some(seal) = inputs.seals.get(i) else { return Err("スタンプが選ばれていません。".into()) };
            let Some(png) = seal.png() else { return Err("理事長印の画像を読み込めませんでした。".into()) };
            Some(Armed {
                name: seal.name.clone(),
                file: seal.file.clone(),
                data: std::sync::Arc::new(png),
                source_page: 0,
                chairman: true,
                scale: seal.scale,
                source: PlaceSource::Chairman(i),
                overlay: None,
                effects: seal.effects,
                seal_ref: Some(crate::seal_service::SealRef::of(seal)),
            })
        }
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
                effects: crate::seal_store::SealEffects::off(),
                seal_ref: None,
            }
        }),
        _ => None,
    };
    let Some(armed) = armed else { return Err("スタンプが選ばれていません。".into()) };
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
        preview_gen: fresh_preview_gen(),
        drag: None,
        scale,
        nominal,
        source: armed.source,
        circular,
        date_overlay: armed.overlay,
        rotation,
        seal: None,
        seal_ref: armed.seal_ref,
    };
    if place.chairman {
        if let Some((width, height, rgba)) = place.pixels.clone() {
            place.seal = Some(SealDraft {
                source: (width, height, rgba),
                effects: armed.effects,
                seed: next_effect_seed(0),
                angle_deg: 0.0,
                axis: (1.0, 1.0),
                ink_rect: place.rect,
                error: None,
                applied: None,
                source_gen: fresh_preview_gen(),
            });
            refresh_seal_preview(&mut place);
        } else {
            place.seal = Some(SealDraft {
                source: (0, 0, Vec::new()),
                effects: armed.effects,
                seed: 1,
                angle_deg: 0.0,
                axis: (1.0, 1.0),
                ink_rect: place.rect,
                error: Some("印影を表示できません。設定を見直すか、キャンセルしてください。".into()),
                applied: None,
                source_gen: fresh_preview_gen(),
            });
        }
    }
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
                seals: self.seals.seals(),
                date_stamps: &self.date_stamps,
                seal_unusable: (!self.seals.usable()).then(|| self.seals.unusable_reason()),
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
            PlaceSource::Chairman(_) => match place.seal_ref.as_ref() {
                Some(seal) => self.begin_seal_update(seal, crate::seal_store_v2::SealChange::Scale(scale), "この倍率を既定にしました。"),
                None => self.notify("理事長印が見つかりません。"),
            },
            PlaceSource::Date(i) => {
                if let Some(stamp) = self.date_stamps.get_mut(i) {
                    stamp.scale = scale;
                    self.notify("この倍率を既定にしました。");
                }
            }
        }
    }

    pub(crate) fn save_place_effects(&mut self) {
        let Some(index) = self.active else { return };
        let Some(place) = self.views[index].stamp_place.clone() else { return };
        let PlaceSource::Chairman(_) = place.source else { return };
        let Some(effects) = place.seal.as_ref().map(|seal| seal.effects) else { return };
        let Some(seal) = place.seal_ref.as_ref() else { return };
        self.begin_seal_update(seal, crate::seal_store_v2::SealChange::Effects(effects), "この加工設定を既定にしました。");
    }

    pub(crate) fn ask_stamp_confirm(&mut self) {
        if self.front_blocks_stamp_commit() {
            return;
        }
        let Some(index) = self.active else { return };
        let Some(place) = self.views[index].stamp_place.clone() else { return };
        if let Some(why) = self.stamp_refusal(index) {
            self.notify(why);
            return;
        }
        if place.chairman {
            self.seal_approval = None;
            if let Some(why) = commit_block_message(&place) {
                self.notify(why);
                return;
            }
            self.dialog = Some(Dialog::ConfirmSeal);
            return;
        }
        self.commit_stamp_place();
    }

    /// A prompt other than the chairman-seal confirmation is in front. Commit and the
    /// confirmation itself wait until that prompt is gone.
    fn front_blocks_stamp_commit(&self) -> bool {
        match self.front_prompt() {
            None => false,
            Some(crate::PromptLayer::Dialog) => !matches!(self.dialog, Some(Dialog::ConfirmSeal)),
            Some(_) => true,
        }
    }

    pub(crate) fn commit_stamp_place(&mut self) {
        if self.front_blocks_stamp_commit() {
            return;
        }
        let Some(index) = self.active else { return };
        let Some(mut place) = self.views[index].stamp_place.clone() else { return };
        if let Some(why) = self.stamp_refusal(index) {
            self.notify(why);
            return;
        }
        if let Some(why) = commit_block_message(&place) {
            self.notify(why);
            return;
        }
        if place.date_overlay.as_ref().is_some_and(|overlay| overlay.drawn.is_some()) && !rebake_date(&mut place) {
            self.notify("日付を画像にできませんでした。");
            return;
        }
        let id = self.views[index].id;
        let file_name = self.session.get(id).map(|doc| doc.name.clone()).unwrap_or_default();
        let bytes = match stamp_image_bytes(&place) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.seal_approval = None;
                self.notify(error);
                return;
            }
        };
        let (processed, angle_deg, fade, seed, rotation_mode) = place
            .seal
            .as_ref()
            .map(|seal| {
                let processed = seal_writes_processed(seal);
                let angle_deg = if processed { seal.angle_deg } else { 0.0 };
                let fade = if processed { seal.effects.fade } else { 0.0 };
                (processed, angle_deg, fade, seal.seed, rotation_mode_name(&seal.effects))
            })
            .unwrap_or((false, 0.0, 0.0, 0, ""));
        if place.chairman {
            let approved = self
                .seal_approval
                .as_ref()
                .is_some_and(|approval| approval_matches(approval, id, &place) && approval.sha256 == sha256_bytes(bytes.as_ref()));
            if !approved {
                let had = self.seal_approval.take();
                if had.is_none() {
                    self.notify("承認されていないため、押印しませんでした。");
                } else {
                    self.notify("承認の内容が変わったため、押印しませんでした。もう一度確認してください。");
                }
                return;
            }
            if !self.seals.usable() {
                self.notify(format!("押印しませんでした。{}", self.seals.unusable_reason()));
                return;
            }
            let Some(seal) = place.seal_ref.clone().filter(|seal| self.seals.matches(seal)) else {
                self.seal_approval = None;
                self.notify("この理事長印は仮置きの後に変更または削除されたため、押印しませんでした。仮置きをやり直してください。");
                return;
            };
            let op_id = match crate::seal_store_v2::new_id("op") {
                Ok(op_id) => op_id,
                Err(error) => {
                    self.notify(format!("押印しませんでした。{error}"));
                    return;
                }
            };
            let details = crate::seal_store_v2::UseDetails {
                file: file_name.clone(),
                page: place.page.saturating_add(1) as u32,
                image_sha256: sha256_hex(bytes.as_ref()),
                processed,
                angle_deg,
                fade,
                seed,
                rotation_mode: rotation_mode.to_string(),
            };
            let store_id = self.seals.snapshot.as_ref().map(|snapshot| snapshot.store_id.clone()).unwrap_or_default();
            let op = crate::seal_service::StampOp {
                doc: id,
                op_id,
                seal,
                place,
                bytes,
                file_name,
                store_id,
                image_sha256: details.image_sha256.clone(),
                annot_nm: String::new(),
            };
            self.begin_seal_stamp(op, details);
            return;
        }
        let edit = pdfcraft_engine::Edit::AddCustomStamp {
            page: place.page,
            rect: place.rect,
            name: place.name.clone(),
            file: pdfcraft_engine::MarkFile { name: place.file.clone(), bytes, page: place.source_page },
            author: self.comment_prefs.author.clone(),
            opacity: place.opacity.clamp(0.0, 1.0),
        };
        if !self.apply_edit(edit) {
            return;
        }
        if let Some(view) = self.views.get_mut(index) {
            view.stamp_place = None;
        }
        self.seal_approval = None;
        self.quick_tool = QuickTool::Select;
    }

    pub(crate) fn delete_selected_seal(&mut self) {
        let Some(seal) = self.seal_delete.take() else { return };
        if !self.seals.usable() {
            self.notify(self.seals.unusable_reason());
            return;
        }
        self.begin_seal_delete(seal);
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
        let seals = crate::seal_flow::testing::SealDir::new("scale-type");
        let seal_dir = seals.path().to_path_buf();
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
            let mut app = crate::PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("opened.pdf", Some(path_s), bytes).unwrap();
            app.set_view_only(true);
            app.grant_stamp();
            app.grant_edit_text();
            app.grant_save_as();
            app.set_option("zoom", "100").unwrap();
            app.set_seal_dir(seal_dir);
            app
        });
        h.run_steps(4);
        crate::seal_flow::testing::settle_frames(&mut h);
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
        assert!(!h.state().seals.running(), "cancel registers nothing");
        assert!(h.state().seals.seals().is_empty());
        assert!(!seals.path().join(crate::seal_store_v2::V2_FILE).exists(), "cancel creates no store");
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

#[cfg(test)]
mod effect_preview_tests {
    use super::*;

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

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut png = Vec::new();
        let mut image = image::RgbaImage::new(width, height);
        for (x, y, px) in image.enumerate_pixels_mut() {
            let edge = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
            *px = if edge { image::Rgba([0, 0, 0, 0]) } else { image::Rgba([180, 20, 20, 255]) };
        }
        image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        png
    }

    fn seal(effects: crate::seal_store::SealEffects) -> crate::seal_store_v2::Seal {
        use base64::Engine as _;
        let data = png(48, 32);
        crate::seal_store_v2::Seal {
            id: "seal-00000000000000000000000000000000".into(),
            revision: 1,
            name: "試験印".into(),
            file: "seal.png".into(),
            width: 48,
            height: 32,
            sha256: sha256_hex(&data),
            data: base64::engine::general_purpose::STANDARD.encode(&data),
            scale: 100.0,
            effects,
        }
    }

    /// The placement as the app's own registered seal, so the store accepts its approval.
    fn chairman_in(app: &crate::PdfCraftApp, effects: crate::seal_store::SealEffects, seed: u64, rotation: u16) -> StampPlace {
        let mut place = chairman(effects, seed, rotation);
        let stored = app.seals.seals().first().expect("a registered seal");
        assert_eq!(stored.png().unwrap(), place.data.as_ref().clone(), "the same image as registered");
        place.seal_ref = Some(crate::seal_service::SealRef::of(stored));
        place
    }

    fn uses(app: &crate::PdfCraftApp) -> Vec<crate::seal_store_v2::Event> {
        app.seals.snapshot.as_ref().map(|s| s.events.iter().filter(|e| e.kind.starts_with("use_")).cloned().collect()).unwrap_or_default()
    }

    fn last_use(app: &crate::PdfCraftApp) -> crate::seal_store_v2::Event {
        uses(app).pop().expect("a use line")
    }

    fn note(event: &crate::seal_store_v2::Event) -> crate::seal_store_v2::Note {
        event.note.clone().unwrap_or_default()
    }

    fn registered_png(app: &crate::PdfCraftApp) -> Vec<u8> {
        app.seals.seals()[0].png().unwrap()
    }

    fn chairman(effects: crate::seal_store::SealEffects, seed: u64, rotation: u16) -> StampPlace {
        let seal = seal(effects);
        let bytes = seal.png().unwrap();
        let mut place = build_stamp_place(
            ArmedInputs {
                tool: QuickTool::ChairmanSeal(0),
                custom_stamps: &[],
                seals: std::slice::from_ref(&seal),
                date_stamps: &[],
                seal_unusable: None,
            },
            0,
            [100.0, 150.0],
            rotation,
        )
        .unwrap();
        assert_eq!(place.data.as_ref(), &bytes);
        let draft = place.seal.as_mut().unwrap();
        draft.seed = seed;
        refresh_seal_preview(&mut place);
        assert_eq!(place.seal.as_ref().unwrap().source.2, crate::seal_store::preview_rgba(&bytes).unwrap().2);
        place
    }

    fn center(rect: [f64; 4]) -> [f64; 2] {
        [(rect[0] + rect[2]) * 0.5, (rect[1] + rect[3]) * 0.5]
    }

    #[test]
    fn effects_off_keep_the_original_preview_rectangle_and_png() {
        let bytes = png(48, 32);
        let place = chairman(crate::seal_store::SealEffects::off(), 7, 0);
        let draft = place.seal.as_ref().unwrap();
        assert!(draft.effects.is_passthrough());
        assert_eq!(draft.angle_deg, 0.0);
        assert_eq!(place.rect, draft.ink_rect);
        assert_eq!(place.pixels.as_ref().map(|(_, _, px)| px.clone()), Some(draft.source.2.clone()));
        assert_eq!(place.data.as_slice(), bytes.as_slice());
        assert!(commit_block_message(&place).is_none());
        assert_eq!(
            crate::stamp_preset::size_label(measured_rect(&place), place.circular),
            crate::stamp_preset::size_label(place.rect, place.circular)
        );
        assert!((place.scale - 100.0).abs() < 1e-6);
    }

    #[test]
    fn effects_on_preview_keeps_centre_scale_opacity_and_source() {
        let mut place = chairman(crate::seal_store::SealEffects::enabled_preset(), 5, 0);
        place.opacity = 0.35;
        apply_place_scale(&mut place, 80.0);
        let ink_center = center(measured_rect(&place));
        let png = place.data.clone();
        let source = place.seal.as_ref().unwrap().source.2.clone();
        let error = set_place_effects(
            &mut place,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 10.0, fade: 10.0, bleed: 5.0, manual: false, manual_deg: 0.0 },
        );
        assert!(error.is_none(), "{error:?}");
        let draft = place.seal.as_ref().unwrap();
        assert!(draft.angle_deg.abs() <= 10.0 + 1e-9);
        assert!(commit_block_message(&place).is_none());
        assert!(!place.seal.as_ref().unwrap().effects.is_passthrough());
        assert!((place.opacity - 0.35).abs() < 1e-9);
        assert!((place.scale - 80.0).abs() < 1e-6);
        assert_eq!(center(draft.ink_rect), ink_center);
        assert_eq!(center(place.rect), ink_center);
        assert_eq!(place.data.as_ref(), png.as_ref());
        assert_eq!(draft.source.2, source);
        assert_ne!(place.pixels.as_ref().map(|(_, _, px)| px.as_slice()), Some(source.as_slice()));
        assert_eq!(
            crate::stamp_preset::size_label(measured_rect(&place), place.circular),
            crate::stamp_preset::size_label(draft.ink_rect, place.circular)
        );
    }

    #[test]
    fn another_seed_changes_the_pattern_and_the_same_seed_repeats_it() {
        let mut place = chairman(crate::seal_store::SealEffects::enabled_preset(), 11, 0);
        let first = place.pixels.clone();
        let angle = place.seal.as_ref().unwrap().angle_deg;
        let seed = place.seal.as_ref().unwrap().seed;
        place.seal.as_mut().unwrap().seed = next_effect_seed(seed);
        refresh_seal_preview(&mut place);
        assert_ne!(place.seal.as_ref().unwrap().seed, seed);
        assert_ne!(place.pixels, first);
        place.seal.as_mut().unwrap().seed = 11;
        refresh_seal_preview(&mut place);
        assert_eq!(place.pixels, first);
        assert_eq!(place.seal.as_ref().unwrap().angle_deg, angle);
        assert_eq!(place.data.as_slice(), png(48, 32).as_slice());
    }

    #[test]
    fn page_rotation_keeps_the_ink_box_and_puts_padding_outside_it() {
        for rotation in [90u16, 270] {
            let off = chairman(crate::seal_store::SealEffects::off(), 3, rotation);
            let nominal = crate::stamp_preset::nominal_size(rotation, Some((48, 32)));
            let expected = crate::stamp_preset::place_rect([100.0, 150.0], nominal, 100.0);
            assert_eq!(off.seal.as_ref().unwrap().ink_rect, expected);
            assert_eq!(off.rect, expected);
            let on = chairman(
                crate::seal_store::SealEffects { enabled: true, rotation_deg: 10.0, fade: 0.0, bleed: 0.0, manual: false, manual_deg: 0.0 },
                5,
                rotation,
            );
            let draft = on.seal.as_ref().unwrap();
            assert_eq!(draft.ink_rect, expected);
            assert_eq!(center(on.rect), center(draft.ink_rect));
            assert!(draft.angle_deg.abs() > 0.2, "{}", draft.angle_deg);
            assert!(on.rect[2] - on.rect[0] > expected[2] - expected[0] || on.rect[3] - on.rect[1] > expected[3] - expected[1]);
            assert_eq!(crate::stamp_preset::size_label(measured_rect(&on), on.circular), crate::stamp_preset::size_label(expected, on.circular));
        }
    }

    #[test]
    fn a_general_stamp_and_a_date_stamp_are_not_processed() {
        let data = std::sync::Arc::new(png(48, 32));
        let stamp = CustomStamp { category: "試験".into(), name: "一般".into(), file: "a.png".into(), page: 0, data: data.clone(), scale: 80.0 };
        let place = build_stamp_place(
            ArmedInputs {
                tool: QuickTool::CustomStamp(0),
                custom_stamps: std::slice::from_ref(&stamp),
                seals: &[],
                date_stamps: &[],
                seal_unusable: None,
            },
            0,
            [40.0, 50.0],
            90,
        )
        .unwrap();
        assert!(place.seal.is_none());
        assert!(!place.chairman);
        assert!(commit_block_message(&place).is_none());
        assert!((place.scale - 80.0).abs() < 1e-6);
        assert_eq!(place.data.as_ref(), data.as_ref());
        let nominal = crate::stamp_preset::nominal_size(90, Some((48, 32)));
        assert_eq!(place.rect, crate::stamp_preset::place_rect([40.0, 50.0], nominal, 80.0));

        let dated = crate::stamp_preset::DateStamp {
            name: "日付".into(),
            file: "date.png".into(),
            data: data.clone(),
            scale: 100.0,
            region: crate::stamp_preset::default_region(),
            font: crate::stamp_preset::default_font(),
            color: crate::stamp_preset::default_color(),
            drawn: None,
        };
        let place = build_stamp_place(
            ArmedInputs {
                tool: QuickTool::DateStamp(0),
                custom_stamps: &[],
                seals: &[],
                date_stamps: std::slice::from_ref(&dated),
                seal_unusable: None,
            },
            0,
            [40.0, 50.0],
            0,
        )
        .unwrap();
        assert!(place.seal.is_none());
        assert!(place.date_overlay.is_some());
        assert!(commit_block_message(&place).is_none());
        assert_eq!(place.data.as_ref(), data.as_ref());
    }

    #[test]
    fn processed_chairman_seals_ask_for_approval_and_a_direct_commit_without_it_is_refused() {
        let mut app = crate::PdfCraftApp::new();
        let bytes = app.session.create_blank(200.0, 300.0, 1).unwrap();
        app.open_bytes("opened.pdf", None, bytes.to_vec()).unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        let original = png(48, 32);
        app.views[0].stamp_place = Some(chairman(crate::seal_store::SealEffects::enabled_preset(), 5, 0));
        let registered = app.views[0].stamp_place.as_ref().unwrap().data.clone();
        app.ask_stamp_confirm();
        assert!(matches!(app.dialog, Some(crate::Dialog::ConfirmSeal)), "{:?}", app.toast);
        app.commit_stamp_place();
        assert_eq!(app.toast.as_ref().map(|(text, _)| text.as_str()), Some("承認されていないため、押印しませんでした。"));
        assert!(app.views[0].stamp_place.is_some());
        assert!(!app.seals.running() && uses(&app).is_empty());
        assert_eq!(app.views[0].stamp_place.as_ref().unwrap().data.as_ref(), registered.as_ref());
        assert_eq!(registered.as_ref(), original.as_slice());
        app.views[0].stamp_place = Some(chairman(crate::seal_store::SealEffects::off(), 5, 0));
        app.toast = None;
        app.ask_stamp_confirm();
        assert!(matches!(app.dialog, Some(crate::Dialog::ConfirmSeal)));
        assert!(app.seal_approval.is_none());
        assert!(!app.seals.running() && uses(&app).is_empty());
    }

    #[test]
    fn signed_and_annotation_banned_pdfs_still_refuse_a_processed_seal() {
        let mut author = crate::PdfCraftApp::new();
        let blank = author.session.create_blank(200.0, 300.0, 1).unwrap();
        author.open_bytes("plain.pdf", None, blank.to_vec()).unwrap();
        let id = author.views[0].id;
        author
            .session
            .apply(
                id,
                pdfcraft_engine::Edit::Protect(pdfcraft_engine::Protection {
                    permissions_password: Some("owner".into()),
                    printing: pdfcraft_engine::Printing::None,
                    changes: pdfcraft_engine::Changes::None,
                    ..pdfcraft_engine::Protection::default()
                }),
            )
            .unwrap();
        let locked = author.session.get(id).unwrap().bytes.clone();
        let mut banned = crate::PdfCraftApp::new();
        banned.set_view_only(true);
        banned.grant_stamp();
        banned.open_bytes("locked.pdf", None, locked.to_vec()).unwrap();
        banned.quick_tool = QuickTool::ChairmanSeal(0);
        let refused = banned.begin_stamp_place(0, 0, [40.0, 40.0], 0).unwrap_err();
        assert!(refused.contains("注釈"), "{refused}");
        assert!(!banned.seals.running() && uses(&banned).is_empty());

        let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
        let identity = pdfcraft_engine::sign::pkcs12::open(&p12, "test").unwrap();
        let plain = pdfcraft_engine::Session::new().create_blank(200.0, 300.0, 1).unwrap();
        let mut signer = pdfcraft_engine::Session::new().with_clock(|| 1_800_000_000);
        let sid = signer.open("plain.pdf", None, plain, None).unwrap();
        let signed = signer.sign(sid, &identity, pdfcraft_engine::SignOptions { rect: None, ..Default::default() }).unwrap();
        let mut signed_app = crate::PdfCraftApp::new();
        signed_app.set_view_only(true);
        signed_app.grant_stamp();
        signed_app.open_bytes("signed.pdf", None, signed.to_vec()).unwrap();
        signed_app.quick_tool = QuickTool::ChairmanSeal(0);
        let refused = signed_app.begin_stamp_place(0, 0, [40.0, 40.0], 0).unwrap_err();
        assert!(refused.contains("電子署名"), "{refused}");
        assert!(!signed_app.seals.running() && uses(&signed_app).is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn saving_the_default_reloads_and_a_failed_write_keeps_the_old_one() {
        use crate::seal_flow::testing::{SealDir, connect, fail_writes_after};
        let dir = SealDir::new("preview");
        let path = dir.path().join(crate::seal_store_v2::V2_FILE);
        let stored_seal = |dir: &SealDir| match crate::seal_store_v2::SealVault::new(dir.path()).state().unwrap() {
            crate::seal_store_v2::StoreState::Ready { snapshot, .. } => snapshot.seals[0].clone(),
            _ => panic!("no store"),
        };
        let mut app = crate::PdfCraftApp::new();
        let bytes = app.session.create_blank(200.0, 300.0, 1).unwrap();
        app.open_bytes("opened.pdf", None, bytes.to_vec()).unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        connect(&mut app, &dir);
        app.start_chairman_seal("seal.png".into(), png(48, 32));
        app.save_custom_stamp();
        app.seal_settle();
        assert_eq!(app.seals.seals()[0].effects, crate::seal_store::SealEffects::off());
        app.quick_tool = QuickTool::ChairmanSeal(0);
        app.begin_stamp_place(0, 0, [80.0, 90.0], 0).unwrap();
        let stored = std::fs::read(&path).unwrap();
        let preset = crate::seal_store::SealEffects::enabled_preset();
        let error = app.views[0].stamp_place.as_mut().and_then(|place| set_place_effects(place, preset));
        assert!(error.is_none(), "{error:?}");
        assert_eq!(std::fs::read(&path).unwrap(), stored, "editing the preview must not rewrite the store");
        assert_eq!(app.seals.seals()[0].effects, crate::seal_store::SealEffects::off());
        app.save_place_effects();
        app.seal_settle();
        assert_eq!(app.seals.seals()[0].effects, preset, "{:?}", app.toast);
        assert_eq!(registered_png(&app), png(48, 32));
        let reloaded = stored_seal(&dir);
        assert_eq!(reloaded.effects, preset);
        assert_eq!(reloaded.png().unwrap(), png(48, 32));
        assert_eq!(
            app.views[0].stamp_place.as_ref().and_then(|place| place.seal_ref.clone()),
            Some(crate::seal_service::SealRef::of(&app.seals.seals()[0])),
            "the placement follows the revision this window saved"
        );
        fail_writes_after(&app, Some(0));
        let _ = app.views[0].stamp_place.as_mut().and_then(|place| set_place_effects(place, crate::seal_store::SealEffects::off()));
        app.save_place_effects();
        app.seal_settle();
        fail_writes_after(&app, None);
        assert_eq!(app.toast.as_ref().map(|(text, _)| text.as_str()), Some("保存しませんでした。test: stopped"));
        assert_eq!(app.seals.seals()[0].effects, preset);
        assert_eq!(stored_seal(&dir).effects, preset);
    }

    #[test]
    fn approval_hash_is_sha256() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    fn render_page(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut renderer = pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes.to_vec()), Default::default());
        let shot = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 2.0, ..Default::default() });
        assert!(shot.error.is_none(), "{:?}", shot.error);
        (shot.width, shot.height, shot.rgba.to_vec())
    }

    fn close_rect(place: [f64; 4], annotation: [f32; 4]) {
        for (got, want) in annotation.iter().zip(place) {
            assert!((f64::from(*got) - want).abs() < 0.05, "annotation {annotation:?} place {place:?}");
        }
    }

    fn fresh_seal_app(dir: &crate::seal_flow::testing::SealDir) -> crate::PdfCraftApp {
        let mut app = crate::PdfCraftApp::new();
        let blank = app.session.create_blank(240.0, 320.0, 1).unwrap();
        app.open_bytes("opened.pdf", None, blank.to_vec()).unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        app.grant_edit_text();
        crate::seal_flow::testing::connect(&mut app, dir);
        app.start_chairman_seal("seal.png".into(), png(48, 32));
        app.accept_stamp_registration();
        app.seal_settle();
        assert_eq!(registered_png(&app), png(48, 32), "{:?}", app.toast);
        app.quick_tool = QuickTool::ChairmanSeal(0);
        app
    }

    /// Approve the placement shown and run the whole stamp to its record.
    fn approve_and_commit(app: &mut crate::PdfCraftApp) {
        approve_current_seal(app).unwrap();
        app.commit_stamp_place();
        app.seal_settle();
    }

    fn step(app: &mut crate::PdfCraftApp, command: &str) {
        assert!(app.execute(command), "{:?}", app.toast);
        app.seal_settle();
    }

    fn commit_on_rotated_page(degrees: i64) {
        let dir = crate::seal_flow::testing::SealDir::new(&format!("turned-{degrees}"));
        let mut maker = pdfcraft_engine::Session::new();
        let blank = maker.create_blank(240.0, 320.0, 1).unwrap();
        let made = maker.open("turned.pdf", None, blank, None).unwrap();
        maker.apply(made, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees }).unwrap();
        let turned = maker.save_bytes(made).unwrap();
        let mut app = crate::PdfCraftApp::new();
        app.open_bytes("turned.pdf", None, turned.to_vec()).unwrap();
        let id = app.views[0].id;
        app.set_view_only(true);
        app.grant_stamp();
        app.grant_save_as();
        crate::seal_flow::testing::connect(&mut app, &dir);
        app.start_chairman_seal("seal.png".into(), png(48, 32));
        app.accept_stamp_registration();
        app.seal_settle();
        let registered = registered_png(&app);
        let place = chairman_in(
            &app,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 6.0, fade: 10.0, bleed: 5.0, manual: false, manual_deg: 0.0 },
            4,
            degrees as u16,
        );
        let rect = place.rect;
        let ink = place.seal.as_ref().unwrap().ink_rect;
        assert_eq!(center(rect), center(ink), "{degrees}");
        app.views[0].stamp_place = Some(place);
        approve_and_commit(&mut app);
        assert!(app.views[0].stamp_place.is_none(), "{degrees} {:?}", app.toast);
        let ann = app.session.get(id).unwrap().info.annotations.iter().find(|a| a.subtype == "Stamp").unwrap().rect;
        close_rect(rect, ann);
        let saved = app.session.save_edited_copy_bytes(id).unwrap();
        let mut reader = pdfcraft_engine::Session::new();
        let opened = reader.open("turned.pdf", None, saved, None).unwrap();
        let again = reader.get(opened).unwrap().info.annotations.iter().find(|a| a.subtype == "Stamp").unwrap().rect;
        close_rect(rect, again);
        assert_eq!(registered_png(&app), registered);
        let event = last_use(&app);
        assert_eq!(event.kind, "use_committed", "{degrees}");
        assert!(note(&event).processed, "{degrees}");
        assert!((note(&event).fade - 10.0).abs() < 1e-9, "{degrees}");
    }

    /// The registration window is in front until its OK button dismisses it. A stamp waits,
    /// and so does a commit while the permissions prompt covers the approval dialog.
    #[test]
    #[cfg(windows)]
    fn a_registration_dialog_blocks_stamping_until_ok_closes_it() {
        let _policy = policy();
        let dir = crate::seal_flow::testing::SealDir::new("front");
        let mut app = crate::PdfCraftApp::new();
        let blank = app.session.create_blank(240.0, 320.0, 1).unwrap();
        app.open_bytes("opened.pdf", None, blank.to_vec()).unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        crate::seal_flow::testing::connect(&mut app, &dir);
        let id = app.views[0].id;
        app.start_chairman_seal("seal.png".into(), png(48, 32));
        assert!(matches!(app.dialog, Some(crate::Dialog::CreateStamp)));
        app.views[0].stamp_place = Some(chairman(crate::seal_store::SealEffects::off(), 1, 0));
        app.ask_stamp_confirm();
        assert!(matches!(app.dialog, Some(crate::Dialog::CreateStamp)), "registration stays in front");
        assert!(app.seal_approval.is_none());
        app.commit_stamp_place();
        assert!(app.views[0].stamp_place.is_some(), "{:?}", app.toast);
        assert!(!app.seals.running() && uses(&app).is_empty());
        assert!(app.session.get(id).unwrap().info.annotations.iter().all(|a| a.subtype != "Stamp"));

        app.accept_stamp_registration();
        app.seal_settle();
        assert!(app.dialog.is_none(), "{:?}", app.toast);
        app.views[0].stamp_place = Some(chairman_in(&app, crate::seal_store::SealEffects::off(), 1, 0));
        app.ask_stamp_confirm();
        assert!(matches!(app.dialog, Some(crate::Dialog::ConfirmSeal)));
        app.commit_stamp_place();
        assert_eq!(app.toast.as_ref().map(|(text, _)| text.as_str()), Some("承認されていないため、押印しませんでした。"));
        assert!(!app.seals.running() && uses(&app).is_empty());
        assert!(app.views[0].stamp_place.is_some());

        approve_current_seal(&mut app).unwrap();
        app.unlock_prompt = Some(crate::UnlockPrompt { doc: id, name: "opened.pdf".into(), input: String::new(), error: None, then: None });
        app.toast = None;
        app.commit_stamp_place();
        assert!(app.toast.is_none(), "a prompt in front of the approval dialog does not commit");
        assert!(app.views[0].stamp_place.is_some());
        assert!(!app.seals.running() && uses(&app).is_empty());
        app.unlock_prompt = None;
        app.commit_stamp_place();
        app.seal_settle();
        assert!(app.views[0].stamp_place.is_none(), "{:?}", app.toast);
        assert_eq!(uses(&app).iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), crate::seal_flow::testing::COMMITTED);
    }

    /// Processed chairman seals commit the preview PNG. The registered image and an effect-off
    /// placement stay on the original bytes and rectangle.
    #[test]
    #[cfg(windows)]
    #[allow(clippy::too_many_lines)]
    fn a_processed_chairman_seal_commits_the_preview_png() {
        let _policy = policy();
        let dir = crate::seal_flow::testing::SealDir::new("commit");
        let mut app = fresh_seal_app(&dir);
        let registered = registered_png(&app);
        let id = app.views[0].id;

        app.views[0].stamp_place = Some(chairman_in(&app, crate::seal_store::SealEffects::off(), 5, 0));
        let off_rect = app.views[0].stamp_place.as_ref().unwrap().rect;
        approve_and_commit(&mut app);
        assert!(app.views[0].stamp_place.is_none(), "off commit {:?}", app.toast);
        let off_event = last_use(&app);
        assert_eq!(off_event.kind, "use_committed");
        assert!(!note(&off_event).processed);
        assert_eq!(note(&off_event).rotation_mode, "manual");
        assert_eq!(off_event.image_sha256, sha256_hex(&registered));
        assert!(!uses(&app).iter().any(|event| event.image_sha256.len() > 64));
        let ann = app.session.get(id).unwrap().info.annotations.iter().find(|a| a.subtype == "Stamp").unwrap().rect;
        close_rect(off_rect, ann);
        app.grant_save_as();
        let off_saved = app.session.save_edited_copy_bytes(id).unwrap();
        let (_, _, off_rgba) = render_page(off_saved.as_ref());
        assert_eq!(registered_png(&app), registered);

        step(&mut app, "edit.undo");
        step(&mut app, "edit.redo");
        let redone = last_use(&app);
        assert_eq!(redone.kind, "use_redone");
        assert_eq!(redone.image_sha256, off_event.image_sha256);
        assert_eq!(note(&redone).rotation_mode, note(&off_event).rotation_mode);
        assert!(!note(&redone).processed);
        let redone_saved = app.session.save_edited_copy_bytes(id).unwrap();
        let (_, _, redone_rgba) = render_page(redone_saved.as_ref());
        assert_eq!(redone_rgba, off_rgba, "undo and redo keep the stamped image");
        step(&mut app, "edit.undo");

        let mut worn = chairman_in(
            &app,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 30.0, manual: false, manual_deg: 0.0 },
            8,
            0,
        );
        worn.opacity = 0.4;
        let worn_png = stamp_image_bytes(&worn).unwrap();
        assert_ne!(worn_png.as_ref(), registered.as_slice(), "wear must not reuse the registered PNG");
        let ink = worn.seal.as_ref().unwrap().ink_rect;
        assert_eq!(center(worn.rect), center(ink));
        assert_eq!(crate::stamp_preset::size_label(measured_rect(&worn), worn.circular), crate::stamp_preset::size_label(ink, worn.circular));
        app.views[0].stamp_place = Some(worn.clone());
        approve_current_seal(&mut app).unwrap();
        app.views[0].stamp_place.as_mut().unwrap().opacity = 0.2;
        let history_before = uses(&app).len();
        app.commit_stamp_place();
        assert_eq!(
            app.toast.as_ref().map(|(text, _)| text.as_str()),
            Some("承認の内容が変わったため、押印しませんでした。もう一度確認してください。")
        );
        assert!(app.seal_approval.is_none());
        assert!(!app.seals.running());
        assert_eq!(uses(&app).len(), history_before);
        assert!(app.views[0].stamp_place.is_some());

        app.views[0].stamp_place.as_mut().unwrap().opacity = 0.4;
        approve_current_seal(&mut app).unwrap();
        let moved = app.views[0].stamp_place.as_ref().unwrap().rect;
        app.views[0].stamp_place.as_mut().unwrap().rect = [moved[0] + 3.0, moved[1], moved[2] + 3.0, moved[3]];
        app.commit_stamp_place();
        assert!(app.toast.as_ref().unwrap().0.contains("変わった"));
        assert!(!app.seals.running());
        assert_eq!(uses(&app).len(), history_before);

        app.views[0].stamp_place = Some(worn.clone());
        approve_current_seal(&mut app).unwrap();
        if let Some(place) = app.views[0].stamp_place.as_mut() {
            apply_place_scale(place, 80.0);
        }
        app.commit_stamp_place();
        assert!(app.toast.as_ref().unwrap().0.contains("変わった"), "{:?}", app.toast);
        assert!(!app.seals.running());
        assert_eq!(uses(&app).len(), history_before);

        app.views[0].stamp_place = Some(worn.clone());
        let committed_rect = app.views[0].stamp_place.as_ref().unwrap().rect;
        let preview = app.views[0].stamp_place.as_ref().unwrap().pixels.clone();
        approve_and_commit(&mut app);
        assert!(app.views[0].stamp_place.is_none(), "processed commit {:?}", app.toast);
        let event = last_use(&app);
        assert_eq!(event.kind, "use_committed");
        assert!(note(&event).processed);
        assert!((note(&event).fade - 30.0).abs() < 1e-9);
        assert_eq!(note(&event).seed, 8);
        assert_eq!(note(&event).angle_deg, 0.0);
        assert_eq!(note(&event).rotation_mode, "random");
        assert_eq!(event.image_sha256, sha256_hex(worn_png.as_ref()));
        assert_ne!(event.image_sha256, sha256_hex(&registered));
        let ann = app.session.get(id).unwrap().info.annotations.iter().rfind(|a| a.subtype == "Stamp").unwrap().rect;
        close_rect(committed_rect, ann);
        let (pw, ph, px) = preview.unwrap();
        assert_eq!(worn_png.as_slice(), crate::stamp_preset::encode_rgba_png(pw, ph, &px).unwrap().as_slice());
        let saved = app.session.save_edited_copy_bytes(id).unwrap();
        let mut reader = pdfcraft_engine::Session::new();
        let reopened = reader.open("copy.pdf", None, saved.clone(), None).unwrap();
        let again = reader.get(reopened).unwrap().info.annotations.iter().rfind(|a| a.subtype == "Stamp").unwrap().rect;
        close_rect(committed_rect, again);
        let (worn_w, worn_h, worn_rgba) = render_page(saved.as_ref());
        assert_ne!(worn_rgba, off_rgba, "the processed stamp must not look like the original PNG");
        let image = egui::ColorImage::from_rgba_unmultiplied([worn_w as usize, worn_h as usize], &worn_rgba);
        let printed = crate::canvas::print_preview_image(crate::canvas::print_preview_tag(1.0, false), image.clone());
        assert_eq!(printed.pixels, image.pixels, "print preview keeps the processed page");
        assert_eq!(registered_png(&app), registered);
        step(&mut app, "edit.undo");
        step(&mut app, "edit.redo");
        let worn_again = app.session.save_edited_copy_bytes(id).unwrap();
        let (_, _, worn_again_rgba) = render_page(worn_again.as_ref());
        assert_eq!(worn_again_rgba, worn_rgba, "undo and redo keep the processed image");
        let redone_worn = last_use(&app);
        assert_eq!(redone_worn.kind, "use_redone");
        assert!(note(&redone_worn).processed);
        assert_eq!(redone_worn.image_sha256, event.image_sha256);
        assert_eq!(note(&redone_worn).rotation_mode, note(&event).rotation_mode);

        let turned = chairman_in(
            &app,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 10.0, fade: 0.0, bleed: 0.0, manual: false, manual_deg: 0.0 },
            5,
            0,
        );
        let turned_png = stamp_image_bytes(&turned).unwrap();
        assert_ne!(turned.rect, turned.seal.as_ref().unwrap().ink_rect);
        assert_eq!(center(turned.rect), center(turned.seal.as_ref().unwrap().ink_rect));
        app.views[0].stamp_place = Some(turned.clone());
        approve_and_commit(&mut app);
        let turn_event = last_use(&app);
        assert_eq!(turn_event.kind, "use_committed", "{:?}", app.toast);
        assert!(note(&turn_event).processed);
        assert!(note(&turn_event).angle_deg.abs() > 0.2);
        assert_eq!(note(&turn_event).rotation_mode, "random");
        assert_eq!(note(&turn_event).fade, 0.0);
        assert_eq!(turn_event.image_sha256, sha256_hex(turned_png.as_ref()));

        let mut both = chairman_in(
            &app,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 8.0, fade: 20.0, bleed: 12.0, manual: false, manual_deg: 0.0 },
            9,
            0,
        );
        let both_seed = both.seal.as_ref().unwrap().seed;
        both.seal.as_mut().unwrap().seed = next_effect_seed(both_seed);
        refresh_seal_preview(&mut both);
        let both_png = stamp_image_bytes(&both).unwrap();
        app.views[0].stamp_place = Some(both);
        approve_and_commit(&mut app);
        let both_event = last_use(&app);
        assert_eq!(both_event.kind, "use_committed", "{:?} {}", app.toast, crate::seal_store_v2::last_store_io_notes());
        assert!(note(&both_event).processed);
        assert!((note(&both_event).fade - 20.0).abs() < 1e-9);
        assert_ne!(note(&both_event).seed, both_seed);
        assert_eq!(both_event.image_sha256, sha256_hex(both_png.as_ref()));
        assert_ne!(both_event.image_sha256, sha256_hex(&registered));

        for degrees in [90_i64, 270] {
            commit_on_rotated_page(degrees);
        }

        let mut broken = chairman_in(
            &app,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 0.0, manual: false, manual_deg: 0.0 },
            3,
            0,
        );
        broken.pixels = Some((4, 4, vec![1, 2, 3]));
        let kept = broken.data.clone();
        app.views[0].stamp_place = Some(broken);
        assert!(approve_current_seal(&mut app).is_err());
        let history_before = uses(&app).len();
        app.commit_stamp_place();
        assert!(app.toast.as_ref().unwrap().0.contains("画像にできません"), "{:?}", app.toast);
        assert!(!app.seals.running());
        assert_eq!(uses(&app).len(), history_before);
        assert_eq!(app.views[0].stamp_place.as_ref().unwrap().data.as_ref(), kept.as_ref());

        app.views[0].stamp_place = Some(chairman_in(
            &app,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 0.0, fade: 15.0, bleed: 0.0, manual: false, manual_deg: 0.0 },
            2,
            0,
        ));
        approve_current_seal(&mut app).unwrap();
        crate::seal_flow::testing::fail_writes_after(&app, Some(0));
        let stamps_before = app.session.get(id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count();
        let history_before = crate::seal_flow::testing::use_kinds_on_disk(&dir);
        app.commit_stamp_place();
        app.seal_settle();
        crate::seal_flow::testing::fail_writes_after(&app, None);
        assert!(app.toast.as_ref().unwrap().0.contains("使用履歴"), "{:?}", app.toast);
        assert_eq!(app.session.get(id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count(), stamps_before);
        assert!(app.views[0].stamp_place.is_some());
        assert_eq!(crate::seal_flow::testing::use_kinds_on_disk(&dir), history_before, "nothing about the refused stamp was written");
        assert_eq!(registered_png(&app), registered);
    }

    #[test]
    fn a_zero_turn_keeps_the_registered_png_and_a_new_pattern_keeps_a_manual_angle() {
        let idle = chairman(crate::seal_store::SealEffects::off(), 3, 0);
        assert_eq!(stamp_image_bytes(&idle).unwrap().as_ref(), idle.data.as_ref());
        assert_eq!(idle.rect, idle.seal.as_ref().unwrap().ink_rect);
        let legacy = crate::seal_store::SealEffects { enabled: false, rotation_deg: 2.0, fade: 10.0, bleed: 5.0, manual: false, manual_deg: 0.0 };
        let old = chairman(legacy, 1, 0);
        assert!(old.seal.as_ref().unwrap().effects.is_passthrough());
        assert_eq!(stamp_image_bytes(&old).unwrap().as_ref(), old.data.as_ref());
        assert_eq!(old.rect, old.seal.as_ref().unwrap().ink_rect);

        let mut sampled = chairman(
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 30.0, fade: 0.0, bleed: 0.0, manual: false, manual_deg: 0.0 },
            6,
            0,
        );
        sampled.seal.as_mut().unwrap().angle_deg = 0.0;
        sampled.seal.as_mut().unwrap().effects.fade = 0.0;
        assert_eq!(stamp_image_bytes(&sampled).unwrap().as_ref(), sampled.data.as_ref(), "a random draw of 0° must not re-encode the PNG");

        let mut fixed = chairman(
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 12.0, fade: 10.0, bleed: 0.0, manual: true, manual_deg: -15.0 },
            2,
            0,
        );
        let angle = fixed.seal.as_ref().unwrap().angle_deg;
        assert!((angle + 15.0).abs() < 1.0e-9, "{angle}");
        let shown = fixed.pixels.clone();
        fixed.seal.as_mut().unwrap().seed = next_effect_seed(fixed.seal.as_ref().unwrap().seed);
        refresh_seal_preview(&mut fixed);
        assert!((fixed.seal.as_ref().unwrap().angle_deg - angle).abs() < 1.0e-9);
        assert_eq!(fixed.seal.as_ref().unwrap().effects.manual_deg, -15.0);
        assert_ne!(fixed.pixels, shown, "wear still changes with the seed");

        let mut still = chairman(
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 0.0, manual: true, manual_deg: 15.0 },
            4,
            0,
        );
        let pixels = still.pixels.clone();
        still.seal.as_mut().unwrap().seed = next_effect_seed(4);
        refresh_seal_preview(&mut still);
        assert!((still.seal.as_ref().unwrap().angle_deg - 15.0).abs() < 1.0e-9);
        assert_eq!(still.pixels, pixels, "a new pattern does not move a fixed angle when there is no wear");
        assert_ne!(still.rect, still.seal.as_ref().unwrap().ink_rect);
        assert_eq!(center(still.rect), center(still.seal.as_ref().unwrap().ink_rect));
        assert_eq!(
            crate::stamp_preset::size_label(measured_rect(&still), still.circular),
            crate::stamp_preset::size_label(still.seal.as_ref().unwrap().ink_rect, still.circular)
        );
    }

    #[test]
    #[cfg(windows)]
    fn manual_rotation_commits_and_a_later_change_is_refused() {
        let _policy = policy();
        let dir = crate::seal_flow::testing::SealDir::new("manual");
        let mut app = fresh_seal_app(&dir);
        app.grant_save_as();
        let registered = registered_png(&app);
        let id = app.views[0].id;
        let manual = crate::seal_store::SealEffects { enabled: true, rotation_deg: 4.0, fade: 0.0, bleed: 2.0, manual: true, manual_deg: -30.0 };
        let place = chairman_in(&app, manual, 4, 0);
        let rect = place.rect;
        app.views[0].stamp_place = Some(place.clone());
        let history_before = uses(&app).len();
        app.commit_stamp_place();
        assert_eq!(app.toast.as_ref().map(|(text, _)| text.as_str()), Some("承認されていないため、押印しませんでした。"));
        assert!(!app.seals.running());
        assert_eq!(uses(&app).len(), history_before);

        app.views[0].stamp_place = Some(place.clone());
        approve_and_commit(&mut app);
        assert!(app.views[0].stamp_place.is_none(), "{:?}", app.toast);
        let event = last_use(&app);
        assert_eq!(event.kind, "use_committed");
        assert_eq!(note(&event).rotation_mode, "manual");
        assert!(note(&event).processed);
        assert!((note(&event).angle_deg + 30.0).abs() < 1.0e-6, "{}", note(&event).angle_deg);
        assert_eq!(note(&event).fade, 0.0);
        assert_eq!(event.image_sha256, sha256_hex(stamp_image_bytes(&place).unwrap().as_ref()));
        let ann = app.session.get(id).unwrap().info.annotations.iter().rfind(|a| a.subtype == "Stamp").unwrap().rect;
        close_rect(rect, ann);
        let saved = app.session.save_edited_copy_bytes(id).unwrap();
        let mut reader = pdfcraft_engine::Session::new();
        let opened = reader.open("copy.pdf", None, saved.clone(), None).unwrap();
        let again = reader.get(opened).unwrap().info.annotations.iter().rfind(|a| a.subtype == "Stamp").unwrap().rect;
        close_rect(rect, again);
        let (_, _, first) = render_page(saved.as_ref());
        step(&mut app, "edit.undo");
        step(&mut app, "edit.redo");
        let redone = app.session.save_edited_copy_bytes(id).unwrap();
        let (_, _, second) = render_page(redone.as_ref());
        assert_eq!(first, second);
        assert_eq!(last_use(&app).kind, "use_redone");
        assert_eq!(note(&last_use(&app)).rotation_mode, "manual");
        assert_eq!(registered_png(&app), registered);

        let mut next = chairman_in(
            &app,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 2.0, fade: 0.0, bleed: 0.0, manual: true, manual_deg: 15.0 },
            7,
            0,
        );
        app.views[0].stamp_place = Some(next.clone());
        approve_current_seal(&mut app).unwrap();
        next.seal.as_mut().unwrap().effects.manual_deg = 10.0;
        refresh_seal_preview(&mut next);
        app.views[0].stamp_place = Some(next);
        let history_before = uses(&app).len();
        app.commit_stamp_place();
        assert!(app.toast.as_ref().unwrap().0.contains("変わった"), "{:?}", app.toast);
        assert!(!app.seals.running());
        assert_eq!(uses(&app).len(), history_before);

        let mut random = chairman_in(&app, crate::seal_store::SealEffects::enabled_preset(), 8, 0);
        app.views[0].stamp_place = Some(random.clone());
        approve_current_seal(&mut app).unwrap();
        random.seal.as_mut().unwrap().effects.rotation_deg = 30.0;
        refresh_seal_preview(&mut random);
        app.views[0].stamp_place = Some(random.clone());
        app.commit_stamp_place();
        assert!(app.toast.as_ref().unwrap().0.contains("変わった"), "{:?}", app.toast);

        app.views[0].stamp_place = Some(random.clone());
        approve_current_seal(&mut app).unwrap();
        random.seal.as_mut().unwrap().effects.manual = true;
        random.seal.as_mut().unwrap().effects.manual_deg = 0.0;
        refresh_seal_preview(&mut random);
        app.views[0].stamp_place = Some(random.clone());
        app.commit_stamp_place();
        assert!(app.toast.as_ref().unwrap().0.contains("変わった"), "{:?}", app.toast);

        app.views[0].stamp_place = Some(random.clone());
        approve_current_seal(&mut app).unwrap();
        random.seal.as_mut().unwrap().seed = next_effect_seed(random.seal.as_ref().unwrap().seed);
        refresh_seal_preview(&mut random);
        app.views[0].stamp_place = Some(random.clone());
        app.commit_stamp_place();
        assert!(app.toast.as_ref().unwrap().0.contains("変わった"), "{:?}", app.toast);

        app.views[0].stamp_place = Some(random.clone());
        approve_current_seal(&mut app).unwrap();
        random.seal.as_mut().unwrap().effects.fade = 20.0;
        refresh_seal_preview(&mut random);
        app.views[0].stamp_place = Some(random);
        app.commit_stamp_place();
        assert!(app.toast.as_ref().unwrap().0.contains("変わった"), "{:?}", app.toast);

        assert!(!app.seals.running());
        assert_eq!(uses(&app).len(), history_before, "no changed approval reached the store");

        let wear = chairman_in(
            &app,
            crate::seal_store::SealEffects { enabled: true, rotation_deg: 0.0, fade: 12.0, bleed: 9.0, manual: true, manual_deg: 0.0 },
            11,
            0,
        );
        let wear_png = stamp_image_bytes(&wear).unwrap();
        assert_ne!(wear_png.as_ref(), registered.as_slice());
        app.views[0].stamp_place = Some(wear.clone());
        approve_and_commit(&mut app);
        assert!(app.views[0].stamp_place.is_none(), "{:?}", app.toast);
        let wear_event = last_use(&app);
        assert_eq!(wear_event.kind, "use_committed");
        assert_eq!(note(&wear_event).rotation_mode, "manual");
        assert!(note(&wear_event).processed);
        assert_eq!(note(&wear_event).angle_deg, 0.0);
        assert!((note(&wear_event).fade - 12.0).abs() < 1.0e-9);
        assert_eq!(wear_event.image_sha256, sha256_hex(wear_png.as_ref()));
    }

    #[test]
    fn the_place_bar_offers_effects_only_for_a_chairman_seal() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
            let mut app = crate::PdfCraftApp::new();
            let bytes = app.session.create_blank(200.0, 300.0, 1).unwrap();
            app.open_bytes("opened.pdf", None, bytes.to_vec()).unwrap();
            app.set_view_only(true);
            app.grant_stamp();
            app
        });
        h.run_steps(2);
        h.state_mut().views[0].stamp_place = Some(chairman(crate::seal_store::SealEffects::enabled_preset(), 5, 0));
        h.run_steps(2);
        assert!(h.query_by_label("印影効果…").is_some());
        assert!(h.query_by_label("加工済み印影の確定は次の実装段階で対応します").is_none());
        assert!(h.query_by_label("にじみ").is_none());
        h.get_by_label("印影効果…").click();
        h.run_steps(2);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::SealEffects)));
        assert!(h.query_by_label("手動回転").is_some());
        assert!(h.query_by_label("ランダム回転").is_some());
        assert!(h.query_by_label("別パターンを生成").is_some());
        assert!(h.query_by_label("加工前に戻す").is_some());
        assert!(h.query_by_label("この加工設定を既定にする").is_some());
        h.state_mut().dialog = None;
        let mut general = chairman(crate::seal_store::SealEffects::off(), 1, 0);
        general.chairman = false;
        general.seal = None;
        h.state_mut().views[0].stamp_place = Some(general);
        h.run_steps(2);
        assert!(h.query_by_label("印影効果…").is_none());

        h.state_mut().set_ui_mode(crate::UiMode::Simple);
        h.state_mut().views[0].stamp_place = Some(chairman(crate::seal_store::SealEffects::enabled_preset(), 5, 0));
        h.run_steps(2);
        h.get_by_label("印影効果…").click();
        h.run_steps(2);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::SealEffects)));
        assert!(h.query_by_label("手動回転").is_some());
        assert!(h.query_by_label("ランダム回転").is_some());
        assert!(h.query_by_label("別パターンを生成").is_some());
        assert!(h.query_by_label("加工前").is_some());
        assert!(h.query_by_label("加工後").is_some());
        let source = h.state().views[0].stamp_place.as_ref().unwrap().seal.as_ref().unwrap().source.clone();
        crate::seal_effect::testing::reset();
        h.get_by_label("100%").click();
        h.run_steps(1);
        h.get_by_label("200%").click();
        h.run_steps(1);
        h.get_by_label("全体").click();
        h.run_steps(1);
        assert_eq!(crate::seal_effect::testing::calls(), 0, "changing the comparison zoom must not rebuild the seal");
        assert_eq!(h.state().views[0].stamp_place.as_ref().unwrap().seal.as_ref().unwrap().source, source);
        let generation = h.state().views[0].stamp_place.as_ref().unwrap().preview_gen;
        let pixels = h.state().views[0].stamp_place.as_ref().unwrap().pixels.clone();
        crate::seal_effect::testing::reset();
        let title = h.get_by_label("印影効果").rect();
        // The accessible name covers the whole window. Only the title strip moves it.
        let from = egui::pos2(title.min.x + 48.0, title.min.y + 18.0);
        let to = from + egui::vec2(140.0, 70.0);
        h.hover_at(from);
        h.run_steps(1);
        h.drag_at(from);
        h.run_steps(1);
        h.hover_at(to);
        h.run_steps(2);
        h.drop_at(to);
        h.run_steps(2);
        assert_eq!(crate::seal_effect::testing::calls(), 0, "moving the window must not rebuild the seal");
        assert_eq!(h.state().views[0].stamp_place.as_ref().unwrap().preview_gen, generation);
        assert_eq!(h.state().views[0].stamp_place.as_ref().unwrap().pixels, pixels);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::SealEffects)));
        let moved = h.get_by_label("印影効果").rect();
        assert!(moved.center().distance(title.center()) > 20.0, "the title bar drag should move the window, before {title:?} after {moved:?}");

        crate::seal_effect::testing::reset();
        let doc = h.state().views[0].id;
        h.state_mut().seal_approval = Some(SealApproval { doc, page: 0, rect: [0.0; 4], opacity: 1.0, preview_gen: 0, sha256: [0; 32] });
        let fade = h.get_by_role_and_label(egui::accesskit::Role::Slider, "かすれ").rect();
        let start = fade.center();
        let end = start + egui::vec2(48.0, 0.0);
        h.drag_at(start);
        h.run_steps(1);
        h.hover_at(end);
        h.run_steps(3);
        let during = crate::seal_effect::testing::calls();
        assert!(during <= 1, "a slider drag rebuilt the seal {during} times before release");
        h.drop_at(end);
        h.run_steps(3);
        let after = crate::seal_effect::testing::calls();
        assert!(after <= during + 1, "release rebuilt more than once: during {during} after {after}");
        assert!(after >= 1, "releasing the slider should rebuild once");
        assert!(h.state().seal_approval.is_none(), "changing wear drops the approval");

        let source = h.state().views[0].stamp_place.as_ref().unwrap().seal.as_ref().unwrap().source.clone();
        let shown = h.state().views[0].stamp_place.as_ref().unwrap().pixels.clone();
        h.get_by_label("別パターンを生成").click();
        h.run_steps(2);
        assert_eq!(h.state().views[0].stamp_place.as_ref().unwrap().seal.as_ref().unwrap().source, source);
        assert_ne!(h.state().views[0].stamp_place.as_ref().unwrap().pixels, shown);
        h.get_by_label("加工前に戻す").click();
        h.run_steps(2);
        let reverted = h.state().views[0].stamp_place.as_ref().unwrap().seal.as_ref().unwrap().effects;
        assert!(reverted.manual);
        assert_eq!(reverted.manual_deg, 0.0);
        assert_eq!(reverted.fade, 0.0);
        assert!(reverted.is_passthrough());
        assert!(h.state().seal_approval.is_none());
    }

    #[test]
    fn the_preview_rebuilds_only_when_the_settings_or_seed_change() {
        let mut place = chairman(crate::seal_store::SealEffects::enabled_preset(), 5, 0);
        let generation = place.preview_gen;
        let pixels = place.pixels.clone();
        refresh_seal_preview(&mut place);
        assert_eq!(place.preview_gen, generation);
        assert_eq!(place.pixels, pixels);
        let same = place.seal.as_ref().unwrap().effects;
        assert!(set_place_effects(&mut place, same).is_none());
        assert_eq!(place.preview_gen, generation, "the same settings and seed must not rebuild");
        let mut ignored = same;
        ignored.bleed = 30.0;
        assert!(set_place_effects(&mut place, ignored).is_none());
        assert_eq!(place.preview_gen, generation, "a stored spread must not rebuild the preview");
        assert_eq!(place.pixels, pixels);
        let mut next = same;
        next.fade = 30.0;
        assert!(set_place_effects(&mut place, next).is_none());
        assert_ne!(place.preview_gen, generation);
        let rebuilt = place.pixels.clone();
        assert_ne!(rebuilt, pixels);
        let generation = place.preview_gen;
        place.seal.as_mut().unwrap().seed = 99;
        refresh_seal_preview(&mut place);
        assert_ne!(place.preview_gen, generation);
        assert_ne!(place.pixels, rebuilt);
        let again = place.pixels.clone();
        let generation = place.preview_gen;
        refresh_seal_preview(&mut place);
        assert_eq!(place.preview_gen, generation);
        assert_eq!(place.pixels, again);
    }

    #[test]
    fn a_slider_drag_does_not_commit_until_it_stops() {
        assert_eq!(control_commits(true, true, false, false), (true, false));
        assert_eq!(control_commits(false, false, true, false), (false, true));
        assert_eq!(control_commits(true, false, false, false), (true, true));
        assert_eq!(control_commits(true, false, false, true), (true, false));
        assert_eq!(control_commits(false, false, false, false), (false, false));
    }

    #[test]
    fn a_covering_dialog_refuses_the_stamp_commit() {
        let mut app = crate::PdfCraftApp::new();
        let bytes = app.session.create_blank(200.0, 300.0, 1).unwrap();
        app.open_bytes("opened.pdf", None, bytes.to_vec()).unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        app.views[0].stamp_place = Some(chairman(crate::seal_store::SealEffects::off(), 5, 0));
        approve_current_seal(&mut app).unwrap();
        assert!(app.seal_approval.is_some());
        app.dialog = Some(crate::Dialog::SealEffects);
        app.ask_stamp_confirm();
        app.commit_stamp_place();
        assert_eq!(app.dialog, Some(crate::Dialog::SealEffects));
        assert!(app.views[0].stamp_place.is_some());
        assert!(!app.seals.running() && uses(&app).is_empty());
        assert!(app.seal_approval.is_some(), "a covered commit must leave the approval in place");
        assert!(app.toast.is_none());
    }

    #[test]
    fn enter_does_not_skip_seal_approval_and_escape_clears_it() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
            let mut app = crate::PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            let bytes = app.session.create_blank(200.0, 300.0, 1).unwrap();
            app.open_bytes("opened.pdf", None, bytes.to_vec()).unwrap();
            app.set_view_only(true);
            app.grant_stamp();
            app
        });
        h.run_steps(3);
        h.state_mut().views[0].stamp_place = Some(chairman(crate::seal_store::SealEffects::off(), 3, 0));
        h.state_mut().ask_stamp_confirm();
        h.run_steps(3);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::ConfirmSeal)));
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(matches!(h.state().dialog, Some(crate::Dialog::ConfirmSeal)), "enter must not confirm");
        assert!(h.state().seal_approval.is_none());
        assert!(h.state().views[0].stamp_place.is_some());
        assert!(!h.state().seals.running() && uses(h.state()).is_empty());
        h.get_by_label("この文書への押印が承認されていることを確認しました").click();
        h.run_steps(2);
        assert!(h.state().seal_approval.is_some());
        let id = h.state().views[0].id;
        h.state_mut().unlock_prompt = Some(crate::UnlockPrompt { doc: id, name: "opened.pdf".into(), input: String::new(), error: None, then: None });
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(h.state().views[0].stamp_place.is_some(), "enter on the front prompt must not stamp");
        assert!(!h.state().seals.running() && uses(h.state()).is_empty());
        assert!(matches!(h.state().dialog, Some(crate::Dialog::ConfirmSeal)));
        h.state_mut().unlock_prompt = None;
        h.run_steps(2);
        assert!(h.state().seal_approval.is_some());
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.state().dialog.is_none(), "escape cancels the confirmation");
        assert!(h.state().seal_approval.is_none(), "escape drops the approval");
        assert!(h.state().views[0].stamp_place.is_some());
        assert!(!h.state().seals.running() && uses(h.state()).is_empty());
    }

    #[test]
    fn save_shortcuts_wait_until_the_dialog_closes() {
        use egui_kittest::Harness;
        let dir = std::env::temp_dir().join(format!("pdfcraft-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("copy.pdf");
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
            let mut app = crate::PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            let bytes = app.session.create_blank(200.0, 300.0, 1).unwrap();
            app.open_bytes("opened.pdf", None, bytes.to_vec()).unwrap();
            app.set_view_only(true);
            app.grant_save_as();
            app
        });
        h.run_steps(3);
        h.state_mut().dialog = Some(crate::Dialog::Preferences);
        h.state_mut().toast = None;
        h.state_mut().save_override = Some(dest.display().to_string());
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
        h.run_steps(3);
        assert_eq!(h.state().dialog, Some(crate::Dialog::Preferences));
        assert!(h.state().toast.is_none(), "ctrl+s must not reach the document: {:?}", h.state().toast);
        assert!(h.state().pending_save_copy.is_none());
        h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S);
        h.run_steps(3);
        assert_eq!(h.state().dialog, Some(crate::Dialog::Preferences), "ctrl+shift+s must not open save as");
        assert!(h.state().pending_save_copy.is_none());
        assert!(!dest.exists());
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.state().dialog.is_none());
        h.state_mut().toast = None;
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
        h.run_steps(3);
        assert!(h.state().dialog.is_none());
        assert_eq!(h.state().toast.as_ref().map(|(text, _)| text.as_str()), Some(crate::command_policy::REFUSED));
        assert!(!dest.exists());
        h.state_mut().toast = None;
        h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S);
        h.run_steps(3);
        assert_eq!(h.state().dialog, Some(crate::Dialog::SaveCopy));
        assert!(!dest.exists(), "save as opens a confirmation and does not write yet");
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.state().dialog.is_none(), "escape cancels the save confirmation");
        assert!(h.state().pending_save_copy.is_none());
        assert!(!dest.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
