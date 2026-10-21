//! Edit a PDF ▸ Add content: the Text tool (click on the page and type), Image (pick a file;
//! it lands in the middle of the page), and editing what was added: select, move, resize
//! (images keep their proportions), double-click text to retype it, format it from the panel,
//! Delete to remove it. Each change is one undoable engine edit.

use egui::{Color32, CornerRadius, Pos2, Rect, Stroke};
use pdfcraft_engine::{Added, AddedContent, AddedText, Edit, FontFamily, TextAlign};
use pdfcraft_render::DocInfo;

use crate::canvas::{DocView, PageXform};
use crate::theme::Tokens;
use crate::widgets;

/// The colour of the warning under text the standard fonts can't draw.
pub(crate) const UNDRAWABLE: Color32 = Color32::from_rgb(0xD7, 0x37, 0x3F);

/// The first character of a draft that can't be drawn, as the engine decides it (it refuses
/// such text, #125): outside the standard fonts and not shaped with the Arabic face.
pub(crate) fn undrawable(d: &TextDraft, memo: &UndrawableMemo) -> Option<char> {
    // Checked as `finish` will commit it: without trailing whitespace.
    let t = AddedText { text: d.text.trim_end().to_string(), rect: d.rect, ..d.style.clone() };
    if let Some((key, c)) = memo.borrow().as_ref()
        && *key == t
    {
        return *c;
    }
    let c = pdfcraft_engine::first_undrawable(&t);
    *memo.borrow_mut() = Some((t, c));
    c
}

/// The last draft [`undrawable`] checked and its answer: the check lays the text out (and shapes
/// Arabic), too slow to repeat every frame for long text, so it reruns only when the text or
/// anything that changes its layout does.
pub(crate) type UndrawableMemo = std::cell::RefCell<Option<(AddedText, Option<char>)>>;

/// Shown when a readable face has no hiragana, katakana, or kanji and the missing character is one
/// of those letters.
const NO_JAPANESE_FONT: &str = "このフォントは日本語に対応していません。日本語対応フォントを選択してください。";

/// The warning for a catalogue face that cannot draw `missing`.
///
/// A face with no Japanese letters, and a missing Japanese letter, says the face does not support
/// Japanese. A face that has any Japanese letters — even only some — names the missing character
/// instead. An unreadable face, or a missing character that is not Japanese, also names that
/// character. The missing character alone never decides the wording.
pub(crate) fn catalog_missing_message(id: &str, missing: char) -> String {
    let supports = pdfcraft_fonts::catalog::get(id).and_then(|face| face.supports_japanese());
    glyph_gap_message(supports, missing)
}

/// `supports_japanese` is [`pdfcraft_fonts::catalog::Face::supports_japanese`]. `Some(false)` is the
/// only “no Japanese” answer. `Some(true)` covers a full Japanese face and a face that has only
/// some Japanese letters. `None` is unreadable, so the missing character is named.
pub(crate) fn glyph_gap_message(supports_japanese: Option<bool>, missing: char) -> String {
    if supports_japanese == Some(false) && pdfcraft_fonts::catalog::is_japanese_letter(missing) {
        NO_JAPANESE_FONT.to_string()
    } else {
        format!("このフォントには『{missing}』の文字が収録されていません。別のフォントを選択してください。")
    }
}

/// The warning shown for text with `c` in it.
pub(crate) fn undrawable_message(c: char) -> String {
    crate::i18n::fmt(
        tl!("The PDF text fonts can't draw “{c}” ({code}). Choose supported text."),
        &[("c", &c.to_string()), ("code", &format!("U+{:04X}", u32::from(c)))],
    )
}

/// The warning after an edit that saved text its font can't draw: PdfCraft drew `?` for it (a form
/// field, which other viewers redraw from its value, or a comment, where the `?` shows everywhere).
pub(crate) fn undrawn_message(n: &pdfcraft_engine::TextNotice) -> String {
    let template = if n.redrawn_elsewhere {
        tl!(
            "This field's font can't draw “{c}” ({code}), so PdfCraft shows ? in its place. The value is saved, and other PDF viewers redraw the field from it."
        )
    } else {
        tl!("The fonts PdfCraft writes with can't draw “{c}” ({code}), so it shows as ? in every PDF viewer. The text is saved.")
    };
    crate::i18n::fmt(template, &[("c", &n.ch.to_string()), ("code", &format!("U+{:04X}", u32::from(n.ch)))])
}

const SELECT_BLUE: Color32 = Color32::from_rgb(0x14, 0x73, 0xE6);

/// The width of the frame that moves a text box while it is being typed.
const DRAFT_FRAME: f32 = 6.0;

/// The text being typed or retyped in place.
#[derive(Clone, Debug, PartialEq)]
pub struct TextDraft {
    pub page: usize,
    /// `None` for new text; the item index for retyping.
    pub index: Option<usize>,
    /// Display space.
    pub rect: [f64; 4],
    pub text: String,
    pub style: AddedText,
    pub focus: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Move,
    /// Which corner: (left, top) in screen terms.
    Corner(bool, bool),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentView {
    /// The selected added item: (page, index among the page's items).
    pub selected: Option<(usize, usize)>,
    pub draft: Option<TextDraft>,
    /// Select the item an add creates (the page's item count before it).
    pub select_added: Option<(usize, usize)>,
    grab: Option<(usize, Grab, Pos2)>,
    /// The draft being dragged by its frame: its page, where the press began, and the draft's
    /// box then. Nothing is committed until Done.
    draft_grab: Option<(usize, Pos2, [f64; 4])>,
    /// The text box's size on screen last frame, so the frame around it can take the press
    /// before the text box sees it.
    editor_size: Option<egui::Vec2>,
    undrawable: UndrawableMemo,
    layout: Option<(AddedText, f32, f64, [f64; 4])>,
}

impl ContentView {
    /// Text still being typed, for a print copy. `Err` is a character this build cannot draw;
    /// the draft is left as it is. Empty text is `Ok(None)`.
    pub(crate) fn print_pending(&self) -> Result<Option<pdfcraft_engine::PendingPrintText>, char> {
        let Some(draft) = &self.draft else { return Ok(None) };
        if draft.text.trim().is_empty() {
            return Ok(None);
        }
        if let Some(c) = undrawable(draft, &self.undrawable) {
            return Err(c);
        }
        Ok(pending_print_text(draft))
    }
}

/// The draft as text to draw on a print copy, using the same box [`finish`] would commit.
pub(crate) fn pending_print_text(draft: &TextDraft) -> Option<pdfcraft_engine::PendingPrintText> {
    let text = draft.text.trim_end().to_string();
    if text.trim().is_empty() {
        return None;
    }
    Some(pdfcraft_engine::PendingPrintText {
        page: draft.page,
        replace: draft.index,
        text: AddedText { rect: draft.rect, text, ..draft.style.clone() },
    })
}

impl ContentView {
    pub(crate) fn finish_draft(&mut self, added: &[Added]) -> Option<Edit> {
        self.draft.take().and_then(|d| finish(self, d, added))
    }
    /// The draft's first character the standard fonts can't draw. Such a draft is kept open, and
    /// counts as unsaved work, until it is corrected or discarded.
    pub(crate) fn blocked(&self) -> Option<char> {
        self.draft.as_ref().and_then(|d| undrawable(d, &self.undrawable))
    }

    /// Keeps a blocked draft open and focused instead of finishing or replacing it.
    pub(crate) fn hold_blocked(&mut self) -> bool {
        let blocked = self.blocked().is_some();
        if let Some(d) = self.draft.as_mut().filter(|_| blocked) {
            d.focus = true;
        }
        blocked
    }
}

/// While an added item is moved, the page without it, rendered once per gesture: the item's own
/// pixels follow the pointer over it instead of staying put until release (#1488).
#[derive(Default)]
pub(crate) struct MoveBackground {
    key: Option<(u64, usize, usize)>,
    renderer: Option<pdfcraft_render::RenderPool>,
    texture: Option<egui::TextureHandle>,
    tag: Option<u64>,
    /// The item's display-space box when the gesture started.
    from: Option<[f64; 4]>,
    /// The applied move's box. The page raster on screen still has the item where it was until
    /// its replacement arrives, so the preview stays up until then.
    settling: Option<[f64; 4]>,
}

impl MoveBackground {
    pub fn pending(&self) -> bool {
        self.renderer.is_some() && self.texture.is_none()
    }

    pub fn prepare(&mut self, ctx: &egui::Context, doc: &pdfcraft_engine::Document, cv: &ContentView, scale: f32) {
        let key = match (cv.grab, cv.selected) {
            (Some((gp, Grab::Move, _)), Some((sp, si))) if gp == sp => Some((doc.edit_generation(), sp, si)),
            _ => None,
        };
        if !(key.is_none() && self.settling.is_some()) && self.key != key {
            *self = Self { key, ..Default::default() };
            if let Some((_, page, index)) = key {
                self.renderer = doc.added_content_preview(page, index).ok().map(|p| p.renderer());
                self.from = on_page(doc.added(), page).into_iter().find(|(i, _)| *i == index).map(|(_, a)| a.content.rect());
            }
        }
        if self.settling.is_some() {
            return;
        }
        let (Some((_, page, _)), Some(renderer)) = (self.key, self.renderer.as_ref()) else { return };
        let Some(p) = doc.info.pages.get(page) else { return };
        let max_side = ctx.input(|i| i.max_texture_side) as f32;
        let scale = scale.min(crate::canvas::BASE_SIDE.min(max_side) / p.width.max(p.height).max(1.0));
        let tag = (scale * 1000.0) as u64;
        if self.tag != Some(tag) {
            renderer.set_queue(vec![pdfcraft_render::RenderRequest { page, scale, tag, ..Default::default() }]);
            self.tag = Some(tag);
        }
        if let Some(out) = renderer.try_recv() {
            if out.error.is_some() {
                // Without a background the item stays put while dragged, as before.
                self.renderer = None;
                return;
            }
            if out.request.tag == tag {
                self.texture = Some(ctx.load_texture(
                    "added-content-move-background",
                    crate::canvas::texture_image([out.width as usize, out.height as usize], out.rgba),
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        if self.pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    /// The move was applied: keep the preview, now at the new generation, until the page is
    /// rendered again. Any other edit ends it.
    pub fn committed(&mut self, edit: &Edit, generation: u64) {
        match edit {
            Edit::UpdateContent { page, index, content } if self.texture.is_some() && self.key.is_some_and(|(_, p, i)| (p, i) == (*page, *index)) => {
                self.key = Some((generation, *page, *index));
                self.settling = Some(content.rect());
            }
            _ => *self = Self::default(),
        }
    }

    pub fn page_received(&mut self, page: usize) {
        if self.settling.is_some() && self.key.is_some_and(|(_, p, _)| p == page) {
            *self = Self::default();
        }
    }

    /// The page without the moving item, then the item's pixels taken from `page_tex` (the page
    /// as rendered, item included where it was) at the dragged or applied position.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &PageXform,
        page: usize,
        info: &DocInfo,
        cv: &ContentView,
        page_tex: Option<egui::TextureId>,
    ) {
        let (Some((_, kp, _)), Some(background), Some(page_tex), Some(from)) = (self.key, &self.texture, page_tex, self.from) else { return };
        if kp != page {
            return;
        }
        let r = screen_rect(xf, info, page, from);
        let d = if let Some((_, _, start)) = cv.grab {
            let Some(p) = ui.input(|i| i.pointer.hover_pos()) else { return };
            p - start
        } else if let Some(to) = self.settling {
            // A moved text box keeps its top edge (its height follows its lines).
            screen_rect(xf, info, page, to).left_top() - r.left_top()
        } else {
            return;
        };
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(xf.rect));
        xf.paint_image(&painter, background.id(), 0.0, 0.0, 1.0, 1.0);
        let r = r.expand(1.0);
        let mut mesh = egui::Mesh::with_texture(page_tex);
        for c in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()] {
            let (u, v) = xf.screen_to_norm(c);
            mesh.vertices.push(egui::epaint::Vertex { pos: c + d, uv: Pos2::new(u, v), color: Color32::WHITE });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Text style for new text (the panel's Format controls set it).
pub fn default_style() -> AddedText {
    AddedText { size: 12.0, ..AddedText::default() }
}

/// Display-space rect → screen.
fn screen_rect(xf: &PageXform, info: &DocInfo, page: usize, r: [f64; 4]) -> Rect {
    let ph = info.pages[page].height as f64;
    xf.view_rect([r[0] as f32, (ph - r[3]) as f32, r[2] as f32, (ph - r[1]) as f32])
}

/// Screen point → display space.
fn to_display(xf: &PageXform, info: &DocInfo, page: usize, p: Pos2) -> [f64; 2] {
    let (vx, vy) = xf.screen_to_view(p);
    [vx as f64, info.pages[page].height as f64 - vy as f64]
}

fn on_page(added: &[Added], page: usize) -> Vec<(usize, &Added)> {
    added.iter().filter(|a| a.page == page).enumerate().collect()
}

pub(crate) fn hits_added(xf: &PageXform, info: &DocInfo, page: usize, added: &[Added], point: Pos2) -> bool {
    on_page(added, page).iter().any(|(_, a)| screen_rect(xf, info, page, a.content.rect()).expand(2.0).contains(point))
}

fn handles(r: Rect) -> [(Pos2, bool, bool); 4] {
    [(r.left_top(), true, true), (r.right_top(), false, true), (r.left_bottom(), true, false), (r.right_bottom(), false, false)]
}

/// The rect while moving or resizing (images keep their proportions).
fn dragged(r: Rect, grab: Grab, d: egui::Vec2, keep_aspect: bool) -> Rect {
    match grab {
        Grab::Move => r.translate(d),
        Grab::Corner(left, top) => {
            let (mut dx, mut dy) = (if left { -d.x } else { d.x }, if top { -d.y } else { d.y });
            if keep_aspect && r.width() > 0.0 && r.height() > 0.0 {
                let k = ((r.width() + dx) / r.width()).max((r.height() + dy) / r.height()).max(0.05);
                dx = r.width() * k - r.width();
                dy = r.height() * k - r.height();
            }
            let mut out = r;
            if left {
                out.min.x -= dx;
            } else {
                out.max.x += dx;
            }
            if top {
                out.min.y -= dy;
            } else {
                out.max.y += dy;
            }
            Rect::from_two_pos(out.min, out.max)
        }
    }
}

/// A draft's box (display space) moved by `delta`, kept on a `page_width` × `page_height` page:
/// its top edge stays on the page and at least the start of its first line stays visible.
fn moved_draft_rect(start: [f64; 4], delta: [f64; 2], page_width: f64, page_height: f64) -> [f64; 4] {
    // Finite inputs keep both clamps' bounds ordered (`clamp` panics otherwise).
    if !delta.iter().chain(&start).chain(&[page_width, page_height]).all(|v| v.is_finite()) {
        return start;
    }
    let (width, height) = (start[2] - start[0], start[3] - start[1]);
    let left = (start[0] + delta[0]).clamp(0.0, (page_width - 20.0).max(0.0));
    let top = (start[3] + delta[1]).clamp(height.min(page_height).max(0.0), page_height.max(0.0));
    [left, top - height, left + width, top]
}

/// Added text and images keep pointer ownership over a form field while editing content.
pub(crate) fn select_grabs(ui: &egui::Ui, xf: &PageXform, page: usize, info: &DocInfo, added: &[Added], view: &DocView) -> bool {
    let cv = &view.content;
    if cv.grab.is_some_and(|(grab_page, _, _)| grab_page == page) {
        return true;
    }
    let Some(p) = ui.input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos())).filter(|p| xf.rect.contains(*p)) else {
        return false;
    };
    on_page(added, page).into_iter().any(|(index, item)| {
        let rect = screen_rect(xf, info, page, item.content.rect());
        rect.expand(2.0).contains(p)
            || cv.selected == Some((page, index)) && handles(rect).into_iter().any(|(corner, _, _)| corner.distance(p) <= 6.0)
    })
}

/// What the text tool does with one pointer event. The item is the one under the press, not
/// under the pointer after it has moved: a drag that leaves a short box still moves that box,
/// and only a click that started on empty page starts a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextToolStep {
    /// Press began on an existing item and became a drag.
    Move,
    /// Press began on an existing item and was released as a click.
    Edit,
    /// Click began on empty page.
    NewBox,
    /// Button is down, or a drag that did not start on an item.
    Wait,
}

fn adding_text_step(drag_started: bool, clicked: bool, pressed_on_item: bool) -> TextToolStep {
    if drag_started && pressed_on_item {
        TextToolStep::Move
    } else if clicked && pressed_on_item {
        TextToolStep::Edit
    } else if clicked {
        TextToolStep::NewBox
    } else {
        TextToolStep::Wait
    }
}

/// Pointer input on one page in Edit mode (`adding_text` with the Text tool). `true` when it
/// used the input.
#[allow(clippy::too_many_arguments)]
pub(crate) fn page_input(
    ui: &egui::Ui,
    resp: &egui::Response,
    xf: &PageXform,
    page: usize,
    info: &DocInfo,
    added: &[Added],
    adding_text: bool,
    edit_on_click: bool,
    style: &AddedText,
    view: &mut DocView,
) -> bool {
    let mut consumed = false;
    let pointer = ui.input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos()));
    let cv = &mut view.content;
    if let Some((gp, grab, start)) = cv.grab
        && gp == page
    {
        consumed = true;
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            cv.grab = None;
            let end = pointer.unwrap_or(start);
            if let Some((sp, si)) = cv.selected
                && sp == page
                && let Some((_, a)) = on_page(added, page).into_iter().find(|(i, _)| *i == si)
                && (end - start).length() >= 1.0
            {
                let keep = matches!(a.content, AddedContent::Image(_));
                let r = dragged(screen_rect(xf, info, page, a.content.rect()), grab, end - start, keep);
                let (p0, p1) = (to_display(xf, info, page, r.left_bottom()), to_display(xf, info, page, r.right_top()));
                let mut rect = [p0[0], p0[1], p1[0], p1[1]];
                if let AddedContent::Text(t) = &a.content
                    && grab == Grab::Move
                {
                    // Text height follows its lines: keep the top edge's offset only.
                    rect = [rect[0], rect[3] - (t.rect[3] - t.rect[1]), rect[2], rect[3]];
                }
                view.pending_edit = Some(Edit::UpdateContent { page, index: si, content: a.content.with_rect(rect) });
            }
        }
        return consumed;
    }
    let Some(p) = pointer.filter(|p| xf.rect.contains(*p)) else { return consumed };
    let items = on_page(added, page);
    let covers = |at: Pos2, item: &Added| screen_rect(xf, info, page, item.content.rect()).expand(2.0).contains(at);
    let hit = items.iter().rev().find(|(_, a)| covers(p, a)).map(|(i, a)| (*i, *a));
    // Where the button went down. A later hover can already be outside a short text box.
    let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
    let at_press = items.iter().rev().find(|(_, a)| covers(origin, a)).map(|(i, a)| (*i, *a));
    let selected_rect = cv
        .selected
        .filter(|s| s.0 == page)
        .and_then(|(_, si)| items.iter().find(|(i, _)| *i == si))
        .map(|(_, a)| screen_rect(xf, info, page, a.content.rect()));
    let corner = selected_rect.and_then(|r| handles(r).into_iter().find(|(c, ..)| c.distance(p) <= 6.0));
    let corner_at_press = selected_rect.and_then(|r| handles(r).into_iter().find(|(c, ..)| c.distance(origin) <= 6.0));
    if resp.drag_started()
        && let Some((_, left, top)) = corner_at_press
    {
        cv.grab = Some((page, Grab::Corner(left, top), origin));
        return true;
    }
    if adding_text {
        match adding_text_step(resp.drag_started(), resp.clicked(), at_press.is_some()) {
            TextToolStep::Move => {
                if let Some((i, _)) = at_press {
                    cv.selected = Some((page, i));
                    cv.grab = Some((page, Grab::Move, origin));
                    return true;
                }
            }
            TextToolStep::NewBox => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                // Text that can't be drawn stays where it is being typed (#125).
                if cv.hold_blocked() {
                    return true;
                }
                // A click elsewhere keeps the text being typed and starts a new box (#74). The click
                // can reach the page before the editor sees it lose focus, so finish it here.
                let typed = cv.draft.take().and_then(|d| finish(cv, d, added));
                // The Format panel styles the new text, not the box just kept.
                (cv.selected, cv.select_added) = (None, None);
                let at = to_display(xf, info, page, origin);
                let mut style = style.clone();
                style.text.clear();
                let page_width = info.pages.get(page).map_or(f64::INFINITY, |p| p.width as f64);
                let rect = [at[0], at[1] - style.size * 1.2, (at[0] + 200.0).min(page_width), at[1]];
                style.rect = rect;
                cv.draft = Some(TextDraft { page, index: None, rect, text: String::new(), style, focus: true });
                if let Some(edit) = typed {
                    view.pending_edit = Some(edit);
                }
                return true;
            }
            TextToolStep::Wait if at_press.is_none() => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                return false;
            }
            TextToolStep::Edit | TextToolStep::Wait => {}
        }
    } else if resp.drag_started()
        && let Some((i, _)) = at_press
    {
        cv.selected = Some((page, i));
        cv.grab = Some((page, Grab::Move, origin));
        return true;
    }
    if let Some((_, left, top)) = corner {
        ui.ctx().set_cursor_icon(if left == top { egui::CursorIcon::ResizeNwSe } else { egui::CursorIcon::ResizeNeSw });
    } else if hit.is_some() || at_press.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
    }
    let clicked_item = if adding_text { at_press } else { hit };
    if (resp.double_clicked() || edit_on_click && resp.clicked() && corner.is_none())
        && let Some((i, a)) = clicked_item
        && let AddedContent::Text(t) = &a.content
    {
        if cv.hold_blocked() {
            return true;
        }
        if cv.draft.as_ref().is_some_and(|d| d.page == page && d.index == Some(i)) {
            return true;
        }
        if let Some(edit) = cv.finish_draft(added) {
            view.pending_edit = Some(edit);
        }
        cv.draft = Some(TextDraft { page, index: Some(i), rect: t.rect, text: t.text.clone(), style: t.clone(), focus: true });
        cv.selected = Some((page, i));
        consumed = true;
        return consumed;
    }
    if resp.clicked() {
        match hit {
            Some((i, _)) => {
                cv.selected = Some((page, i));
                consumed = true;
            }
            None if corner.is_none() => cv.selected = None,
            None => consumed = true,
        }
    }
    consumed || hit.is_some() && resp.is_pointer_button_down_on()
}

/// Outlines of added items (hover and selection with handles) and the box being dragged.
pub(crate) fn paint_page(ui: &egui::Ui, painter: &egui::Painter, xf: &PageXform, page: usize, info: &DocInfo, added: &[Added], view: &DocView) {
    let cv = &view.content;
    let pointer = ui.input(|i| i.pointer.hover_pos());
    for (i, a) in on_page(added, page) {
        if cv.draft.as_ref().is_some_and(|d| d.page == page && d.index == Some(i)) {
            continue;
        }
        let mut r = screen_rect(xf, info, page, a.content.rect());
        let selected = cv.selected == Some((page, i));
        if selected
            && let (Some((gp, g, start)), Some(p)) = (cv.grab, pointer)
            && gp == page
        {
            r = dragged(r, g, p - start, matches!(a.content, AddedContent::Image(_)));
        }
        if selected {
            painter.rect_stroke(r.expand(1.0), CornerRadius::ZERO, Stroke::new(1.5, SELECT_BLUE), egui::StrokeKind::Outside);
            for (c, ..) in handles(r.expand(1.0)) {
                painter.circle(c, 3.5, Color32::WHITE, Stroke::new(1.5, SELECT_BLUE));
            }
        } else if pointer.is_some_and(|p| r.expand(2.0).contains(p)) {
            painter.rect_stroke(r.expand(1.0), CornerRadius::ZERO, Stroke::new(1.0, SELECT_BLUE.gamma_multiply(0.7)), egui::StrokeKind::Outside);
        }
    }
}

/// Delete removes the selected item; Escape clears the selection.
pub(crate) fn keys(ctx: &egui::Context, view: &mut DocView) {
    if view.objects.input_blocked() || view.content.selected.is_none() || view.content.draft.is_some() || ctx.egui_wants_keyboard_input() {
        return;
    }
    let (del, esc) = ctx.input(|i| (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace), i.key_pressed(egui::Key::Escape)));
    if del && let Some((page, index)) = view.content.selected.take() {
        view.pending_edit = Some(Edit::DeleteContent { page, index });
    } else if esc {
        view.content.selected = None;
    }
}

/// After an add: select the new item.
pub(crate) fn after_refresh(view: &mut DocView, added: &[Added]) {
    if let Some((page, before)) = view.content.select_added.take()
        && on_page(added, page).len() > before
    {
        view.content.selected = Some((page, before));
    }
    if let Some((p, i)) = view.content.selected
        && on_page(added, p).len() <= i
    {
        view.content.selected = None;
    }
}

/// Whether the text-entry operation finishes this frame.
///
/// The Done button finishes it. Clicking away finishes it. Enter does not: while a Japanese
/// IME is converting, Enter confirms the conversion, and in the editor it only breaks a line.
/// A non-empty IME preedit also holds the operation open.
pub(crate) fn confirms_text_entry(done_clicked: bool, lost_focus: bool, pointer_on_buttons: bool, ime_composing: bool) -> bool {
    if ime_composing {
        return false;
    }
    done_clicked || lost_focus && !pointer_on_buttons
}

/// The in-place text editor, with Done and Discard beside it. Returns the edit once the text is
/// committed (a click away, or Done), and whether Done finished adding text. Discard or Escape
/// throws the draft away. `labelled` shows 確定 and キャンセル for the view-only text tool.
pub(crate) fn editor(
    ctx: &egui::Context,
    view: &mut DocView,
    info: &DocInfo,
    added: &[Added],
    labelled: bool,
    commit_on_focus_loss: bool,
) -> (Option<Edit>, bool) {
    let Some(page) = view.content.draft.as_ref().map(|d| d.page) else {
        view.content.draft_grab = None;
        return (None, false);
    };
    let Some(xf) = view.page_xform(page) else { return (None, false) };
    let zoom = xf.rect.width() / xf.pw.max(1.0);
    let Some((page_width, page_height)) = info.pages.get(page).map(|p| (p.width as f64, p.height as f64)) else { return (None, false) };
    if let Some((grab_page, from, start)) = view.content.draft_grab {
        let (pointer, down) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
        if grab_page != page {
            view.content.draft_grab = None;
        } else {
            if let (Some(p), Some(d)) = (pointer, view.content.draft.as_mut()) {
                let (a, b) = (to_display(&xf, info, page, from), to_display(&xf, info, page, p));
                d.style.rect = moved_draft_rect(start, [b[0] - a[0], b[1] - a[1]], page_width, page_height);
            }
            if !down {
                view.content.draft_grab = None;
            }
        }
    }
    let Some(draft) = view.content.draft.as_mut() else { return (None, false) };
    let font = crate::edit_text_ui::editor_font(&draft.style, (draft.style.size as f32 * zoom).clamp(1.0, 300.0));
    let text = AddedText { text: draft.text.clone(), ..draft.style.clone() };
    if let Some((key, z, width, rect)) = &view.content.layout
        && *key == text
        && *z == zoom
        && *width == page_width
    {
        draft.rect = *rect;
    } else {
        let source = draft.style.rect;
        if draft.index.is_some() && draft.text == draft.style.text {
            draft.rect = source;
        } else {
            let natural = ctx.fonts_mut(|f| f.layout_no_wrap(draft.text.clone(), font.clone(), Color32::BLACK).size().x) as f64 / zoom as f64;
            let width = natural.max(pdfcraft_engine::added_text_width(&text)) + 2.0;
            let available = page_width - source[0];
            let width = width.max(source[2] - source[0]).min(available.max(1.0));
            draft.rect = [source[0], source[1], source[0] + width, source[3]];
            draft.rect = pdfcraft_engine::added_text_rect(&AddedText { rect: draft.rect, ..text.clone() });
        }
        view.content.layout = Some((text, zoom, page_width, draft.rect));
    }
    let r = screen_rect(&xf, info, page, draft.rect);
    // Where the retyped item is drawn on the page, which stays put while its draft is dragged.
    let source_rect = draft
        .index
        .and_then(|index| on_page(added, page).into_iter().find(|(i, _)| *i == index))
        .map(|(_, a)| screen_rect(&xf, info, page, a.content.rect()));
    let background_ready = draft.index.is_some_and(|index| view.text_background.ready_added(page, index));
    let tokens = Tokens::get(ctx);
    let (mut commit, mut cancel, mut done) = (false, false, false);
    let mut undrawable = None;
    let (mut start_grab, mut editor_size) = (None, view.content.editor_size);
    let dragging = view.content.draft_grab.is_some();
    // Set for the frame before the editor borrows the draft. A prompt in front owns Escape, and
    // the focus it takes is not a click away.
    let page_keys = !view.objects.input_blocked();
    let memo = &view.content.undrawable;
    let margin = egui::vec2(DRAFT_FRAME, DRAFT_FRAME);
    let area = egui::Area::new(egui::Id::new(("added-text", view.id.0))).order(egui::Order::Foreground).fixed_pos(r.min - margin).constrain(false);
    area.show(ctx, |ui| {
        let Some(t) = view.content.draft.as_mut() else { return };
        ui.set_width(r.width() + 64.0 + DRAFT_FRAME);
        if let Some(source_rect) = source_rect.filter(|_| !background_ready) {
            ui.painter().rect_filled(source_rect.union(r).expand(1.0), CornerRadius::ZERO, Color32::WHITE);
        }
        // The frame around the text moves it. It is laid down before the text box, which is on
        // top of it inside, so a press in the text still edits. It senses clicks too: a drag-only
        // widget around a click widget would hand its presses to that widget.
        let frame_rect = Rect::from_min_size(r.min, editor_size.unwrap_or(egui::vec2(r.width().max(60.0), r.height()))).expand(DRAFT_FRAME);
        let frame = ui.interact(frame_rect, ui.id().with("added-text-move"), egui::Sense::click_and_drag());
        frame.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, crate::identity::localized("入力欄を移動", "Move text box")));
        if frame.drag_started() {
            start_grab = ui.input(|i| i.pointer.press_origin());
        }
        let active = dragging || frame.dragged() || frame.hovered();
        let band = SELECT_BLUE.gamma_multiply(if active { 0.45 } else { 0.2 });
        ui.painter().rect_stroke(frame_rect.shrink(DRAFT_FRAME / 2.0), CornerRadius::ZERO, Stroke::new(DRAFT_FRAME, band), egui::StrokeKind::Middle);
        // A press on the frame is not a click away: the text keeps its focus, so an IME
        // conversion in progress is not interrupted (asking for the focus back would end it).
        let hold_focus = dragging || frame.is_pointer_button_down_on() || frame.clicked() || frame.dragged() || frame.drag_stopped();
        let surrender = ui.ctx().options(|o| o.input_options.surrender_focus_on);
        if hold_focus {
            ui.ctx().options_mut(|o| o.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never);
        }
        // The box is white in every theme, so the caret must not follow the theme (pale in dark mode) (#655).
        ui.visuals_mut().text_cursor.stroke.color = crate::edit_text_ui::caret_color_on(Color32::WHITE);
        let [cr, cg, cb] = t.style.color.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let resp = egui::Frame::NONE.inner_margin(DRAFT_FRAME).show(ui, |ui| {
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut t.text)
                        .font(font)
                        .desired_width(r.width().max(60.0))
                        .desired_rows(1)
                        .background_color(if background_ready { Color32::TRANSPARENT } else { Color32::WHITE })
                        .text_color(Color32::from_rgb(cr, cg, cb))
                        .hint_text(tl!("Type text"))
                        .id_salt("added-text-edit"),
                );
                if t.focus {
                    resp.request_focus();
                    t.focus = false;
                }
                resp
            });
            let resp = resp.inner;
            ui.ctx().options_mut(|o| o.input_options.surrender_focus_on = surrender);
            editor_size = Some(resp.rect.size());
            if dragging || frame.dragged() || frame.hovered() && !resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
            }
            let buttons = egui::Frame::NONE
                .fill(tokens.card)
                .stroke(Stroke::new(1.0, tokens.border))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(2)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                    ui.horizontal(|ui| {
                        if labelled {
                            done = ui.button(crate::identity::localized("確定", "Done")).clicked();
                            cancel = ui.button(crate::identity::localized("キャンセル", "Cancel")).clicked();
                        } else {
                            done = crate::icons::button(ui, "check", 24.0, false, tl!("Done adding text")).clicked();
                            cancel = crate::icons::button(ui, "x", 24.0, false, tl!("Discard this text (Esc)")).clicked();
                        }
                    });
                })
                .response
                .rect;
            // Pressing a button takes the focus from the text: that is not a click away.
            let on_buttons = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| buttons.contains(p));
            let ime_composing = ui
                .input(|i| i.events.iter().any(|event| matches!(event, egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if !text.is_empty())));
            // Enter confirms an IME conversion or inserts a line. It is not a way to finish.
            // Escape discards only while this editor has the page. A prompt in front keeps the draft.
            cancel |= page_keys && ui.input(|i| i.key_pressed(egui::Key::Escape));
            // Focus leaving for a prompt, including Print, is not a click away.
            let away = commit_on_focus_loss && page_keys && resp.lost_focus() && !view.objects.hold_editor;
            commit = confirms_text_entry(done, away, on_buttons || hold_focus, ime_composing);
        });
        // Page text is drawn with the standard fonts; anything else would be saved as `?` (#125).
        undrawable = self::undrawable(t, memo);
        if let Some(c) = undrawable {
            let chosen = t.style.font_id.as_deref().filter(|id| pdfcraft_engine::standard_family(id).is_none());
            let message = if let Some(id) = chosen {
                if pdfcraft_fonts::catalog::get(id).is_some_and(|face| face.available() && face.can_preview()) {
                    catalog_missing_message(id, c)
                } else {
                    "指定フォントが利用できないため、別の書体へ替えずに編集を止めています。".into()
                }
            } else if matches!(u32::from(c), 0x3000..=0x30FF | 0x3400..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF) {
                crate::identity::localized(
                    "このビルドの日本語フォントでは、この文字をPDFに書き込めません。",
                    "This build's Japanese font cannot write this character into the PDF.",
                )
            } else {
                undrawable_message(c)
            };
            ui.add(egui::Label::new(egui::RichText::new(message).small().color(UNDRAWABLE)).wrap_mode(egui::TextWrapMode::Wrap));
        }
    });
    view.content.editor_size = editor_size;
    if let Some(from) = start_grab
        && let Some(d) = view.content.draft.as_ref()
    {
        view.content.draft_grab = Some((page, from, d.style.rect));
        ctx.request_repaint();
    }
    if cancel {
        (view.content.draft, view.content.draft_grab) = (None, None);
        return (None, false);
    }
    // Text that can't be drawn stays in the editor, with the warning, rather than being lost.
    if !commit || undrawable.is_some() {
        return (None, false);
    }
    view.content.draft_grab = None;
    let edit = view.content.draft.take().and_then(|d| finish(&mut view.content, d, added));
    (edit, done)
}

/// A finished draft as an edit: new text is added (and selected once it exists), retyped text
/// updated, and text emptied by retyping removed. Empty new text adds nothing.
fn finish(cv: &mut ContentView, d: TextDraft, added: &[Added]) -> Option<Edit> {
    let text = d.text.trim_end().to_string();
    match d.index {
        None if text.trim().is_empty() => None,
        None => {
            cv.select_added = Some((d.page, on_page(added, d.page).len()));
            Some(Edit::AddText { page: d.page, text: AddedText { rect: d.rect, text, ..d.style } })
        }
        Some(i) if text.trim().is_empty() => Some(Edit::DeleteContent { page: d.page, index: i }),
        Some(i) => {
            let (_, a) = on_page(added, d.page).into_iter().find(|(k, _)| *k == i)?;
            let AddedContent::Text(old) = &a.content else { return None };
            let updated = AddedText { text, rect: d.rect, ..d.style };
            (*old != updated).then_some(Edit::UpdateContent { page: d.page, index: i, content: AddedContent::Text(updated) })
        }
    }
}

/// Format text (shown in the Edit panel while a text item is selected, or for new text).
/// Returns the changed style; for a selected item the caller turns it into an update.
pub(crate) fn format_panel(ui: &mut egui::Ui, t: &Tokens, style: &AddedText) -> Option<AddedText> {
    let mut s = style.clone();
    widgets::section_title(ui, tl!("Format text"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("font-family").selected_text(s.family.label()).width(110.0).show_ui(ui, |ui| {
            for f in [FontFamily::Helvetica, FontFamily::Times, FontFamily::Courier] {
                if ui.selectable_value(&mut s.family, f, f.label()).changed() {
                    s.font_id = None;
                }
            }
        });
        // A list rather than a drag value: every change is an undo step.
        egui::ComboBox::from_id_salt("font-size").selected_text(format!("{} pt", s.size)).width(70.0).show_ui(ui, |ui| {
            for size in [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0, 48.0, 72.0] {
                ui.selectable_value(&mut s.size, size, format!("{size} pt"));
            }
        });
    });
    ui.horizontal(|ui| {
        if ui.selectable_label(s.bold, egui::RichText::new("B").strong()).on_hover_text(tl!("Bold")).clicked() {
            s.bold = !s.bold;
        }
        if ui.selectable_label(s.italic, egui::RichText::new("I").italics()).on_hover_text(tl!("Italic")).clicked() {
            s.italic = !s.italic;
        }
        ui.separator();
        for (a, icon, tip) in [
            (TextAlign::Left, "align-left", tl!("Align left")),
            (TextAlign::Center, "align-center", tl!("Centre")),
            (TextAlign::Right, "align-right", tl!("Align right")),
            (TextAlign::Justify, "align-justify", tl!("Justify")),
        ] {
            if crate::icons::button(ui, icon, 26.0, s.align == a, tip).clicked() {
                s.align = a;
            }
        }
    });
    let c = s.color;
    if let Some(picked) = crate::comments::swatch_grid(ui, Some(c)) {
        s.color = picked;
    }
    ui.label(egui::RichText::new(tl!("Thai and Arabic use regular fallback fonts when available.")).small().color(t.text_faint));
    (s != *style).then_some(s)
}

/// One compact row of typeface, size, style and colour. Colour opens a popup, so the bar does
/// not grow a swatch grid over the page. Items wrap onto another short row when the window is
/// narrow, instead of being clipped.
fn entry_format_row(ui: &mut egui::Ui, style: &AddedText) -> (Option<AddedText>, f32, bool) {
    let mut s = style.clone();
    let mut pick = false;
    let response = ui.horizontal_wrapped(|ui| {
        ui.set_min_height(28.0);
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
        let title = ui.label(egui::RichText::new(crate::identity::localized("文字入力", "Add text")).strong());
        title.on_hover_text(crate::identity::localized(
            "ページをクリックして入力します。変換中のEnterでは確定しません。入力中は青い枠を、確定後は文字をドラッグして移動できます。",
            "Click the page and type. Enter during IME conversion does not finish the text. Drag the blue frame to move the text while typing, or the text itself after Done.",
        ));
        ui.label(font_caption(&s)).on_hover_text(crate::identity::localized("選択中の書体", "Chosen typeface"));
        if crate::widgets::pill_button(ui, "フォントを選択…", false).clicked() {
            pick = true;
        }
        egui::ComboBox::from_id_salt("entry-font-size").selected_text(format!("{} pt", s.size)).width(72.0).show_ui(ui, |ui| {
            for size in [8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0, 48.0, 72.0] {
                ui.selectable_value(&mut s.size, size, format!("{size} pt"));
            }
        });
        if ui.selectable_label(s.bold, egui::RichText::new("B").strong()).on_hover_text(tl!("Bold")).clicked() {
            s.bold = !s.bold;
        }
        if ui.selectable_label(s.italic, egui::RichText::new("I").italics()).on_hover_text(tl!("Italic")).clicked() {
            s.italic = !s.italic;
        }
        if s.font_id.is_some() && s.italic {
            ui.label(egui::RichText::new("この書体に斜体はありません。斜体は付きません。").small().color(Color32::from_rgb(176, 48, 48)));
        } else if s.font_id.is_some() && s.bold && !catalog_has_bold(s.font_id.as_deref()) {
            ui.label(egui::RichText::new("この書体に太字（ウェイト700）がないため、選んだ太さのまま描きます。").small().color(Color32::from_rgb(176, 48, 48)));
        }
        for (align, icon, tip) in [
            (TextAlign::Left, "align-left", tl!("Align left")),
            (TextAlign::Center, "align-center", tl!("Centre")),
            (TextAlign::Right, "align-right", tl!("Align right")),
            (TextAlign::Justify, "align-justify", tl!("Justify")),
        ] {
            if crate::icons::button(ui, icon, 26.0, s.align == align, tip).clicked() {
                s.align = align;
            }
        }
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "文字色"));
        ui.painter().circle_filled(rect.center(), 8.0, crate::comments::color32(s.color));
        ui.painter().circle_stroke(rect.center(), 8.0, Stroke::new(1.0, Color32::from_black_alpha(50)));
        let resp = resp.on_hover_text(crate::identity::localized("文字色", "Text colour"));
        let picked = egui::Popup::menu(&resp).show(|ui| crate::comments::swatch_grid(ui, Some(s.color)));
        if let Some(color) = picked.and_then(|response| response.inner) {
            s.color = color;
        }
    });
    ((s != *style).then_some(s), response.response.rect.height(), pick)
}

fn font_caption(style: &AddedText) -> String {
    if let Some(id) = style.font_id.as_deref() {
        return match pdfcraft_fonts::catalog::get(id) {
            Some(face) if face.available() && face.can_preview() => face.display_name.to_string(),
            _ => "指定フォントが利用できない".into(),
        };
    }
    let mut name = style.family.label().to_string();
    if style.bold {
        name.push_str(" 太字");
    }
    if style.italic {
        name.push_str(" 斜体");
    }
    name
}

fn catalog_has_bold(id: Option<&str>) -> bool {
    let Some(id) = id else { return false };
    let face = pdfcraft_fonts::catalog::get(id);
    face.is_some_and(|face| face.weight == 700) || pdfcraft_fonts::catalog::bold_partner(id).is_some()
}

/// Open the shared font window for added text. The draft, its text and its place stay.
pub(crate) fn open_text_font(app: &mut crate::PdfCraftApp, style: &AddedText, sample: &str) -> bool {
    if app.dialog.is_some() {
        return false;
    }
    let text: String = sample.chars().filter(|c| !c.is_control()).take(64).collect();
    app.font_choice = None;
    app.font_return = None;
    app.date_font_row = None;
    app.font_hold = false;
    app.text_font = true;
    app.font_picker = crate::font_picker::State::opening_for(
        Some(pdfcraft_engine::current_font_id(style).as_str()),
        pdfcraft_fonts::catalog::Offer { raster: false, text, standard: true },
    );
    app.dialog = Some(crate::Dialog::FontPicker);
    true
}

/// Apply 決定 to the text being typed, or to the selected added text. Cancel leaves both.
pub(crate) fn apply_text_font_choice(app: &mut crate::PdfCraftApp) {
    let Some(choice) = app.font_choice.take() else {
        refocus_text_draft(app);
        return;
    };
    if let Some(index) = app.active
        && app.views.get(index).is_some_and(|view| view.content.draft.is_some())
    {
        let sample = app.views[index].content.draft.as_ref().map(|draft| draft.text.clone()).unwrap_or_default();
        if let Some(next) = chosen_style(app.views[index].content.draft.as_ref().map(|draft| draft.style.clone()).as_ref(), &choice, &sample)
            && let Some(draft) = app.views[index].content.draft.as_mut()
        {
            draft.style = AddedText { text: draft.text.clone(), rect: draft.rect, ..next };
            draft.focus = true;
            app.text_style = AddedText { text: String::new(), rect: [0.0; 4], ..draft.style.clone() };
        } else {
            refocus_text_draft(app);
        }
        return;
    }
    let selected = app.active_ids().and_then(|(index, doc_id)| {
        let (page, item) = app.views.get(index)?.content.selected?;
        let doc = app.session.get(doc_id)?;
        let added = doc.added().iter().filter(|added| added.page == page).nth(item)?;
        match &added.content {
            AddedContent::Text(text) => Some((page, item, text.clone())),
            AddedContent::Image(_) => None,
        }
    });
    if let Some((page, item, text)) = selected
        && let Some(next) = chosen_style(Some(&text), &choice, &text.text)
    {
        app.text_style = AddedText { text: String::new(), rect: [0.0; 4], ..next.clone() };
        app.apply_edit(Edit::UpdateContent { page, index: item, content: AddedContent::Text(next) });
        return;
    }
    if let Some(next) = chosen_style(Some(&app.text_style), &choice, "") {
        app.text_style = AddedText { text: String::new(), rect: [0.0; 4], ..next };
    }
}

fn chosen_style(style: Option<&AddedText>, choice: &str, sample: &str) -> Option<AddedText> {
    let text: String = sample.chars().filter(|c| !c.is_control()).take(64).collect();
    let offer = pdfcraft_fonts::catalog::Offer { raster: false, text, standard: true };
    let face = pdfcraft_fonts::catalog::get(choice).filter(|face| pdfcraft_fonts::catalog::face_offered(face, &offer))?;
    let mut next = style.cloned().unwrap_or_default();
    if let Some((family, bold)) = pdfcraft_engine::standard_family(choice) {
        next.family = family;
        next.bold = bold;
        next.font_id = None;
    } else {
        next.font_id = Some(choice.to_string());
        next.bold = face.weight == 700;
    }
    Some(next)
}

fn refocus_text_draft(app: &mut crate::PdfCraftApp) {
    if let Some(draft) = app.active.and_then(|index| app.views.get_mut(index)).and_then(|view| view.content.draft.as_mut()) {
        draft.focus = true;
    }
}

pub(crate) fn text_font_button(ui: &mut egui::Ui) -> bool {
    widgets::pill_button(ui, "フォントを選択…", false).clicked()
}

/// The view-only text tool: where to click, the preview's size and colour, and that Enter
/// during IME conversion does not finish the text. The bar stays short so the page stays visible.
pub(crate) fn text_entry_strip(app: &mut crate::PdfCraftApp, ui: &mut egui::Ui) {
    if app.quick_tool != crate::QuickTool::AddText {
        return;
    }
    let Some((_, id)) = app.active_ids() else { return };
    if !app.session.allows_added_text(id) {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let height_id = egui::Id::new("text_entry_bar_height");
    let height = ui.data(|data| data.get_temp::<f32>(height_id)).unwrap_or(40.0).clamp(36.0, 96.0);
    egui::Panel::top("text_entry_bar")
        .exact_size(height)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(12, 4)).stroke(egui::Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            let draft =
                app.active.and_then(|index| app.views.get(index)).and_then(|view| view.content.draft.as_ref().map(|draft| draft.style.clone()));
            let selected = app.active_ids().and_then(|(index, doc_id)| {
                let (page, item) = app.views.get(index)?.content.selected?;
                let doc = app.session.get(doc_id)?;
                let added = doc.added().iter().filter(|added| added.page == page).nth(item)?;
                match &added.content {
                    AddedContent::Text(text) => Some((page, item, text.clone())),
                    AddedContent::Image(_) => None,
                }
            });
            let style = draft.clone().or_else(|| selected.as_ref().map(|(_, _, text)| text.clone())).unwrap_or_else(|| app.text_style.clone());
            let (row, used, pick) = entry_format_row(ui, &style);
            let next = (used + 8.0).clamp(36.0, 96.0);
            if (next - height).abs() > 1.0 {
                ui.data_mut(|data| data.insert_temp(height_id, next));
                ui.ctx().request_repaint();
            }
            if let Some(next) = row {
                if draft.is_some()
                    && let Some(index) = app.active
                    && let Some(draft) = app.views.get_mut(index).and_then(|view| view.content.draft.as_mut())
                {
                    draft.style = AddedText { text: draft.text.clone(), rect: draft.rect, ..next.clone() };
                } else if let Some((page, item, _)) = selected {
                    app.apply_edit(Edit::UpdateContent { page, index: item, content: AddedContent::Text(next.clone()) });
                }
                app.text_style = AddedText { text: String::new(), rect: [0.0; 4], ..next };
            }
            if pick {
                let sample = app
                    .active
                    .and_then(|index| app.views.get(index))
                    .and_then(|view| view.content.draft.as_ref().map(|draft| draft.text.clone()))
                    .or_else(|| selected.as_ref().map(|(_, _, text)| text.text.clone()))
                    .unwrap_or_default();
                open_text_font(app, &style, &sample);
            }
        });
}

/// How to use the tools, under the Add content list.
pub(crate) fn hint(ui: &mut egui::Ui, t: &Tokens) {
    ui.label(
        egui::RichText::new(
            tl!("Click on the page to add text; each click starts a new box, and ✓ finishes. Drag items to move them, drag a corner to resize, double-click text to edit it."),
        )
            .small()
            .color(t.text_faint),
    );
    ui.add_space(4.0);
}

impl crate::PdfCraftApp {
    /// Pick an image file for the active document, read it, and call `then` with its name and
    /// bytes: now when `save_override` answers (tests and automation never see a native dialog),
    /// otherwise on a later frame, and only if that document is still the active one.
    #[cfg(not(target_arch = "wasm32"))]
    fn pick_image(&mut self, title: &str, then: impl FnOnce(&mut Self, String, Vec<u8>) + Send + 'static) {
        let read = |app: &mut Self, path: std::path::PathBuf| match std::fs::read(&path) {
            Ok(bytes) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                then(app, name, bytes);
            }
            Err(e) => app.notify_fmt("Couldn't read {name}: {e}", &[("name", &path.display().to_string()), ("e", &e.to_string())]),
        };
        match self.save_override.clone() {
            Some(p) if p.ends_with(".png") || p.ends_with(".jpg") => read(self, p.into()),
            Some(_) => {}
            None => {
                let dialog = rfd::AsyncFileDialog::new()
                    .add_filter(tl!("Images"), &["png", "jpg", "jpeg", "tif", "tiff", "gif", "bmp", "jp2", "j2k", "jpx"])
                    .set_title(title);
                let target = self.active_ids().map(|(_, id)| id);
                self.ask_one(crate::pickers::Ask::File(dialog), target, read);
            }
        }
    }

    /// Edit a PDF ▸ Add content ▸ Image: pick a file and place it in the middle of the current page.
    pub fn add_image_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.pick_image(tl!("Choose an image"), |app, name, bytes| app.add_image(name, bytes));
        #[cfg(target_arch = "wasm32")]
        self.notify_tr("Adding images arrives on the web with file pickers for images");
    }

    /// Edit text & images ▸ Replace Image: pick a file to draw in a page image's place.
    pub(crate) fn replace_page_image_dialog(&mut self, page: usize, index: usize) {
        #[cfg(not(target_arch = "wasm32"))]
        self.pick_image(tl!("Replace image"), move |app, name, bytes| {
            app.apply_edit(Edit::EditPageImage {
                page,
                index,
                change: pdfcraft_engine::ImageEdit::Replace { name, bytes: std::sync::Arc::new(bytes) },
            });
        });
        #[cfg(target_arch = "wasm32")]
        self.notify_fmt(
            "Replacing images on page {p} arrives on the web with image pickers ({i})",
            &[("p", &(page + 1).to_string()), ("i", &index.to_string())],
        );
    }

    /// Edit text & images ▸ Save Image As.
    pub(crate) fn save_page_image(&mut self, page: usize, index: usize) {
        let Some((_, id)) = self.active_ids() else { return };
        if !self.may_copy(id) {
            return;
        }
        let file = self.session.get(id).map(|d| d.page_image_file(page, index));
        match file {
            Some(Ok((ext, bytes))) => {
                self.write_files(&[(format!("Image page {} #{}.{ext}", page + 1, index + 1), std::sync::Arc::new(bytes))], "Save image");
            }
            Some(Err(e)) => self.notify_fmt("Couldn't save the image: {e}", &[("e", &e.to_string())]),
            None => {}
        }
    }

    /// Copy Image: image `index` on `page`, at its own resolution, to the clipboard.
    pub(crate) fn copy_page_image(&mut self, page: usize, index: usize) -> bool {
        if self.refuse_in_view_only() {
            return false;
        }
        let Some((_, id)) = self.active_ids() else { return false };
        if !self.may_copy(id) {
            return false;
        }
        let Some(pixels) = self.session.get(id).map(|d| d.page_image_rgba(page, index)) else { return false };
        let copied = pixels.and_then(|(w, h, rgba)| {
            #[cfg(not(target_arch = "wasm32"))]
            if self.system_clipboard {
                let img = arboard::ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Borrowed(&rgba) };
                arboard::Clipboard::new().and_then(|mut c| c.set_image(img)).map_err(|e| e.to_string())?;
            }
            #[cfg(target_arch = "wasm32")]
            if self.system_clipboard {
                return Err("the web version can't put images on the clipboard yet; use Save Image As".to_string());
            }
            self.last_copied_image = Some((w, h, rgba));
            Ok((w, h))
        });
        match copied {
            Ok((w, h)) => {
                self.notify_fmt("The image has been copied ({w} × {h} pixels)", &[("w", &w.to_string()), ("h", &h.to_string())]);
                true
            }
            Err(e) => {
                self.notify_fmt("Couldn't copy the image: {e}", &[("e", &e)]);
                false
            }
        }
    }

    /// The picture's visible box on its page, in user space.
    fn page_image_box(&self, page: usize, index: usize) -> Option<[f64; 4]> {
        let (_, id) = self.active_ids()?;
        let image = self.session.get(id)?.page_images(page).into_iter().nth(index)?;
        Some(image.visible_rect.unwrap_or(image.rect))
    }

    /// Cut: copy the picture, and only when that worked take it off the page (one undo step).
    pub(crate) fn cut_page_image(&mut self, page: usize, index: usize) {
        if self.copy_page_image(page, index) {
            self.apply_edit(Edit::EditPageImage { page, index, change: pdfcraft_engine::ImageEdit::Delete });
        }
    }

    /// Redact image: a redaction mark over the whole picture. It is applied, like any mark, from
    /// Redact a PDF.
    pub(crate) fn redact_page_image(&mut self, page: usize, index: usize) {
        let Some(rect) = self.page_image_box(page, index) else { return };
        let author = self.comment_prefs.author.clone();
        let mark = self.redact_prefs.mark(page, vec![pdfcraft_engine::rect_quad(rect)], &author);
        if self.apply_edit(mark) {
            self.notify_tr("The image is marked for redaction. Apply it with Redact a PDF");
        }
    }

    /// Create link: the link properties for a new link over the picture.
    pub(crate) fn link_page_image(&mut self, page: usize, index: usize) {
        if let Some(rect) = self.page_image_box(page, index) {
            self.open_link_props(page, Some(rect), None);
        }
    }

    /// Recognize text: the Recognize Text dialog, set to this page.
    pub fn recognize_text_on_page(&mut self, page: usize) {
        self.ocr_draft.pages = crate::ocr_ui::OcrPages::Range;
        self.ocr_draft.from = page + 1;
        self.ocr_draft.to = page + 1;
        self.dialog = Some(crate::Dialog::RecognizeText);
    }

    /// Click an image field: pick a picture for it.
    pub fn choose_field_image(&mut self, name: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let name = name.to_string();
            self.pick_image(tl!("Select Icon"), move |app, _, bytes| {
                app.apply_edit(Edit::SetFieldImage { name, image: std::sync::Arc::new(bytes) });
            });
        }
        #[cfg(target_arch = "wasm32")]
        self.notify_fmt("{name}: choosing images arrives on the web with file pickers for images", &[("name", &name)]);
    }

    /// Edit image ▸ Replace: pick a file for the selected image.
    pub fn replace_image_dialog(&mut self, page: usize, index: usize) {
        #[cfg(not(target_arch = "wasm32"))]
        self.pick_image(tl!("Replace image"), move |app, name, bytes| {
            app.apply_edit(Edit::ReplaceImage { page, index, name, bytes: std::sync::Arc::new(bytes) });
        });
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (page, index);
            self.notify_tr("Replacing images arrives on the web with image file pickers");
        }
    }

    /// Place image `bytes` in the middle of the current page and select it.
    pub fn add_image(&mut self, name: String, bytes: Vec<u8>) {
        let Some((i, id)) = self.active_ids() else { return };
        let page = self.views[i].current;
        let before = self.session.get(id).map_or(0, |d| d.added().iter().filter(|a| a.page == page).count());
        if self.apply_edit(Edit::AddImage { page, rect: None, name, bytes: std::sync::Arc::new(bytes) }) {
            self.views[i].content.select_added = Some((page, before));
            self.left = crate::LeftPanel::Tool("edit");
            self.left_open = true;
        }
    }
}

/// What the image tools ask for.
pub(crate) enum ImageAction {
    Update(pdfcraft_engine::AddedContent),
    Replace,
}

// Lucide names a flip icon after the axis it draws, Acrobat names the flip after the direction the
// picture moves: Flip horizontal (left and right swap) shows Lucide's vertical axis.
const FLIP_HORIZONTAL_ICON: &str = "flip-vertical-2";
const FLIP_VERTICAL_ICON: &str = "flip-horizontal-2";

/// Edit image: rotate, flip, crop, replace (shown while an added image is selected).
pub(crate) fn image_panel(ui: &mut egui::Ui, t: &Tokens, img: &pdfcraft_engine::AddedImage) -> Option<ImageAction> {
    let mut out = None;
    widgets::section_title(ui, tl!("Edit image"));
    ui.horizontal(|ui| {
        let mut i = img.clone();
        let r = i.rect;
        let turn = |i: &mut pdfcraft_engine::AddedImage, k: u8| {
            i.rotation = (i.rotation + k) % 4;
            // The box turns with the picture, around its centre.
            let (cx, cy, w, h) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0, r[2] - r[0], r[3] - r[1]);
            i.rect = [cx - h / 2.0, cy - w / 2.0, cx + h / 2.0, cy + w / 2.0];
        };
        if crate::icons::button(ui, "rotate-ccw", 28.0, false, tl!("Rotate counterclockwise")).clicked() {
            turn(&mut i, 1);
            out = Some(ImageAction::Update(AddedContent::Image(i.clone())));
        }
        if crate::icons::button(ui, "rotate-cw", 28.0, false, tl!("Rotate clockwise")).clicked() {
            turn(&mut i, 3);
            out = Some(ImageAction::Update(AddedContent::Image(i.clone())));
        }
        if crate::icons::button(ui, FLIP_HORIZONTAL_ICON, 28.0, false, tl!("Flip horizontal")).clicked() {
            i.flip_h = !i.flip_h;
            out = Some(ImageAction::Update(AddedContent::Image(i.clone())));
        }
        if crate::icons::button(ui, FLIP_VERTICAL_ICON, 28.0, false, tl!("Flip vertical")).clicked() {
            i.flip_v = !i.flip_v;
            out = Some(ImageAction::Update(AddedContent::Image(i.clone())));
        }
        if crate::icons::button(ui, "replace", 28.0, false, tl!("Replace image")).clicked() {
            out = Some(ImageAction::Replace);
        }
    });
    ui.label(egui::RichText::new(tl!("Crop (% trimmed from each side)")).small().color(t.text_muted));
    let mut crop = img.crop.map(|v| (v * 100.0).round());
    let mut changed = false;
    ui.horizontal(|ui| {
        for (k, label) in ["L", "B", "R", "T"].into_iter().enumerate() {
            ui.label(label);
            // Applied when the drag ends, so a drag is one undo step.
            let r = ui.add(egui::DragValue::new(&mut crop[k]).range(0.0..=45.0).speed(0.5).suffix("%"));
            changed |= r.drag_stopped() || (r.changed() && !r.dragged());
        }
    });
    if changed {
        let mut i = img.clone();
        i.crop = crop.map(|v| v / 100.0);
        if i.crop != img.crop {
            out = Some(ImageAction::Update(AddedContent::Image(i)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    struct PolicyRestore((bool, bool, bool));

    impl Drop for PolicyRestore {
        fn drop(&mut self) {
            crate::output_policy::set_blocked(self.0.0);
            crate::output_policy::set_save_as_allowed(self.0.1);
            crate::output_policy::set_print_allowed(self.0.2);
        }
    }

    /// The image a flip button leaves behind, as the panel returns it.
    fn flipped(tooltip: &str) -> pdfcraft_engine::AddedImage {
        let img = pdfcraft_engine::AddedImage::new([0.0, 0.0, 100.0, 50.0], pdfcraft_cos::ObjRef::new(7, 0));
        let mut fonts = false;
        let mut h = egui_kittest::Harness::new_ui_state(
            move |ui, out: &mut Option<pdfcraft_engine::AddedImage>| {
                // The app's fonts (the semibold section title) take effect from the next frame.
                if !fonts {
                    ui.ctx().set_fonts(crate::theme::font_definitions());
                    fonts = true;
                    return;
                }
                let t = Tokens::get(ui.ctx());
                if let Some(ImageAction::Update(AddedContent::Image(i))) = image_panel(ui, &t, &img) {
                    *out = Some(i);
                }
            },
            None,
        );
        h.run();
        h.get_by_label(tooltip).click();
        h.run();
        h.state().clone().expect("the button updates the image")
    }

    #[test]
    fn flip_buttons_show_the_mirror_axis_of_the_flip_they_make() {
        // #1834: Acrobat's Flip horizontal swaps left and right across a vertical axis. Lucide names
        // its flip icons after the axis drawn, so that is its flip-vertical-2.
        let svg = |name: &str| {
            let (_, bytes) = crate::icon_data::ICONS.iter().find(|(n, _)| *n == name).expect("bundled icon");
            String::from_utf8_lossy(bytes).into_owned()
        };
        let (h, v) = (svg(FLIP_HORIZONTAL_ICON), svg(FLIP_VERTICAL_ICON));
        assert!(h.contains("M12 2v2") && !h.contains("M4 12H2"), "Flip horizontal draws a vertical axis:\n{h}");
        assert!(v.contains("M4 12H2") && !v.contains("M12 2v2"), "Flip vertical draws a horizontal axis:\n{v}");

        let i = flipped("Flip horizontal");
        assert!(i.flip_h && !i.flip_v, "Flip horizontal mirrors left and right");
        let i = flipped("Flip vertical");
        assert!(i.flip_v && !i.flip_h, "Flip vertical mirrors top and bottom");
    }

    #[test]
    fn a_drag_that_leaves_the_box_moves_it_and_a_click_on_empty_page_starts_a_new_one() {
        assert_eq!(super::adding_text_step(true, false, true), super::TextToolStep::Move);
        assert_eq!(super::adding_text_step(true, true, true), super::TextToolStep::Move, "a drag is not also a new box");
        assert_eq!(super::adding_text_step(false, true, true), super::TextToolStep::Edit);
        assert_eq!(super::adding_text_step(false, true, false), super::TextToolStep::NewBox);
        assert_eq!(super::adding_text_step(true, false, false), super::TextToolStep::Wait, "a drag from empty page adds nothing");
        assert_eq!(super::adding_text_step(false, false, true), super::TextToolStep::Wait);
    }

    #[test]
    fn a_dragged_draft_moves_whole_and_stays_on_its_page() {
        let start = [40.0, 250.0, 240.0, 264.0];
        assert_eq!(super::moved_draft_rect(start, [10.0, -30.0], 300.0, 300.0), [50.0, 220.0, 250.0, 234.0]);
        assert_eq!(super::moved_draft_rect(start, [-100.0, 100.0], 300.0, 300.0), [0.0, 286.0, 200.0, 300.0], "kept inside the top left");
        assert_eq!(super::moved_draft_rect(start, [500.0, -500.0], 300.0, 300.0), [280.0, 0.0, 480.0, 14.0], "its first letters stay on the page");
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(super::moved_draft_rect(start, [bad, 0.0], 300.0, 300.0), start);
            assert_eq!(super::moved_draft_rect(start, [5.0, 5.0], 300.0, bad), start, "a bad page size leaves the box where it is");
        }
        // A box taller than its page, and a page with no size, do not cross the clamp bounds.
        assert_eq!(super::moved_draft_rect([0.0, 0.0, 10.0, 500.0], [0.0, 0.0], 300.0, 300.0), [0.0, -200.0, 10.0, 300.0]);
        assert_eq!(super::moved_draft_rect(start, [5.0, 5.0], 0.0, 0.0), [0.0, -14.0, 200.0, 0.0]);
    }

    #[test]
    fn enter_and_ime_preedit_do_not_finish_text_entry() {
        assert!(!super::confirms_text_entry(false, false, false, false), "Enter is not a finish signal");
        assert!(!super::confirms_text_entry(false, true, false, true), "a conversion in progress stays open");
        assert!(!super::confirms_text_entry(true, false, false, true), "Done during conversion does not finish the operation");
        assert!(super::confirms_text_entry(true, false, false, false), "Done finishes once conversion has ended");
        assert!(super::confirms_text_entry(false, true, false, false), "clicking away finishes");
        assert!(!super::confirms_text_entry(false, true, true, false), "the confirm buttons are not a click away");
    }

    fn added(id: &str, text: &str) -> pdfcraft_engine::AddedText {
        pdfcraft_engine::AddedText { text: text.to_string(), font_id: Some(id.to_string()), ..pdfcraft_engine::AddedText::default() }
    }

    fn notice(id: &str, text: &str) -> Option<String> {
        pdfcraft_engine::first_undrawable(&added(id, text)).map(|c| super::catalog_missing_message(id, c))
    }

    /// A Japanese letter the face does not contain. The search stays small and stops at the first miss.
    fn absent_japanese(id: &str) -> Option<char> {
        let face = pdfcraft_fonts::catalog::get(id)?;
        (0x3400u32..=0x3410)
            .chain(0x20000u32..=0x20020)
            .filter_map(char::from_u32)
            .find(|c| pdfcraft_fonts::catalog::is_japanese_letter(*c) && !face.has_glyph(*c))
    }

    fn absent_other(id: &str) -> Option<char> {
        let face = pdfcraft_fonts::catalog::get(id)?;
        ['ก', '☺', 'ℵ', 'Ω'].into_iter().find(|c| !pdfcraft_fonts::catalog::is_japanese_letter(*c) && !face.has_glyph(*c))
    }

    #[test]
    fn a_latin_face_warns_that_it_has_no_japanese_and_a_japanese_face_names_the_glyph() {
        let no_japanese = "このフォントは日本語に対応していません。日本語対応フォントを選択してください。";
        assert_eq!(notice("bundled:inter:regular", "あ").as_deref(), Some(no_japanese));
        assert_eq!(notice("bundled:inter:regular", "東京").as_deref(), Some(no_japanese));
        assert_eq!(notice("bundled:jetbrains-mono:regular", "あ").as_deref(), Some(no_japanese));
        assert_eq!(notice("bundled:inter:medium", "ア").as_deref(), Some(no_japanese));

        let gothic = "craft:biz-udp-gothic:regular";
        let face = pdfcraft_fonts::catalog::get(gothic).unwrap();
        assert!(face.program().is_some(), "this build embeds the Japanese faces");
        let missing = absent_japanese(gothic).unwrap();
        let named = notice(gothic, &missing.to_string()).unwrap();
        assert_eq!(named, format!("このフォントには『{missing}』の文字が収録されていません。別のフォントを選択してください。"));
        assert!(!named.contains("日本語に対応していません"));

        let other = absent_other("bundled:inter:regular").unwrap();
        let latin_gap = super::catalog_missing_message("bundled:inter:regular", other);
        assert_eq!(latin_gap, format!("このフォントには『{other}』の文字が収録されていません。別のフォントを選択してください。"));
        assert!(!latin_gap.contains("日本語に対応していません"));

        assert!(notice(gothic, "東京").is_none(), "a Japanese face that can draw the text shows no warning");
        assert!(notice(gothic, "あ").is_none());

        // A face that already has some Japanese letters is not called unsupported, even when the
        // missing character is あ. An unreadable face is not called unsupported either. A Latin
        // gap on a face with no Japanese is also named as that character. Easy and detailed mode
        // both call this function from the shared text editor.
        let partial = super::glyph_gap_message(Some(true), 'あ');
        assert!(partial.contains('あ') && !partial.contains("日本語に対応していません"));
        let unreadable = super::glyph_gap_message(None, '東');
        assert!(unreadable.contains('東') && !unreadable.contains("日本語に対応していません"));
        assert_eq!(super::glyph_gap_message(Some(false), 'あ'), no_japanese);
        assert_eq!(super::glyph_gap_message(Some(false), 'ア'), no_japanese);
        assert_eq!(super::glyph_gap_message(Some(false), '漢'), no_japanese);
        let latin = super::glyph_gap_message(Some(false), 'Q');
        assert!(latin.contains('Q') && !latin.contains("日本語に対応していません"));

        let mut draft = TextDraft {
            page: 0,
            index: None,
            rect: [0.0; 4],
            text: "東京テラス".to_string(),
            style: added("bundled:inter:regular", ""),
            focus: true,
        };
        draft.style.text.clear();
        let kept = draft.text.clone();
        let memo = UndrawableMemo::default();
        assert_eq!(undrawable(&draft, &memo), Some('東'));
        assert_eq!(draft.text, kept);
        assert_eq!(super::catalog_missing_message("bundled:inter:regular", '東'), no_japanese);
    }

    #[test]
    fn a_font_choice_changes_only_the_face_and_cancel_keeps_the_draft() {
        let _policy =
            PolicyRestore((crate::output_policy::blocked(), crate::output_policy::save_as_allowed(), crate::output_policy::print_allowed()));
        let mut app = crate::PdfCraftApp::new();
        let bytes = app.session.create_blank(200.0, 300.0, 1).unwrap().to_vec();
        app.set_view_only(true);
        app.grant_edit_text();
        app.open_bytes("blank.pdf", None, bytes).unwrap();
        app.quick_tool = crate::QuickTool::AddText;
        app.dialog = None;
        let index = app.active.expect("blank document");
        app.views[index].content.draft = Some(TextDraft {
            page: 0,
            index: None,
            rect: [40.0, 40.0, 180.0, 80.0],
            text: "下書きABC".into(),
            style: AddedText { size: 16.0, color: [0.2, 0.0, 0.0], ..AddedText::default() },
            focus: true,
        });
        let style = app.views[index].content.draft.as_ref().unwrap().style.clone();
        assert!(open_text_font(&mut app, &style, "下書きABC"));
        assert!(matches!(app.dialog, Some(crate::Dialog::FontPicker)));
        apply_text_font_choice(&mut app);
        app.dialog = None;
        let draft = app.views[index].content.draft.as_ref().unwrap();
        assert_eq!(draft.text, "下書きABC");
        assert!(draft.style.font_id.is_none());
        assert_eq!(draft.style.size, 16.0);
        assert!((draft.style.color[0] - 0.2).abs() < 1e-9);

        let style = draft.style.clone();
        assert!(open_text_font(&mut app, &style, "下書きABC"));
        app.font_choice = Some("bundled:inter:regular".into());
        apply_text_font_choice(&mut app);
        app.dialog = None;
        assert!(app.views[index].content.draft.as_ref().unwrap().style.font_id.is_none(), "Inter cannot draw 下書き");
        assert_eq!(app.views[index].content.draft.as_ref().unwrap().text, "下書きABC");

        app.views[index].content.draft.as_mut().unwrap().text = "ABC".into();
        let style = app.views[index].content.draft.as_ref().unwrap().style.clone();
        assert!(open_text_font(&mut app, &style, "ABC"));
        app.font_choice = Some("bundled:inter:semibold".into());
        apply_text_font_choice(&mut app);
        app.dialog = None;
        let draft = app.views[index].content.draft.as_ref().unwrap();
        assert_eq!(draft.style.font_id.as_deref(), Some("bundled:inter:semibold"));
        assert!(!draft.style.bold);
        assert_eq!(draft.text, "ABC");
        assert_eq!(draft.style.size, 16.0);

        let style = draft.style.clone();
        assert!(open_text_font(&mut app, &style, "ABC"));
        app.font_choice = Some("standard:times:bold".into());
        apply_text_font_choice(&mut app);
        let draft = app.views[index].content.draft.as_ref().unwrap();
        assert!(draft.style.font_id.is_none());
        assert_eq!(draft.style.family, FontFamily::Times);
        assert!(draft.style.bold);
        assert_eq!(draft.text, "ABC");
    }
}
