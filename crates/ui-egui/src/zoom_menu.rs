//! The zoom control on the right rail. The simple and detailed interfaces share the rail, so they
//! share this menu: presets, a number you type, and the existing fit commands.

use egui::{Align2, Sense, vec2};

use crate::canvas::{DocView, Fit};
use crate::theme::{self, Tokens};

/// Presets, in percent. Choosing 100 applies exactly 1.0.
pub const PRESETS: [u16; 6] = [50, 75, 100, 125, 150, 200];

/// Said when the typed zoom cannot be read. Kept as a plain sentence so it does not have to be
/// added to every translation catalogue; unknown sentences already stay in English.
pub const INVALID: &str = "Enter a zoom from 8% to 6400%.";

/// A percent as a zoom factor. 100 is exactly 1.0.
pub fn zoom_from_percent(percent: f32) -> f32 {
    if (percent - 100.0).abs() < 1.0e-3 { 1.0 } else { percent / 100.0 }
}

/// A typed zoom: `150`, `150%` or `150.5`. Empty, zero, negative and non-numeric text is `None`.
/// A number past the rail's range is still `Some`; [`DocView::set_zoom`] clamps it to 8%..=6400%.
pub fn parse_percent(text: &str) -> Option<f32> {
    let text = text.trim();
    let text = text.strip_suffix('%').unwrap_or(text).trim();
    if text.is_empty() {
        return None;
    }
    let n = text.parse::<f32>().ok()?;
    if !n.is_finite() || n <= 0.0 { None } else { Some(n) }
}

/// The percent under the zoom buttons, and the menu it opens. A typed zoom that cannot be read
/// comes back as a message for the caller to show; the view is left as it was.
pub(crate) fn button(ui: &mut egui::Ui, view: &mut DocView) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let shown = format!("{:.0}%", view.zoom * 100.0);
    let name = format!("Zoom {shown}");
    let (rect, resp) = ui.allocate_exact_size(vec2(40.0, 18.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    let press = theme::Press::track(ui, &resp);
    press.wash(ui, rect, 4, false);
    ui.painter().text(rect.center() + press.offset(), Align2::CENTER_CENTER, &shown, theme::medium(11.0), t.text_muted);
    let resp = theme::hand(resp).on_hover_text(tl!("Zoom"));
    let id = egui::Id::new(("zoom-entry", view.id));
    if resp.clicked() {
        // Empty, so what is typed is the whole number. The button above already shows the current one.
        ui.data_mut(|d| d.insert_temp(id, String::new()));
    }
    let typed = egui::Popup::menu(&resp).show(|ui| menu(ui, view, id));
    typed.and_then(|r| r.inner)
}

fn menu(ui: &mut egui::Ui, view: &mut DocView, entry_id: egui::Id) -> Option<String> {
    ui.set_min_width(168.0);
    for percent in PRESETS {
        let on = view.fit == Fit::None && (view.zoom * 100.0 - f32::from(percent)).abs() < 0.5;
        if ui.selectable_label(on, format!("{percent}%")).clicked() {
            view.set_zoom(if percent == 100 { 1.0 } else { zoom_from_percent(f32::from(percent)) });
            ui.close();
        }
    }
    ui.separator();
    let typed = entry(ui, view, entry_id);
    ui.separator();
    if ui.button(tl!("Fit to width")).clicked() {
        view.set_fit(Fit::Width);
        ui.close();
    }
    if ui.button(tl!("Zoom to page level")).clicked() {
        view.set_fit(Fit::Page);
        ui.close();
    }
    typed
}

/// The number box. Enter applies it to this tab only.
fn entry(ui: &mut egui::Ui, view: &mut DocView, id: egui::Id) -> Option<String> {
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
    let hint = format!("{:.0}", view.zoom * 100.0);
    let submitted = ui
        .horizontal(|ui| {
            let label = ui.label(tl!("Zoom"));
            let resp = ui.add(egui::TextEdit::singleline(&mut text).hint_text(&hint).desired_width(72.0).char_limit(8)).labelled_by(label.id);
            if text.is_empty() && !resp.has_focus() {
                resp.request_focus();
            }
            ui.input(|i| i.key_pressed(egui::Key::Enter)) && (resp.lost_focus() || resp.has_focus())
        })
        .inner;
    ui.data_mut(|d| d.insert_temp(id, text.clone()));
    if !submitted {
        return None;
    }
    match parse_percent(&text) {
        Some(percent) => {
            view.set_zoom(zoom_from_percent(percent));
            ui.close();
            None
        }
        None => {
            ui.close();
            Some(INVALID.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hundred_percent_is_exactly_one_and_junk_is_refused() {
        assert_eq!(zoom_from_percent(100.0), 1.0);
        assert_eq!(zoom_from_percent(100.0004), 1.0);
        assert_eq!(zoom_from_percent(50.0), 0.5);
        assert_eq!(zoom_from_percent(200.0), 2.0);
        assert_eq!(parse_percent("  150% "), Some(150.0));
        assert_eq!(parse_percent("100"), Some(100.0));
        assert_eq!(parse_percent("12.5"), Some(12.5));
        for bad in ["", "   ", "%", "abc", "0", "0.0", "-20", "inf", "nan", "∞"] {
            assert_eq!(parse_percent(bad), None, "{bad:?}");
        }
        assert_eq!(parse_percent("100%"), Some(100.0));
        assert_eq!(parse_percent("100%%"), None);
    }
}
