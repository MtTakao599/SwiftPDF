//! Shared font window. Callers receive a catalogue id. This module does not edit text or a stamp;
//! a date-stamp caller applies that id to one row after the window closes.

use std::sync::Mutex;

use egui::RichText;

use crate::theme::{self, Tokens};
use crate::widgets;
use pdfcraft_fonts::catalog::{self, Face, SampleImage};

const CACHE_CAP: usize = 48;

#[derive(Clone, Debug)]
pub struct State {
    pub query: String,
    pub sample: String,
    pub selected: Option<String>,
    /// Focus the search field once when the window opens.
    focus_search: bool,
    /// Which faces this opening may list. The default lists every available face.
    offer: catalog::Offer,
}

const NO_MATCH: &str = "該当するフォントがありません。";

impl Default for State {
    fn default() -> Self {
        Self {
            query: String::new(),
            sample: catalog::DEFAULT_SAMPLE.to_string(),
            selected: None,
            focus_search: true,
            offer: catalog::Offer::default(),
        }
    }
}

impl State {
    /// A fresh window, with `current` highlighted and the search field ready for typing.
    pub(crate) fn opening(current: Option<&str>) -> Self {
        Self::opening_for(current, catalog::Offer::default())
    }

    /// Same window, limited to the faces `offer` allows. A non-empty `offer.text` is the first sample.
    pub(crate) fn opening_for(current: Option<&str>, offer: catalog::Offer) -> Self {
        let sample = if offer.text.trim().is_empty() { catalog::DEFAULT_SAMPLE.to_string() } else { offer.text.clone() };
        Self { selected: current.map(str::to_string), sample, offer, ..Self::default() }
    }
}

struct Cache {
    entries: Vec<(String, SampleImage)>,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: Mutex<Cache> = Mutex::new(Cache { entries: Vec::new() });
    &CACHE
}

fn sample_image(id: &str, text: &str, px: f32) -> Option<SampleImage> {
    let key = format!("{id}\u{1f}{text}\u{1f}{px:.1}");
    if let Ok(guard) = cache().lock()
        && let Some((_, image)) = guard.entries.iter().find(|(k, _)| k == &key)
    {
        return Some(image.clone());
    }
    let image = catalog::render_sample(id, text, px).ok()?;
    if let Ok(mut guard) = cache().lock() {
        if let Some(index) = guard.entries.iter().position(|(k, _)| k == &key) {
            guard.entries.remove(index);
        }
        guard.entries.insert(0, (key, image.clone()));
        if guard.entries.len() > CACHE_CAP {
            guard.entries.truncate(CACHE_CAP);
        }
    }
    Some(image)
}

/// `true` when the user confirmed or cancelled.
pub(crate) fn body(ui: &mut egui::Ui, state: &mut State, choice: &mut Option<String>, t: &Tokens) -> bool {
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        *choice = None;
        return true;
    }
    ui.label(RichText::new("フォントを選択").font(theme::semibold(18.0)));
    ui.add_space(4.0);
    let lead = if state.offer.raster {
        "決定すると、開いた行の書体だけを変えます。文字入力には適用しません。"
    } else if state.offer.standard {
        "決定すると文字入力の書体を変えます。標準14は実書体プレビューがありません。"
    } else {
        "決定するとフォントIDを返します。文字入力には、まだ適用されません。"
    };
    ui.label(RichText::new(lead).small().color(t.text_muted));
    ui.add_space(6.0);
    egui::Grid::new("font-picker-fields").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        let search = ui.label("検索");
        let search_edit = ui
            .add(widgets::line(&mut state.query).id_source("font-picker-search").desired_width(360.0).hint_text("名前、ファミリー、スタイル"))
            .labelled_by(search.id);
        if state.focus_search {
            search_edit.request_focus();
            state.focus_search = false;
        }
        ui.end_row();
        let sample = ui.label("サンプル");
        ui.add(widgets::line(&mut state.sample).id_source("font-picker-sample").desired_width(360.0)).labelled_by(sample.id);
        ui.end_row();
    });
    ui.add_space(6.0);
    if state.offer.raster {
        let sentence = if state.offer.text.trim().is_empty() {
            "印影に描ける書体だけを表示しています。"
        } else {
            "印影に描ける書体のうち、この行の文字を収録しているものだけを表示しています。"
        };
        ui.label(RichText::new(sentence).small().color(t.text_muted));
        ui.add_space(4.0);
    }
    let px = (20.0 * ui.ctx().pixels_per_point()).clamp(12.0, 40.0);
    let shown: Vec<&Face> =
        catalog::faces().iter().filter(|face| catalog::face_offered(face, &state.offer) && catalog::matches_query(face, &state.query)).collect();
    if shown.is_empty() {
        ui.label(RichText::new(NO_MATCH).color(t.text_muted));
    }
    egui::ScrollArea::vertical().max_height(360.0).id_salt("font-picker-list").show(ui, |ui| {
        for face in shown {
            let on = state.selected.as_deref() == Some(face.id);
            let frame = egui::Frame::new()
                .fill(if on { t.accent.gamma_multiply(0.16) } else { egui::Color32::TRANSPARENT })
                .corner_radius(6)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    if ui.selectable_label(on, face.display_name).clicked() {
                        state.selected = Some(face.id.to_string());
                    }
                    ui.label(RichText::new(format!("{}  {}", face.family, face.style)).small().color(t.text_muted));
                    paint_sample(ui, face, &state.sample, px);
                });
            if frame.response.clicked() {
                state.selected = Some(face.id.to_string());
            }
            ui.add_space(4.0);
        }
    });
    ui.add_space(6.0);
    if let Some(id) = state.selected.as_deref()
        && let Some(face) = catalog::get(id)
    {
        if catalog::face_offered(face, &state.offer) {
            ui.label(RichText::new(format!("選択中: {}", face.display_name)).strong());
            ui.label(format!("ID: {}", face.id));
            ui.label(format!("PDF: {}", pdf_label(face, &state.offer)));
            ui.label(if face.japanese_sample() {
                "日本語: サンプルの文字を収録しています。"
            } else {
                "日本語: この書体にはサンプルの日本語がありません。"
            });
        } else if face.available() {
            ui.label("この行の文字を収録していないため、この書体は選べません。");
        } else {
            ui.label("指定フォントが利用できない");
        }
    }
    let chosen =
        state.selected.as_deref().and_then(catalog::get).filter(|face| catalog::face_offered(face, &state.offer)).map(|face| face.id.to_string());
    let mut close = false;
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.add_enabled_ui(chosen.is_some(), |ui| widgets::pill_button(ui, "決定", true)).inner.clicked()
            && let Some(id) = chosen.clone()
        {
            *choice = Some(id);
            close = true;
        }
        if widgets::pill_button(ui, "キャンセル", false).clicked() {
            *choice = None;
            close = true;
        }
    });
    close
}

fn pdf_label(face: &Face, offer: &catalog::Offer) -> &'static str {
    match face.pdf {
        catalog::PdfMethod::Standard14Name(_) => "標準14のフォント名（プログラムは埋め込まない）",
        catalog::PdfMethod::Type3Outlines => "使用した文字の Type 3 アウトライン",
        catalog::PdfMethod::EmbedProgram if offer.standard => "使用した文字の Type 3 アウトライン",
        catalog::PdfMethod::EmbedProgram => "フォントプログラムを埋め込む",
    }
}

fn paint_sample(ui: &mut egui::Ui, face: &Face, text: &str, px: f32) {
    if !face.can_preview() {
        ui.label("実書体プレビューなし。標準14の名前として保存し、日本語は従来の代替字形を使います。");
        return;
    }
    let Some(image) = sample_image(face.id, text, px) else {
        ui.label("この書体のサンプルは描画できませんでした。");
        return;
    };
    if !face.japanese_sample() {
        ui.label("日本語非対応。収録されている文字だけを、この書体で表示しています。");
    } else if !image.skipped.is_empty() {
        ui.label(format!("この書体に無い文字は抜いています: {}", image.skipped));
    }
    if image.width == 0 || image.rgba.len() != (image.width as usize).saturating_mul(image.height as usize).saturating_mul(4) {
        return;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([image.width as usize, image.height as usize], &image.rgba);
    let texture = ui.ctx().load_texture(format!("font-sample-{}", face.id), color, egui::TextureOptions::LINEAR);
    let shown = (image.width as f32 / ui.ctx().pixels_per_point()).min(520.0);
    let scale = shown / (image.width as f32).max(1.0);
    ui.image((texture.id(), egui::vec2(image.width as f32 * scale, image.height as f32 * scale)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn harness(simple: bool) -> Harness<'static, crate::PdfCraftApp> {
        let mut h = Harness::builder().with_size(egui::vec2(1100.0, 800.0)).build_eframe(|_cc| {
            let mut app = crate::PdfCraftApp::new();
            if simple {
                app.set_ui_mode(crate::UiMode::Simple);
            }
            app
        });
        h.run_steps(2);
        h
    }

    fn open(h: &mut Harness<'static, crate::PdfCraftApp>) {
        assert!(h.state_mut().open_font_picker(Some("standard:helvetica:regular")));
        h.run_steps(3);
    }

    fn type_search(h: &mut Harness<'static, crate::PdfCraftApp>, text: &str) {
        let field = h.get_by_role_and_label(egui::accesskit::Role::TextInput, "検索");
        field.click();
        field.focus();
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::TextInput, "検索").type_text(text);
        h.run_steps(3);
        assert_eq!(h.state().font_picker.query, text, "the search field keeps what was typed");
    }

    fn offered(id: &str) -> bool {
        catalog::get(id).is_some_and(|face| face.available())
    }

    #[test]
    fn choosing_a_face_returns_its_id_and_cancel_returns_nothing() {
        let mut h = harness(false);
        open(&mut h);
        assert!(h.state().dialog.is_some());
        let (label, id) = if offered("craft:shippori-mincho:regular") {
            ("しっぽり明朝 標準", "craft:shippori-mincho:regular")
        } else {
            ("Helvetica 標準", "standard:helvetica:regular")
        };
        h.get_by_label(label).click();
        h.run_steps(2);
        h.get_by_label("決定").click();
        h.run_steps(3);
        assert!(h.state().dialog.is_none());
        assert_eq!(h.state_mut().take_font_choice().as_deref(), Some(id));
        assert!(h.state_mut().take_font_choice().is_none());

        open(&mut h);
        h.get_by_label("Helvetica 太字").click();
        h.run_steps(1);
        h.get_by_label("キャンセル").click();
        h.run_steps(2);
        assert!(h.state().dialog.is_none());
        assert!(h.state_mut().take_font_choice().is_none());
    }

    #[test]
    fn escape_and_an_empty_search_leave_the_choice_unset() {
        let mut h = harness(true);
        assert_eq!(h.state().ui_mode(), crate::UiMode::Simple);
        open(&mut h);
        type_search(&mut h, "明朝");
        if offered("craft:shippori-mincho:regular") {
            assert!(h.query_by_label("しっぽり明朝 標準").is_some());
            assert!(h.query_by_label("Helvetica 標準").is_none(), "a Japanese query hides Latin faces");
        } else {
            assert!(h.query_by_label(NO_MATCH).is_some());
        }
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().dialog.is_none(), "Escape closes the window");
        assert!(h.state_mut().take_font_choice().is_none());

        open(&mut h);
        type_search(&mut h, "zzz-no-face");
        assert!(h.query_by_label(NO_MATCH).is_some());
        assert!(h.query_by_label("Helvetica 標準").is_none());
        h.get_by_label("キャンセル").click();
        h.run_steps(2);
        assert!(h.state_mut().take_font_choice().is_none());
    }

    #[test]
    fn a_second_open_does_not_keep_the_previous_draft() {
        let mut h = harness(false);
        open(&mut h);
        type_search(&mut h, "courier");
        assert!(h.query_by_label("Courier 標準").is_some());
        assert!(h.query_by_label("Helvetica 標準").is_none());
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        open(&mut h);
        let query = h.state().font_picker.query.clone();
        assert!(query.is_empty(), "reopening cleared the search, left {query:?}");
        assert_eq!(h.state().font_picker.sample, catalog::DEFAULT_SAMPLE);
    }
}
