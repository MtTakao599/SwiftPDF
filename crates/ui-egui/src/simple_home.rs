//! SwiftPDF's simple interface: its Home (the eight things people come to do with a PDF, of
//! which this view-only version opens files and says the rest are coming, and the recent files,
//! including ones that moved or were deleted) and the bar over a document.

use std::collections::HashMap;

use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, Sense, Stroke, vec2};

use crate::theme::{self, Tokens};
use crate::{PdfCraftApp, RECENT_MAX, icons, widgets};

/// The Home buttons: label and icon. Only 開く does something; the rest run nothing at all.
pub(crate) const ACTIONS: [(&str, &str); 8] = [
    ("開く", "folder-open"),
    ("結合", "files"),
    ("分割", "scissors"),
    ("編集", "pencil"),
    ("変換", "arrow-left-right"),
    ("圧縮", "minimize-2"),
    ("印刷", "printer"),
    ("スタンプ", "stamp"),
];

/// What a recent entry says when its file is not where it was.
pub(crate) const MISSING: &str = "ファイルが見つかりません";
pub(crate) const RELOCATE: &str = "場所の再指定";
pub(crate) const REMOVE: &str = "一覧から削除";

/// The widest the Home content grows, centred in wider windows.
const MAX_WIDTH: f32 = 920.0;
const SIDE: f32 = 24.0;
const GAP: f32 = 14.0;
const CARD_HEIGHT: f32 = 104.0;
/// Below this content width the buttons go two to a row instead of four.
const FOUR_ACROSS: f32 = 4.0 * 120.0 + 3.0 * GAP;
/// How long a recent file's presence on disk is trusted before it is looked up again, so Home
/// doesn't ask the disk (or a slow network share) for every file on every frame.
const PRESENCE_SECS: f64 = 1.0;
/// Amber for a file that can't be found: a warning, not an error.
const WARNING: Color32 = Color32::from_rgb(0xC2, 0x7A, 0x00);

/// What a click on the recent list asks for, done once the list is no longer borrowed.
enum RecentAction {
    Open(String),
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Relocate(String),
    Remove(String),
}

pub fn show(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().id_salt("simple_home").auto_shrink([false, false]).show(ui, |ui| {
        let width = (ui.available_width() - 2.0 * SIDE).clamp(1.0, MAX_WIDTH);
        let side = ((ui.available_width() - width) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(side);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.add_space(28.0);
                ui.label(egui::RichText::new("SwiftPDF").font(theme::semibold(24.0)));
                ui.label(egui::RichText::new("PDFを開いて、読む・探す・確かめることができます。").color(t.text_muted).font(theme::regular(14.0)));
                ui.add_space(18.0);
                actions(app, ui, &t, width);
                ui.add_space(30.0);
                recent(app, ui, &t, width);
                ui.add_space(24.0);
            });
        });
    });
}

fn actions(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens, width: f32) {
    let across = if width >= FOUR_ACROSS { 4 } else { 2 };
    let card_width = ((width - GAP * (across - 1) as f32) / across as f32).max(1.0);
    for row in ACTIONS.chunks(across) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for &(label, icon) in row {
                let ready = label == "開く";
                if action_card(ui, t, vec2(card_width, CARD_HEIGHT), label, icon, ready).clicked() {
                    if ready {
                        app.execute("file.open");
                    } else {
                        app.notify(format!("「{label}」は準備中です。"));
                    }
                }
            }
        });
        ui.add_space(GAP - ui.spacing().item_spacing.y);
    }
}

/// One Home button: its icon over its label, with a 準備中 badge on what isn't ready.
fn action_card(ui: &mut egui::Ui, t: &Tokens, size: egui::Vec2, label: &str, icon: &str, ready: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let press = theme::Press::track(ui, &response);
    let fill = press.fill(if ready { t.card } else { t.panel }, t.hover, t.pressed);
    let stroke = Stroke::new(1.0, if ready { t.accent } else { t.border });
    ui.painter().rect(rect, CornerRadius::same(12), fill, stroke, egui::StrokeKind::Inside);
    let body = rect.translate(press.offset());
    let (icon_tint, text) = if ready { (t.accent, t.text) } else { (t.icon, t.text_muted) };
    icons::paint(ui, Rect::from_center_size(body.center() - vec2(0.0, 14.0), vec2(30.0, 30.0)), icon, 28.0, icon_tint);
    ui.painter().text(body.center() + vec2(0.0, 24.0), Align2::CENTER_CENTER, label, theme::semibold(14.0), text);
    if !ready {
        ui.painter().text(body.right_top() + vec2(-10.0, 12.0), Align2::RIGHT_CENTER, "準備中", theme::regular(11.0), t.text_faint);
    }
    let response = if ready { response } else { response.on_hover_text("この機能は準備中です") };
    theme::hand(response)
}

fn recent(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens, width: f32) {
    ui.label(egui::RichText::new("最近使ったPDF").font(theme::semibold(17.0)));
    ui.add_space(8.0);
    if app.recent.is_empty() {
        ui.label(egui::RichText::new("最近開いたPDFはまだありません。「開く」から選んだPDFがここに表示されます。").color(t.text_muted));
        return;
    }
    let mut presence = Presence::load(ui);
    let mut action = None;
    for r in app.recent.iter().take(RECENT_MAX) {
        let found = presence.check(&r.path);
        if let Some(a) = recent_row(ui, t, width, &r.name, &r.path, found) {
            action = Some(a);
        }
    }
    presence.store(ui, app.recent.iter().map(|r| r.path.as_str()));
    match action {
        Some(RecentAction::Open(path)) => app.open_recent(&path),
        #[cfg(not(target_arch = "wasm32"))]
        Some(RecentAction::Relocate(path)) => app.relocate_recent(path),
        #[cfg(target_arch = "wasm32")]
        Some(RecentAction::Relocate(_)) => {}
        Some(RecentAction::Remove(path)) => app.recent.retain(|r| r.path != path),
        None => {}
    }
}

/// One recent file: its name and path, and what's wrong if it can't be found. Clicking a file
/// that is there opens it; one that isn't offers to find it again.
fn recent_row(ui: &mut egui::Ui, t: &Tokens, width: f32, name: &str, path: &str, found: bool) -> Option<RecentAction> {
    let height = if found { 56.0 } else { 74.0 };
    let (rect, row) = ui.allocate_exact_size(vec2(width, height), if found { Sense::click() } else { Sense::hover() });
    row.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, found, name));
    let press = theme::Press::track(ui, &row);
    if found {
        press.wash(ui, rect, 8, false);
    }
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, t.divider));
    let mut action = None;
    // The buttons first, right to left, so the text knows where to stop.
    let mut buttons = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(8.0, 0.0))).layout(Layout::right_to_left(Align::Center)));
    if widgets::pill_button(&mut buttons, REMOVE, false).on_hover_text("ファイルはそのままで、一覧からだけ外します").clicked() {
        action = Some(RecentAction::Remove(path.to_owned()));
    }
    #[cfg(not(target_arch = "wasm32"))]
    if !found && widgets::pill_button(&mut buttons, RELOCATE, true).on_hover_text("移動したPDFを選び直します").clicked() {
        action = Some(RecentAction::Relocate(path.to_owned()));
    }
    let text_right = buttons.min_rect().left() - 12.0;
    let left = rect.left() + 48.0;
    let text_width = (text_right - left).max(1.0);
    let (icon, tint) = if found { ("file-text", Color32::from_rgb(0xE0, 0x3E, 0x3E)) } else { ("triangle-alert", WARNING) };
    icons::paint(ui, Rect::from_min_size(rect.left_top() + vec2(12.0, 14.0), vec2(24.0, 24.0)), icon, 22.0, tint);
    let name_color = if found { t.text } else { t.text_muted };
    // The name is the row's own label; the lines under it are labels so screen readers get them.
    widgets::row_text(ui, egui::pos2(left, rect.top() + 17.0), crate::bidi::visual(name), theme::medium(14.0), name_color, text_width);
    let below = Rect::from_min_size(egui::pos2(left, rect.top() + 29.0), vec2(text_width, height - 31.0));
    let mut lines = ui.new_child(egui::UiBuilder::new().max_rect(below).layout(Layout::top_down(Align::Min)));
    lines.spacing_mut().item_spacing.y = 3.0;
    let line = |ui: &mut egui::Ui, text: &str, font: egui::FontId, color: Color32| {
        ui.add(egui::Label::new(egui::RichText::new(crate::bidi::visual(text)).font(font).color(color)).truncate().selectable(false));
    };
    line(&mut lines, path, theme::regular(11.5), t.text_faint);
    if !found {
        line(&mut lines, MISSING, theme::medium(12.0), WARNING);
    }
    if found && theme::hand(row).on_hover_text(path).clicked() && action.is_none() {
        action = Some(RecentAction::Open(path.to_owned()));
    }
    action
}

/// Whether recent files are on disk, remembered for [`PRESENCE_SECS`] in egui's memory.
struct Presence {
    now: f64,
    seen: HashMap<String, (bool, f64)>,
}

impl Presence {
    fn id() -> egui::Id {
        egui::Id::new("simple-home-presence")
    }

    fn load(ui: &egui::Ui) -> Self {
        let now = ui.input(|i| i.time);
        let seen = ui.data(|d| d.get_temp::<HashMap<String, (bool, f64)>>(Self::id())).unwrap_or_default();
        Self { now, seen }
    }

    fn check(&mut self, path: &str) -> bool {
        match self.seen.get(path) {
            Some(&(found, at)) if (0.0..PRESENCE_SECS).contains(&(self.now - at)) => found,
            _ => {
                let found = on_disk(path);
                self.seen.insert(path.to_owned(), (found, self.now));
                found
            }
        }
    }

    /// Keep what is known about the files still listed.
    fn store<'a>(mut self, ui: &egui::Ui, listed: impl Iterator<Item = &'a str>) {
        let listed: Vec<&str> = listed.collect();
        self.seen.retain(|path, _| listed.contains(&path.as_str()));
        ui.data_mut(|d| d.insert_temp(Self::id(), self.seen));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_disk(path: &str) -> bool {
    std::path::Path::new(path).is_file()
}

/// A browser's recent files are the ones it was given, not paths to look up.
#[cfg(target_arch = "wasm32")]
fn on_disk(_path: &str) -> bool {
    true
}

/// The simple interface's bar over a document: finding text and the document's properties (the
/// rail beside the page has the pages, bookmarks, zoom and turning).
pub fn viewer_bar(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    if app.active.is_none() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("simple_bar")
        .exact_size(40.0)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 0)).stroke(Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::ghost_button(ui, "info", "文書のプロパティ").clicked() {
                    app.execute("file.properties");
                }
                let tip = crate::commands::command_tip(ui.ctx(), "文書内を検索 ({key})", "edit.find");
                if widgets::ghost_button(ui, "search", "検索").on_hover_text(tip).clicked() {
                    app.execute("edit.find");
                }
            });
        });
}

#[cfg(not(target_arch = "wasm32"))]
impl PdfCraftApp {
    /// Home ▸ 場所の再指定: choose where a recent file that went missing is now. Its entry is
    /// replaced only once the chosen file has opened.
    pub(crate) fn relocate_recent(&mut self, old: String) {
        let dialog = rfd::AsyncFileDialog::new().add_filter("PDF", &["pdf"]);
        self.ask_one(crate::pickers::Ask::File(dialog), None, move |app, path| app.relocate_recent_to(old, &path.to_string_lossy()));
    }

    fn relocate_recent_to(&mut self, old: String, new: &str) {
        // Already open: it is that file, which comes to the front.
        if self.tab_showing(new).is_some() {
            self.recent.retain(|r| r.path != old);
            self.open_recent(new);
            return;
        }
        self.pending_relocation = Some((old, new.to_owned()));
        self.open_path(new);
    }
}
