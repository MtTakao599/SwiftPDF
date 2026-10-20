//! Faces the font window can offer. Ids are stable and do not include font bytes in settings.
//!
//! A face is listed only when this build may use it in a PDF. Standard-14 faces have no bundled
//! program, so they are not previewed with a different typeface. Craft and bundled faces preview
//! only their own glyphs: a missing character is skipped, never filled from another face.

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

use crate::embedding;

/// The sample the font window shows until the caller changes it.
pub const DEFAULT_SAMPLE: &str = "東京テラス 2026/10/20";

const SAMPLE_CHARS: usize = 32;
const MAX_SIDE: u32 = 960;

/// Where a listed face comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSource {
    /// A PDF standard-14 name. No program is embedded.
    Standard14,
    /// A craft-fonts face compiled into this build, when the build has one.
    Craft,
    /// An OFL face already shipped with the application.
    Bundled,
}

/// How a later caller may put the face into a PDF. This crate does not write the PDF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfMethod {
    /// `/BaseFont` name, no embedded program.
    Standard14Name(&'static str),
    /// Outlines of the characters actually used, from the bundled program.
    Type3Outlines,
    /// The whole program, after [`embedding::validate_document_input`].
    EmbedProgram,
}

#[derive(Clone, Copy, Debug)]
enum Program {
    None,
    Craft { family: &'static str, style: &'static str },
    Bundled(&'static [u8]),
}

/// One selectable or previewable face. Ids stay stable for saved date stamps and added text.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub id: &'static str,
    pub display_name: &'static str,
    pub family: &'static str,
    pub style: &'static str,
    /// 400 regular, 500 medium, 600 semibold, 700 bold. A bold switch looks for weight 700.
    pub weight: u16,
    pub source: FontSource,
    pub pdf: PdfMethod,
    program: Program,
}

const INTER_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
// Dancing Script stays out of this list. The shipped file is a variable font (`fvar`),
// and `embedding::validate` refuses variable fonts. Signatures still use it on their own path.

/// Every face the catalogue knows. Unavailable ones stay here so an id from an old file still
/// resolves, but [`Face::available`] is false and the window does not offer them.
pub fn faces() -> &'static [Face] {
    &FACES
}

const FACES: [Face; 14] = [
    face(
        "craft:biz-udp-gothic:regular",
        "BIZ UDPゴシック 標準",
        "BIZ UDPGothic",
        "Regular",
        400,
        FontSource::Craft,
        PdfMethod::Type3Outlines,
        Program::Craft { family: "BIZ UDPGothic", style: "Regular" },
    ),
    face(
        "craft:biz-udp-gothic:bold",
        "BIZ UDPゴシック 太字",
        "BIZ UDPGothic",
        "Bold",
        700,
        FontSource::Craft,
        PdfMethod::Type3Outlines,
        Program::Craft { family: "BIZ UDPGothic", style: "Bold" },
    ),
    face(
        "craft:shippori-mincho:regular",
        "しっぽり明朝 標準",
        "Shippori Mincho",
        "Regular",
        400,
        FontSource::Craft,
        PdfMethod::Type3Outlines,
        Program::Craft { family: "Shippori Mincho", style: "Regular" },
    ),
    face(
        "craft:biz-ud-mincho:regular",
        "BIZ UD明朝 標準",
        "BIZ UDMincho",
        "Regular",
        400,
        FontSource::Craft,
        PdfMethod::Type3Outlines,
        Program::Craft { family: "BIZ UDMincho", style: "Regular" },
    ),
    face(
        "standard:helvetica:regular",
        "Helvetica 標準",
        "Helvetica",
        "Regular",
        400,
        FontSource::Standard14,
        PdfMethod::Standard14Name("Helvetica"),
        Program::None,
    ),
    face(
        "standard:helvetica:bold",
        "Helvetica 太字",
        "Helvetica",
        "Bold",
        700,
        FontSource::Standard14,
        PdfMethod::Standard14Name("Helvetica-Bold"),
        Program::None,
    ),
    face(
        "standard:times:regular",
        "Times 標準",
        "Times",
        "Regular",
        400,
        FontSource::Standard14,
        PdfMethod::Standard14Name("Times-Roman"),
        Program::None,
    ),
    face("standard:times:bold", "Times 太字", "Times", "Bold", 700, FontSource::Standard14, PdfMethod::Standard14Name("Times-Bold"), Program::None),
    face(
        "standard:courier:regular",
        "Courier 標準",
        "Courier",
        "Regular",
        400,
        FontSource::Standard14,
        PdfMethod::Standard14Name("Courier"),
        Program::None,
    ),
    face(
        "standard:courier:bold",
        "Courier 太字",
        "Courier",
        "Bold",
        700,
        FontSource::Standard14,
        PdfMethod::Standard14Name("Courier-Bold"),
        Program::None,
    ),
    face(
        "bundled:inter:regular",
        "Inter 標準",
        "Inter",
        "Regular",
        400,
        FontSource::Bundled,
        PdfMethod::EmbedProgram,
        Program::Bundled(crate::INTER_REGULAR),
    ),
    face("bundled:inter:medium", "Inter 中太", "Inter", "Medium", 500, FontSource::Bundled, PdfMethod::EmbedProgram, Program::Bundled(INTER_MEDIUM)),
    face(
        "bundled:inter:semibold",
        "Inter セミボールド",
        "Inter",
        "SemiBold",
        600,
        FontSource::Bundled,
        PdfMethod::EmbedProgram,
        Program::Bundled(INTER_SEMIBOLD),
    ),
    face(
        "bundled:jetbrains-mono:regular",
        "JetBrains Mono 標準",
        "JetBrains Mono",
        "Regular",
        400,
        FontSource::Bundled,
        PdfMethod::EmbedProgram,
        Program::Bundled(JETBRAINS_MONO),
    ),
];

#[allow(clippy::too_many_arguments)]
const fn face(
    id: &'static str,
    display_name: &'static str,
    family: &'static str,
    style: &'static str,
    weight: u16,
    source: FontSource,
    pdf: PdfMethod,
    program: Program,
) -> Face {
    Face { id, display_name, family, style, weight, source, pdf, program }
}

impl Face {
    /// The program used for a preview and, for craft or bundled faces, for PDF output.
    pub fn program(&self) -> Option<&'static [u8]> {
        match self.program {
            Program::None => None,
            Program::Bundled(bytes) => Some(bytes),
            Program::Craft { family, style } => crate::CRAFT_FONTS.iter().find(|f| f.family == family && f.style == style).map(|f| f.bytes),
        }
    }

    /// Whether a caller may offer this face. A missing craft-fonts build, or a bundled file
    /// whose name table does not declare an allowed licence, is not offered.
    pub fn available(&self) -> bool {
        match self.pdf {
            PdfMethod::Standard14Name(_) => true,
            PdfMethod::Type3Outlines => self.program().is_some(),
            PdfMethod::EmbedProgram => self.program().is_some_and(|bytes| embedding::validate_document_input(bytes).is_ok()),
        }
    }

    /// Real outlines exist. Standard-14 faces are false: nothing is drawn in their place.
    pub fn can_preview(&self) -> bool {
        self.program().is_some()
    }

    pub fn has_glyph(&self, ch: char) -> bool {
        let Some(bytes) = self.program() else { return false };
        let Ok(font) = FontRef::new(bytes) else { return false };
        font.charmap().map(ch).is_some_and(|gid| gid.to_u32() != 0)
    }

    /// The default Japanese sample's letters are all in this face.
    pub fn japanese_sample(&self) -> bool {
        ['東', '京', 'テ', 'ラ', 'ス'].into_iter().all(|ch| self.has_glyph(ch))
    }

    /// Whether this face can draw Japanese letters.
    ///
    /// `Some(true)` when at least one hiragana, katakana, or kanji glyph is present, including a
    /// face that has only some of them. The catalogue sample ([`Self::japanese_sample`]) is the
    /// fast positive check; a face that fails it is still Japanese-capable when any other letter
    /// in those scripts is mapped. `Some(false)` when the program was read and none of those
    /// letters are mapped. `None` when there is no program or it cannot be read: that is not
    /// evidence the face lacks Japanese.
    pub fn supports_japanese(&self) -> Option<bool> {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock, PoisonError};
        static CACHE: OnceLock<Mutex<HashMap<&'static str, Option<bool>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(answer) = guard.get(self.id) {
            return *answer;
        }
        let answer = self.supports_japanese_uncached();
        guard.insert(self.id, answer);
        answer
    }

    fn supports_japanese_uncached(&self) -> Option<bool> {
        let bytes = self.program()?;
        let font = FontRef::new(bytes).ok()?;
        let map = font.charmap();
        // Same letters as `japanese_sample`: a complete sample means the face is Japanese-capable.
        if ['東', '京', 'テ', 'ラ', 'ス'].into_iter().all(|ch| map.map(ch).is_some_and(|gid| gid.to_u32() != 0)) {
            return Some(true);
        }
        // A face that holds only some Japanese letters is still Japanese-capable.
        let any = map.mappings().any(|(cp, _)| char::from_u32(cp).is_some_and(is_japanese_letter));
        Some(any)
    }
}

/// Hiragana, katakana, or a CJK ideograph. Punctuation and fullwidth Latin are left out, so a
/// missing mark on a Latin face is a missing character rather than a font with no Japanese.
pub fn is_japanese_letter(c: char) -> bool {
    matches!(
        u32::from(c),
        0x3040..=0x309F // hiragana
            | 0x30A0..=0x30FF // katakana
            | 0x31F0..=0x31FF // katakana phonetic extensions
            | 0x3400..=0x4DBF // CJK extension A
            | 0x4E00..=0x9FFF // CJK unified ideographs
            | 0xF900..=0xFAFF // CJK compatibility ideographs
            | 0xFF66..=0xFF9D // halfwidth katakana
            | 0x1AFF0..=0x1B16F // kana supplement and extensions
            | 0x20000..=0x2A6DF // CJK extension B
            | 0x2A700..=0x2B73F // CJK extension C
            | 0x2B740..=0x2B81F // CJK extension D
            | 0x2B820..=0x2CEAF // CJK extension E
            | 0x2CEB0..=0x2EBEF // CJK extension F
            | 0x2F800..=0x2FA1F // CJK compatibility ideographs supplement
            | 0x30000..=0x3134F // CJK extension G
    )
}

/// Which faces a caller wants in the shared window. Date stamps ask for outlines and for the
/// characters on that row. Added text can use a different [`Offer`] later.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Offer {
    /// The face must have its own outlines. Standard-14 names have none, so they drop out.
    pub raster: bool,
    /// Visible characters the face must contain. Whitespace is not required to have a glyph.
    pub text: String,
    /// Keep standard-14 names even when they have no outlines and lack `text`.
    /// Added text uses them as the existing three families. Date stamps leave this false.
    pub standard: bool,
}

/// Characters in `text` that this face cannot draw, at most eight distinct ones.
pub fn missing_glyphs(face: &Face, text: &str) -> String {
    let mut missing = String::new();
    for ch in text.chars() {
        if ch.is_control() || ch.is_whitespace() || missing.contains(ch) {
            continue;
        }
        if !face.has_glyph(ch) {
            missing.push(ch);
            if missing.chars().count() >= 8 {
                break;
            }
        }
    }
    missing
}

/// Whether the shared window may offer `face` for this use.
pub fn face_offered(face: &Face, offer: &Offer) -> bool {
    if !face.available() {
        return false;
    }
    if matches!(face.pdf, PdfMethod::Standard14Name(_)) {
        if offer.raster {
            return false;
        }
        if offer.standard {
            return true;
        }
        return missing_glyphs(face, &offer.text).is_empty();
    }
    if offer.raster && !face.can_preview() {
        return false;
    }
    if !face.can_preview() {
        return false;
    }
    missing_glyphs(face, &offer.text).is_empty()
}

/// The face for `id`, including one this build cannot offer.
pub fn get(id: &str) -> Option<&'static Face> {
    faces().iter().find(|face| face.id == id)
}

/// Faces the window may list.
pub fn available_faces() -> impl Iterator<Item = &'static Face> {
    faces().iter().filter(|face| face.available())
}

/// The bold face of the same family, when one is available. Medium and semibold are not bold.
pub fn bold_partner(id: &str) -> Option<&'static str> {
    let face = get(id)?;
    faces().iter().find(|other| other.family == face.family && other.weight == 700 && other.available()).map(|other| other.id)
}

/// The regular face of the same family, when one is available.
pub fn regular_partner(id: &str) -> Option<&'static str> {
    let face = get(id)?;
    faces().iter().find(|other| other.family == face.family && other.weight == 400 && other.available()).map(|other| other.id)
}

/// Case-insensitive match on the display name, English family, style and id.
pub fn matches_query(face: &Face, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    [face.display_name, face.family, face.style, face.id].into_iter().any(|part| part.to_lowercase().contains(&query))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleError {
    /// No catalogue entry has this id.
    Unknown,
    /// This face has no bundled program, so a preview would be some other typeface.
    NoOutlines,
}

/// A transparent RGBA preview. `skipped` are characters this face does not contain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub skipped: String,
}

/// Draw `text` with this face's own outlines. Missing characters are listed and not borrowed
/// from another face. `px_per_em` is the em size in pixels.
pub fn render_sample(id: &str, text: &str, px_per_em: f32) -> Result<SampleImage, SampleError> {
    let face = get(id).ok_or(SampleError::Unknown)?;
    let bytes = face.program().ok_or(SampleError::NoOutlines)?;
    let font = FontRef::new(bytes).map_err(|_| SampleError::NoOutlines)?;
    let px = if px_per_em.is_finite() { px_per_em.clamp(8.0, 48.0) } else { 18.0 };
    let loc = LocationRef::default();
    let metrics = font.metrics(Size::unscaled(), loc);
    let upem = metrics.units_per_em.max(1) as f64;
    let scale = 1.0 / upem;
    let ascent = metrics.ascent as f64 * scale;
    let descent = metrics.descent as f64 * scale;
    if !ascent.is_finite() || !descent.is_finite() || ascent <= descent {
        return Err(SampleError::NoOutlines);
    }
    let mut skipped = String::new();
    let mut laid = Laid::default();
    let charmap = font.charmap();
    let glyphs = font.outline_glyphs();
    let advances = font.glyph_metrics(Size::unscaled(), loc);
    for ch in text.chars().filter(|c| !c.is_control()).take(SAMPLE_CHARS) {
        let Some(gid) = charmap.map(ch).filter(|gid| gid.to_u32() != 0) else {
            skipped.push(ch);
            continue;
        };
        let advance = advances.advance_width(gid).unwrap_or(0.0) as f64 * scale;
        if !(0.0..=4.0).contains(&advance) {
            skipped.push(ch);
            continue;
        }
        if let Some(glyph) = glyphs.get(gid) {
            let mut pen = Flatten::new(scale, laid.pen);
            let _ = glyph.draw(DrawSettings::unhinted(Size::unscaled(), loc), &mut pen);
            pen.close();
            if pen.too_complex {
                return Err(SampleError::NoOutlines);
            }
            laid.contours.extend(pen.contours);
            laid.pen += advance;
        } else {
            laid.pen += advance;
        }
    }
    if laid.pen <= 0.0 && laid.contours.is_empty() {
        return Ok(SampleImage { width: 0, height: 0, rgba: Vec::new(), skipped });
    }
    let em = f64::from(px);
    let mut width_px = (laid.pen * em).ceil() + 8.0;
    let mut used = em;
    if width_px > f64::from(MAX_SIDE) && laid.pen > 0.0 {
        used = (f64::from(MAX_SIDE) - 8.0) / laid.pen;
        width_px = f64::from(MAX_SIDE);
    }
    let height_px = ((ascent - descent) * used).ceil() + 8.0;
    let width = finite_dim(width_px);
    let height = finite_dim(height_px);
    if width < 4 || height < 4 {
        return Ok(SampleImage { width: 0, height: 0, rgba: Vec::new(), skipped });
    }
    let bytes_len = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if bytes_len == 0 || bytes_len > 1_000_000 {
        return Err(SampleError::NoOutlines);
    }
    let mut rgba = vec![0u8; bytes_len];
    let baseline = 4.0 + ascent * used;
    paint(&mut rgba, width, height, &laid.contours, laid.pen, used, baseline);
    Ok(SampleImage { width, height, rgba, skipped })
}

fn finite_dim(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= f64::from(MAX_SIDE) { MAX_SIDE } else { value as u32 }
}

#[derive(Default)]
struct Laid {
    contours: Vec<Vec<[f64; 2]>>,
    pen: f64,
}

struct Flatten {
    contours: Vec<Vec<[f64; 2]>>,
    cur: Vec<[f64; 2]>,
    scale: f64,
    dx: f64,
    points: usize,
    too_complex: bool,
}

impl Flatten {
    fn new(scale: f64, dx: f64) -> Self {
        Self { contours: Vec::new(), cur: Vec::new(), scale, dx, points: 0, too_complex: false }
    }

    fn pt(&self, x: f32, y: f32) -> [f64; 2] {
        [self.dx + f64::from(x) * self.scale, f64::from(y) * self.scale]
    }

    fn last(&self) -> [f64; 2] {
        self.cur.last().copied().unwrap_or([self.dx, 0.0])
    }

    fn push(&mut self, p: [f64; 2]) {
        if self.points >= 16384 {
            self.too_complex = true;
            return;
        }
        self.points += 1;
        self.cur.push(p);
    }

    fn close(&mut self) {
        if self.cur.len() > 2 {
            self.contours.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }
}

const STEPS: usize = 8;

impl OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.push(self.pt(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(self.pt(x, y));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (p0, c, p) = (self.last(), self.pt(cx, cy), self.pt(x, y));
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1.0 - t;
            self.push([u * u * p0[0] + 2.0 * u * t * c[0] + t * t * p[0], u * u * p0[1] + 2.0 * u * t * c[1] + t * t * p[1]]);
        }
    }

    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (p0, c0, c1, p) = (self.last(), self.pt(c0x, c0y), self.pt(c1x, c1y), self.pt(x, y));
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.push([a * p0[0] + b * c0[0] + c * c1[0] + d * p[0], a * p0[1] + b * c0[1] + c * c1[1] + d * p[1]]);
        }
    }

    fn close(&mut self) {
        Flatten::close(self);
    }
}

fn paint(buf: &mut [u8], width: u32, height: u32, contours: &[Vec<[f64; 2]>], _advance: f64, em: f64, baseline: f64) {
    if contours.is_empty() {
        return;
    }
    let mut placed = Vec::new();
    let mut min = [f64::MAX, f64::MAX];
    let mut max = [f64::MIN, f64::MIN];
    for contour in contours {
        let mut pts = Vec::new();
        for p in contour {
            let at = [4.0 + p[0] * em, baseline - p[1] * em];
            if !at[0].is_finite() || !at[1].is_finite() {
                return;
            }
            min[0] = min[0].min(at[0]);
            min[1] = min[1].min(at[1]);
            max[0] = max[0].max(at[0]);
            max[1] = max[1].max(at[1]);
            pts.push(at);
        }
        if pts.len() >= 3 {
            placed.push(pts);
        }
    }
    let x0 = px_floor(min[0] - 1.0, width);
    let y0 = px_floor(min[1] - 1.0, height);
    let x1 = px_ceil(max[0] + 1.0, width);
    let y1 = px_ceil(max[1] + 1.0, height);
    for y in y0..y1 {
        for x in x0..x1 {
            let mut hit = 0u8;
            for sy in [0.25, 0.75] {
                for sx in [0.25, 0.75] {
                    if winding(&placed, f64::from(x) + sx, f64::from(y) + sy) {
                        hit = hit.saturating_add(1);
                    }
                }
            }
            if hit == 0 {
                continue;
            }
            let i = (y as usize).saturating_mul(width as usize).saturating_add(x as usize).saturating_mul(4);
            let Some(px) = buf.get_mut(i..i + 4) else { continue };
            let alpha = hit.saturating_mul(63).saturating_add(if hit == 4 { 3 } else { 0 });
            px[0] = 32;
            px[1] = 32;
            px[2] = 32;
            px[3] = alpha;
        }
    }
}

fn px_floor(value: f64, limit: u32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= f64::from(limit) { limit } else { value as u32 }
}

fn px_ceil(value: f64, limit: u32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let up = if value.fract() > 0.0 { value.floor() + 1.0 } else { value };
    if up >= f64::from(limit) { limit } else { up as u32 }
}

fn winding(contours: &[Vec<[f64; 2]>], x: f64, y: f64) -> bool {
    let mut wind = 0i32;
    for contour in contours {
        let n = contour.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let Some(a) = contour.get(i) else { continue };
            let Some(b) = contour.get((i + 1) % n) else { continue };
            let crosses = (a[1] <= y && b[1] > y) || (b[1] <= y && a[1] > y);
            if !crosses {
                continue;
            }
            let dy = b[1] - a[1];
            if dy == 0.0 {
                continue;
            }
            let xint = a[0] + (y - a[1]) * (b[0] - a[0]) / dy;
            if x < xint {
                wind += if b[1] > a[1] { 1 } else { -1 };
            }
        }
    }
    wind != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_letters_are_kana_and_kanji_not_punctuation() {
        assert!(is_japanese_letter('あ') && is_japanese_letter('ン') && is_japanese_letter('東'));
        assert!(is_japanese_letter('\u{FF71}'));
        assert!(!is_japanese_letter('A') && !is_japanese_letter('1') && !is_japanese_letter('、'));
        assert!(!is_japanese_letter('Ａ'));
        let helvetica = get("standard:helvetica:regular").unwrap();
        assert_eq!(helvetica.supports_japanese(), None, "no program is not evidence of missing Japanese");
    }

    #[test]
    fn ids_are_unique_and_partners_stay_in_the_family() {
        let mut ids: Vec<&str> = faces().iter().map(|f| f.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), faces().len());
        assert_eq!(bold_partner("craft:biz-udp-gothic:regular"), Some("craft:biz-udp-gothic:bold"));
        assert_eq!(regular_partner("craft:biz-udp-gothic:bold"), Some("craft:biz-udp-gothic:regular"));
        assert_eq!(bold_partner("standard:times:regular"), Some("standard:times:bold"));
        assert_eq!(bold_partner("bundled:inter:regular"), None, "medium and semibold are not a bold face");
        assert!(get("not-a-font").is_none());
        assert!(get("bundled:dancing-script:regular").is_none(), "the shipped Dancing Script file is a variable font");
        assert!(matches!(render_sample("not-a-font", "A", 18.0), Err(SampleError::Unknown)));
        assert!(matches!(render_sample("standard:helvetica:regular", "A", 18.0), Err(SampleError::NoOutlines)));
    }

    #[test]
    fn standard_faces_are_named_for_pdf_and_are_not_previewed() {
        for (id, base) in [
            ("standard:helvetica:regular", "Helvetica"),
            ("standard:helvetica:bold", "Helvetica-Bold"),
            ("standard:times:regular", "Times-Roman"),
            ("standard:times:bold", "Times-Bold"),
            ("standard:courier:regular", "Courier"),
            ("standard:courier:bold", "Courier-Bold"),
        ] {
            let face = get(id).unwrap();
            assert!(face.available());
            assert!(!face.can_preview());
            assert!(!face.japanese_sample());
            assert_eq!(face.pdf, PdfMethod::Standard14Name(base));
        }
    }

    #[test]
    fn bundled_faces_preview_their_own_glyphs_and_may_be_embedded() {
        for id in ["bundled:inter:regular", "bundled:inter:medium", "bundled:inter:semibold", "bundled:jetbrains-mono:regular"] {
            let face = get(id).unwrap();
            assert!(face.available(), "{id} is not an embeddable OFL face");
            assert!(face.can_preview());
            assert!(!face.japanese_sample(), "{id} must not claim Japanese coverage");
            assert_eq!(face.supports_japanese(), Some(false), "{id} has no Japanese letters");
            assert_eq!(face.pdf, PdfMethod::EmbedProgram);
        }
        let inter = render_sample("bundled:inter:regular", DEFAULT_SAMPLE, 20.0).unwrap();
        assert!(inter.skipped.contains('東'));
        assert!(inter.rgba.iter().any(|b| *b > 0), "the digits are still drawn");
        let mono = render_sample("bundled:jetbrains-mono:regular", "2026", 22.0).unwrap();
        let inter_digits = render_sample("bundled:inter:regular", "2026", 22.0).unwrap();
        assert_ne!(inter_digits.rgba, mono.rgba);
        let again = render_sample("bundled:inter:regular", "2026", 22.0).unwrap();
        assert_eq!(inter_digits, again);
        assert!(render_sample("bundled:inter:regular", "", 20.0).unwrap().rgba.is_empty());
    }

    #[test]
    fn japanese_faces_differ_by_family_and_weight_when_this_build_has_them() {
        let gothic = get("craft:biz-udp-gothic:regular").unwrap();
        let bold = get("craft:biz-udp-gothic:bold").unwrap();
        let mincho = get("craft:shippori-mincho:regular").unwrap();
        let ud = get("craft:biz-ud-mincho:regular").unwrap();
        if gothic.program().is_none() {
            assert!(!gothic.available());
            assert!(!bold.available());
            assert!(!mincho.available());
            assert!(!ud.available());
            return;
        }
        assert!(gothic.available() && bold.available() && mincho.available() && ud.available());
        assert!(gothic.japanese_sample() && bold.japanese_sample());
        assert_eq!(gothic.supports_japanese(), Some(true));
        assert_eq!(bold.supports_japanese(), Some(true));
        assert_eq!(mincho.supports_japanese(), Some(true));
        assert_eq!(ud.supports_japanese(), Some(true));
        assert_ne!(gothic.program(), bold.program());
        let regular = render_sample(gothic.id, "東京", 24.0).unwrap();
        let heavy = render_sample(bold.id, "東京", 24.0).unwrap();
        let serif = render_sample(mincho.id, "東京", 24.0).unwrap();
        assert!(regular.skipped.is_empty() && heavy.skipped.is_empty());
        assert_ne!(regular.rgba, heavy.rgba);
        assert_ne!(regular.rgba, serif.rgba);
        assert!(regular.rgba.iter().any(|b| (1..255).contains(b)), "outline edges are antialiased");
        assert_eq!(regular.rgba[3], 0, "the corner stays clear");
    }

    #[test]
    fn a_query_matches_japanese_and_english_names() {
        assert!(matches_query(get("craft:shippori-mincho:regular").unwrap(), "明朝"));
        assert!(matches_query(get("craft:shippori-mincho:regular").unwrap(), "shippori"));
        assert!(!matches_query(get("craft:biz-udp-gothic:regular").unwrap(), "明朝"));
        assert!(matches_query(get("standard:helvetica:bold").unwrap(), "BOLD"));
        assert!(matches_query(get("bundled:inter:regular").unwrap(), "  "));
    }

    #[test]
    fn a_date_stamp_offer_keeps_outlines_and_the_row_text() {
        let raster = Offer { raster: true, text: String::new(), standard: false };
        assert!(!face_offered(get("standard:helvetica:regular").unwrap(), &raster));
        assert!(face_offered(get("bundled:inter:regular").unwrap(), &raster));
        let japanese = Offer { raster: true, text: "東京".into(), standard: false };
        assert!(!face_offered(get("bundled:inter:regular").unwrap(), &japanese));
        assert!(missing_glyphs(get("bundled:inter:regular").unwrap(), "東京").contains('東'));
        let date = Offer { raster: true, text: "2026/10/20".into(), standard: false };
        assert!(face_offered(get("bundled:inter:semibold").unwrap(), &date));
        assert!(face_offered(get("bundled:jetbrains-mono:regular").unwrap(), &date));
        let gothic = get("craft:biz-udp-gothic:regular").unwrap();
        if gothic.program().is_some() {
            assert!(face_offered(gothic, &japanese));
            assert!(face_offered(get("craft:shippori-mincho:regular").unwrap(), &japanese));
            assert!(face_offered(get("craft:biz-ud-mincho:regular").unwrap(), &japanese));
        }
    }

    #[test]
    fn added_text_keeps_standard_names_when_the_line_is_japanese() {
        let added = Offer { raster: false, text: "東京A".into(), standard: true };
        assert!(face_offered(get("standard:helvetica:regular").unwrap(), &added));
        assert!(face_offered(get("standard:times:bold").unwrap(), &added));
        assert!(face_offered(get("standard:courier:regular").unwrap(), &added));
        assert!(!face_offered(get("bundled:inter:regular").unwrap(), &added));
        assert!(!face_offered(get("bundled:jetbrains-mono:regular").unwrap(), &added));
        let latin = Offer { raster: false, text: "ABC".into(), standard: true };
        assert!(face_offered(get("bundled:inter:regular").unwrap(), &latin));
        assert!(face_offered(get("bundled:jetbrains-mono:regular").unwrap(), &latin));
        assert!(face_offered(get("standard:helvetica:regular").unwrap(), &Offer::default()));
    }
}
