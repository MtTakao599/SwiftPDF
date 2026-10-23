//! Per-stamp placement scale and a date stamp whose date is drawn into a copy of the base image.
//!
//! 100% is the size [`nominal_size`] already used for a click: pixel width and height treated as
//! points, long side at most 200. A percent multiplies that box from its centre and keeps the
//! aspect ratio. The date is rasterized with Inter, which is embedded, so a saved PDF does not
//! need the font again. The stored base image is never modified.

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

pub(crate) const SCALE_MIN: f64 = 25.0;
pub(crate) const SCALE_MAX: f64 = 400.0;

pub(crate) fn default_scale() -> f64 {
    100.0
}

pub(crate) fn default_region() -> [f64; 4] {
    [0.15, 0.35, 0.70, 0.30]
}

pub(crate) fn default_font() -> f64 {
    0.18
}

pub(crate) fn default_color() -> [u8; 3] {
    [196, 30, 58]
}

/// `Some` when `percent` is finite and inside 25–400. Anything else is refused, not clamped,
/// so a bad value is not silently stored.
pub(crate) fn checked_scale(percent: f64) -> Option<f64> {
    if percent.is_finite() && (SCALE_MIN..=SCALE_MAX).contains(&percent) { Some(percent) } else { None }
}

/// A missing or out-of-range saved percent is 100%. Callers use this when reading old data.
/// It does not write the file.
pub(crate) fn scale_or_hundred(percent: f64) -> f64 {
    checked_scale(percent).unwrap_or(100.0)
}

/// Width and height, in PDF points, of a stamp at 100%.
///
/// `pixels` are the image's pixel size. They are used as points, then the long side is capped
/// at 200. A missing image is 120×120. Rotation 90 or 270 swaps the two sides. This is the size
/// a click used before a percent existed.
pub(crate) fn nominal_size(rotation: u16, pixels: Option<(u32, u32)>) -> (f64, f64) {
    let (sw, sh) = pixels.map(|(w, h)| (w as f64, h as f64)).unwrap_or((120.0, 120.0));
    let (sw, sh) = (sw.max(1.0), sh.max(1.0));
    let fit = (200.0 / sw.max(sh)).min(1.0);
    if matches!(rotation, 90 | 270) { (sh * fit, sw * fit) } else { (sw * fit, sh * fit) }
}

/// A rectangle centred on `at` at `percent` of `nominal` (width, height).
pub(crate) fn place_rect(at: [f64; 2], nominal: (f64, f64), percent: f64) -> [f64; 4] {
    let k = if percent.is_finite() { percent / 100.0 } else { 1.0 };
    let (w, h) = (nominal.0 * k, nominal.1 * k);
    [at[0] - w / 2.0, at[1] - h / 2.0, at[0] + w / 2.0, at[1] + h / 2.0]
}

/// The same centre as `rect`, at `percent` of the 100% size.
pub(crate) fn rect_at_scale(rect: [f64; 4], nominal: (f64, f64), percent: f64) -> [f64; 4] {
    let center = [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0];
    place_rect(center, nominal, percent)
}

/// Percent of `nominal_width` that `rect`'s width represents. A zero nominal width stays 100%.
pub(crate) fn percent_of(rect: [f64; 4], nominal_width: f64) -> f64 {
    let width = (rect[2] - rect[0]).abs();
    if nominal_width <= 0.0 || !nominal_width.is_finite() || !width.is_finite() {
        return 100.0;
    }
    width / nominal_width * 100.0
}

/// Real size on the page. A near-square image with transparent corners is a diameter; anything
/// else is width × height. 1 pt = 25.4/72 mm.
pub(crate) fn size_label(rect: [f64; 4], circular: bool) -> String {
    let w = (rect[2] - rect[0]).abs() * 25.4 / 72.0;
    let h = (rect[3] - rect[1]).abs() * 25.4 / 72.0;
    if !w.is_finite() || !h.is_finite() {
        return String::new();
    }
    if circular { format!("直径 {w:.1} mm") } else { format!("{w:.1} × {h:.1} mm") }
}

/// Square image, transparent corners, opaque centre. Used only for the millimetre label.
pub(crate) fn looks_circular(width: u32, height: u32, rgba: &[u8]) -> bool {
    if width < 8 || height < 8 {
        return false;
    }
    let aspect = width as f64 / height as f64;
    if !aspect.is_finite() || !(0.92..=1.08).contains(&aspect) {
        return false;
    }
    let alpha = |x: u32, y: u32| -> u8 {
        let i = (y as usize).saturating_mul(width as usize).saturating_add(x as usize).saturating_mul(4).saturating_add(3);
        rgba.get(i).copied().unwrap_or(255)
    };
    let corner = alpha(0, 0) < 16 && alpha(width - 1, 0) < 16 && alpha(0, height - 1) < 16 && alpha(width - 1, height - 1) < 16;
    corner && alpha(width / 2, height / 2) > 200
}

/// A calendar day. Month and day are not zero-padded when formatted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StampDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl StampDate {
    pub(crate) fn format(self) -> String {
        format!("{}/{}/{}", self.year, self.month, self.day)
    }
}

/// `YYYY/M/D`. Month and day may be written with or without a leading zero. Impossible days,
/// including 2/29 outside a leap year, are `None`.
pub(crate) fn parse_date(text: &str) -> Option<StampDate> {
    let parts: Vec<&str> = text.trim().split('/').collect();
    let [year, month, day] = parts.as_slice() else { return None };
    if year.is_empty() || month.is_empty() || day.is_empty() {
        return None;
    }
    if !year.chars().all(|c| c.is_ascii_digit()) || !month.chars().all(|c| c.is_ascii_digit()) || !day.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let year: i32 = year.parse().ok()?;
    let month: u32 = month.parse().ok()?;
    let day: u32 = day.parse().ok()?;
    if !(1900..=9999).contains(&year) || month == 0 || month > 12 || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(StampDate { year, month, day })
}

pub(crate) fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub(crate) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Japan has no daylight-saving time. The date is UTC+9, not the machine's time zone.
pub(crate) fn japan_date_at(unix_secs: u64) -> StampDate {
    let shifted = unix_secs.saturating_add(9 * 3_600);
    let days = (shifted / 86_400) as i64;
    let (year, month, day) = crate::seal_store::civil_from_days(days);
    StampDate { year, month, day }
}

pub(crate) fn japan_today() -> StampDate {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|dur| dur.as_secs()).unwrap_or(0);
    japan_date_at(secs)
}

/// Sunday = 0. Used by the calendar grid.
pub(crate) fn weekday(date: StampDate) -> u32 {
    let days = days_from_civil(date.year, date.month, date.day);
    (days + 4).rem_euclid(7) as u32
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { u64::from(month) - 3 } else { u64::from(month) + 9 };
    let doy = (153 * mp + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// Fractions of the image, origin at the top left: x, y, width, height. Each side stays inside
/// the image and is at least 5%.
pub(crate) fn clamp_region(region: [f64; 4]) -> [f64; 4] {
    if region.iter().any(|v| !v.is_finite()) {
        return default_region();
    }
    let mut x = region[0].clamp(0.0, 0.95);
    let mut y = region[1].clamp(0.0, 0.95);
    let mut w = region[2].clamp(0.05, 1.0);
    let mut h = region[3].clamp(0.05, 1.0);
    if x + w > 1.0 {
        w = (1.0 - x).max(0.05);
        if x + w > 1.0 {
            x = 0.95;
            w = 0.05;
        }
    }
    if y + h > 1.0 {
        h = (1.0 - y).max(0.05);
        if y + h > 1.0 {
            y = 0.95;
            h = 0.05;
        }
    }
    [x, y, w, h]
}

pub(crate) fn clamp_font(font: f64) -> f64 {
    if font.is_finite() { font.clamp(0.05, 0.50) } else { default_font() }
}

/// The date drawn onto a placement. `base` is the original PNG and is not modified.
/// `drawn` replaces the PNG: the circle, the two captions and the date are painted instead.
#[derive(Clone, Debug)]
pub struct DateOverlay {
    pub date: StampDate,
    /// The text field, including a half-typed value. A valid value matches [`Self::date`].
    pub entry: String,
    pub region: [f64; 4],
    pub font: f64,
    pub color: [u8; 3],
    pub base: Vec<u8>,
    pub drawn: Option<DrawnDate>,
    pub show_calendar: bool,
    pub calendar_year: i32,
    pub calendar_month: u32,
}

pub(crate) const TOP_PRESETS: &[&str] = &["東京テラス", "済", "受領", "確認", "受付", "承認"];
pub(crate) const BOTTOM_PRESETS: &[&str] = &["管理センター", "東京テラス管理組合", "理事会", "管理事務室", ""];

/// Pixel size used for the 100% placement box of a drawn stamp. The baked image may be finer.
pub(crate) const DESIGN_SIDE: u32 = 512;
const PREVIEW_SIDE: u32 = 512;

fn default_top() -> String {
    "済".into()
}

fn default_top_size() -> f64 {
    0.10
}

fn default_top_y() -> f64 {
    0.28
}

fn default_bottom_size() -> f64 {
    0.075
}

fn default_bottom_y() -> f64 {
    0.74
}

fn default_date_size() -> f64 {
    0.16
}

fn default_date_bold() -> bool {
    true
}

fn default_date_y() -> f64 {
    0.50
}

fn default_diameter() -> f64 {
    0.94
}

fn default_ring() -> f64 {
    0.028
}

fn default_rules() -> bool {
    true
}

/// A date stamp drawn from text, not from a template image. Sizes are fractions of the square.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DrawnDate {
    #[serde(default = "default_top")]
    pub top: String,
    #[serde(default = "default_top_size")]
    pub top_size: f64,
    #[serde(default)]
    pub top_bold: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_font: Option<String>,
    #[serde(default = "default_top_y")]
    pub top_y: f64,
    #[serde(default)]
    pub bottom: String,
    #[serde(default = "default_bottom_size")]
    pub bottom_size: f64,
    #[serde(default)]
    pub bottom_bold: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom_font: Option<String>,
    #[serde(default = "default_bottom_y")]
    pub bottom_y: f64,
    #[serde(default = "default_date_size")]
    pub date_size: f64,
    #[serde(default = "default_date_bold")]
    pub date_bold: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_font: Option<String>,
    #[serde(default = "default_date_y")]
    pub date_y: f64,
    #[serde(default = "default_diameter")]
    pub diameter: f64,
    #[serde(default = "default_ring")]
    pub ring: f64,
    #[serde(default = "default_rules")]
    pub rules: bool,
}

impl Default for DrawnDate {
    fn default() -> Self {
        Self {
            top: default_top(),
            top_size: default_top_size(),
            top_bold: false,
            top_font: None,
            top_y: default_top_y(),
            bottom: String::new(),
            bottom_size: default_bottom_size(),
            bottom_bold: false,
            bottom_font: None,
            bottom_y: default_bottom_y(),
            date_size: default_date_size(),
            date_bold: default_date_bold(),
            date_font: None,
            date_y: default_date_y(),
            diameter: default_diameter(),
            ring: default_ring(),
            rules: default_rules(),
        }
    }
}

impl DrawnDate {
    pub(crate) fn clamp(&mut self) {
        self.top = clean_line(&self.top);
        self.bottom = clean_line(&self.bottom);
        self.top_size = finite_unit(self.top_size, 0.05, 0.28, default_top_size());
        self.bottom_size = finite_unit(self.bottom_size, 0.05, 0.28, default_bottom_size());
        self.date_size = finite_unit(self.date_size, 0.08, 0.30, default_date_size());
        self.top_y = finite_unit(self.top_y, 0.16, 0.44, default_top_y());
        self.date_y = finite_unit(self.date_y, 0.38, 0.64, default_date_y());
        self.bottom_y = finite_unit(self.bottom_y, 0.58, 0.88, default_bottom_y());
        self.diameter = finite_unit(self.diameter, 0.70, 0.98, default_diameter());
        self.ring = finite_unit(self.ring, 0.012, 0.08, default_ring());
        clean_font_id(&mut self.top_font);
        clean_font_id(&mut self.date_font);
        clean_font_id(&mut self.bottom_font);
    }
}

fn finite_unit(value: f64, lo: f64, hi: f64, fallback: f64) -> f64 {
    if value.is_finite() { value.clamp(lo, hi) } else { fallback }
}

fn clean_line(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(24).collect()
}

fn clean_font_id(id: &mut Option<String>) {
    let Some(value) = id.as_deref().map(str::to_string) else { return };
    let cleaned: String = value.chars().filter(|c| !c.is_control()).take(128).collect();
    if cleaned.is_empty() {
        *id = None;
    } else if cleaned != value {
        *id = Some(cleaned);
    }
}

/// A date stamp in the settings file. It is an ordinary image stamp: no approval, no DPAPI,
/// no seal history. `data` is the base image of an older PNG stamp. `drawn` is the newer
/// circle-and-text stamp; loading one does not convert the other.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DateStamp {
    pub name: String,
    pub file: String,
    #[serde(with = "b64")]
    pub data: std::sync::Arc<Vec<u8>>,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default = "default_region")]
    pub region: [f64; 4],
    #[serde(default = "default_font")]
    pub font: f64,
    #[serde(default = "default_color")]
    pub color: [u8; 3],
    #[serde(default)]
    pub drawn: Option<DrawnDate>,
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

const MAX_DATE_STAMPS: usize = 32;

pub(crate) fn encode_dates(stamps: &[DateStamp]) -> serde_json::Value {
    serde_json::to_value(stamps).unwrap_or(serde_json::Value::Array(Vec::new()))
}

pub(crate) fn decode_dates(v: &serde_json::Value) -> Vec<DateStamp> {
    v.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<DateStamp>(item.clone()).ok())
                .filter(|stamp| {
                    !stamp.name.trim().is_empty()
                        && stamp.data.len() <= crate::stamps_ui::MAX_STAMP_BYTES
                        && (stamp.drawn.is_some() || !stamp.data.is_empty())
                })
                .take(MAX_DATE_STAMPS)
                .map(|mut stamp| {
                    stamp.scale = scale_or_hundred(stamp.scale);
                    stamp.region = clamp_region(stamp.region);
                    stamp.font = clamp_font(stamp.font);
                    if let Some(drawn) = stamp.drawn.as_mut() {
                        drawn.clamp();
                    }
                    stamp
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn normalize_stamp(stamp: &mut DateStamp) {
    stamp.scale = scale_or_hundred(stamp.scale);
    stamp.region = clamp_region(stamp.region);
    stamp.font = clamp_font(stamp.font);
    if let Some(drawn) = stamp.drawn.as_mut() {
        drawn.clamp();
    }
}

/// Draw `overlay.date` onto a copy of `base`, or paint a drawn stamp at preview size.
pub(crate) fn render_overlay(base: &[u8], overlay: &DateOverlay) -> Option<(u32, u32, Vec<u8>)> {
    if overlay.drawn.is_some() {
        return render_drawn_stamp(overlay, PREVIEW_SIDE);
    }
    let (width, height, rgba) = crate::seal_store::preview_rgba(base)?;
    let drawn = composite_rgba(&rgba, width, height, overlay.date, overlay.region, overlay.font, overlay.color)?;
    Some((width, height, drawn))
}

/// The image placed on the page. A drawn stamp uses a resolution that follows `rect`.
pub(crate) fn render_placed(overlay: &DateOverlay, rect: [f64; 4]) -> Option<(u32, u32, Vec<u8>)> {
    if overlay.drawn.is_some() { render_drawn_stamp(overlay, pixels_for_rect(rect)) } else { render_overlay(&overlay.base, overlay) }
}

/// Side length, in pixels, for a placed drawn stamp. About five pixels per point, capped.
pub(crate) fn pixels_for_rect(rect: [f64; 4]) -> u32 {
    let side = (rect[2] - rect[0]).abs().max((rect[3] - rect[1]).abs());
    if !side.is_finite() || side <= 0.0 {
        return DESIGN_SIDE;
    }
    let px = side * 5.0;
    if !px.is_finite() {
        return DESIGN_SIDE;
    }
    let n = if px <= 384.0 {
        384.0
    } else if px >= 1024.0 {
        1024.0
    } else {
        px.round()
    };
    n as u32
}

/// Encode straight RGBA as a PNG. `None` when the buffer length does not match.
pub(crate) fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let image = image::RgbaImage::from_raw(width, height, rgba.to_vec())?;
    let mut out = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}

/// Paint the date into a new buffer. `base` is left unchanged by the caller; this function
/// only reads it. Ink is clipped to `region`. Transparent pixels outside the glyphs stay as
/// they were, including fully transparent ones.
pub(crate) fn composite_rgba(base: &[u8], width: u32, height: u32, date: StampDate, region: [f64; 4], font: f64, color: [u8; 3]) -> Option<Vec<u8>> {
    let expected = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if expected == 0 || base.len() != expected || width > 8000 || height > 8000 {
        return None;
    }
    let region = clamp_region(region);
    let font = clamp_font(font);
    let text = date.format();
    if text.chars().count() > 16 {
        return None;
    }
    let layout = layout_text(&text)?;
    let (rx, ry, rw, rh) = (region[0] * width as f64, region[1] * height as f64, region[2] * width as f64, region[3] * height as f64);
    if rw < 2.0 || rh < 2.0 {
        return None;
    }
    let mut font_px = (font * height as f64).clamp(4.0, height as f64);
    for _ in 0..16 {
        let text_w = layout.width * font_px;
        let text_h = (layout.ascent - layout.descent) * font_px;
        if text_w <= rw * 0.96 && text_h <= rh * 0.92 {
            break;
        }
        font_px *= 0.82;
        if font_px < 4.0 {
            font_px = 4.0;
            break;
        }
    }
    let text_w = layout.width * font_px;
    let text_h = (layout.ascent - layout.descent) * font_px;
    let left = rx + (rw - text_w) / 2.0;
    let baseline = ry + (rh - text_h) / 2.0 + layout.ascent * font_px;
    let mut contours = Vec::new();
    let mut min = [f64::MAX, f64::MAX];
    let mut max = [f64::MIN, f64::MIN];
    for contour in &layout.contours {
        let mut pts = Vec::with_capacity(contour.len());
        for p in contour {
            let at = [left + p[0] * font_px, baseline - p[1] * font_px];
            if !at[0].is_finite() || !at[1].is_finite() {
                return None;
            }
            min[0] = min[0].min(at[0]);
            min[1] = min[1].min(at[1]);
            max[0] = max[0].max(at[0]);
            max[1] = max[1].max(at[1]);
            pts.push(at);
        }
        if pts.len() >= 3 {
            contours.push(pts);
        }
    }
    let mut out = base.to_vec();
    if contours.is_empty() {
        return Some(out);
    }
    let x0 = finite_px(min[0].max(rx).floor(), width);
    let y0 = finite_px(min[1].max(ry).floor(), height);
    let x1 = finite_px(max[0].min(rx + rw).ceil(), width);
    let y1 = finite_px(max[1].min(ry + rh).ceil(), height);
    for y in y0..y1 {
        for x in x0..x1 {
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            if cx < rx || cy < ry || cx >= rx + rw || cy >= ry + rh {
                continue;
            }
            if !winding(&contours, cx, cy) {
                continue;
            }
            let i = (y as usize).saturating_mul(width as usize).saturating_add(x as usize).saturating_mul(4);
            let Some(px) = out.get_mut(i..i + 4) else { continue };
            px[0] = color[0];
            px[1] = color[1];
            px[2] = color[2];
            px[3] = 255;
        }
    }
    Some(out)
}

/// A non-negative finite coordinate, capped at `limit`. Negative and non-finite values are 0.
fn finite_px(value: f64, limit: u32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= f64::from(limit) { limit } else { value as u32 }
}

fn winding(contours: &[Vec<[f64; 2]>], x: f64, y: f64) -> bool {
    let mut wind = 0i32;
    for contour in contours {
        let n = contour.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let a = contour[i];
            let b = contour[(i + 1) % n];
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

struct TextLayout {
    contours: Vec<Vec<[f64; 2]>>,
    width: f64,
    ascent: f64,
    descent: f64,
}

fn layout_text(text: &str) -> Option<TextLayout> {
    layout_font(pdfcraft_fonts::INTER_REGULAR, text, false)
}

/// Outlines in em units. `faux_bold` draws a second copy, shifted, when the face has no bold.
fn layout_font(bytes: &[u8], text: &str, faux_bold: bool) -> Option<TextLayout> {
    let font = FontRef::new(bytes).ok()?;
    let loc = LocationRef::default();
    let metrics = font.metrics(Size::unscaled(), loc);
    let scale = 1.0 / metrics.units_per_em.max(1) as f64;
    let charmap = font.charmap();
    let glyphs = font.outline_glyphs();
    let advances = font.glyph_metrics(Size::unscaled(), loc);
    let mut pen = Flatten::new(scale);
    for ch in text.chars().take(24) {
        let gid = charmap.map(ch).filter(|gid| gid.to_u32() != 0)?;
        if let Some(glyph) = glyphs.get(gid) {
            let _ = glyph.draw(DrawSettings::unhinted(Size::unscaled(), loc), &mut pen);
            pen.close();
        }
        let advance = advances.advance_width(gid).unwrap_or(0.0) as f64 * scale;
        if !advance.is_finite() || advance < 0.0 {
            return None;
        }
        pen.dx += advance;
    }
    pen.close();
    if pen.too_complex || pen.dx <= 0.0 {
        return None;
    }
    let ascent = metrics.ascent as f64 * scale;
    let descent = metrics.descent as f64 * scale;
    if !ascent.is_finite() || !descent.is_finite() || ascent <= descent {
        return None;
    }
    let mut contours = pen.contours;
    if faux_bold {
        let shift = 0.02;
        let extra: Vec<Vec<[f64; 2]>> = contours.iter().map(|c| c.iter().map(|p| [p[0] + shift, p[1]]).collect()).collect();
        contours.extend(extra);
    }
    Some(TextLayout { contours, width: pen.dx + if faux_bold { 0.02 } else { 0.0 }, ascent, descent })
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
    fn new(scale: f64) -> Self {
        Self { contours: Vec::new(), cur: Vec::new(), scale, dx: 0.0, points: 0, too_complex: false }
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

/// Paint the circle, captions and date into a transparent square. `None` when a line cannot be
/// shaped, including when the Japanese face is not in this build.
pub(crate) fn render_drawn_stamp(overlay: &DateOverlay, side: u32) -> Option<(u32, u32, Vec<u8>)> {
    let design = overlay.drawn.as_ref()?;
    let mut design = design.clone();
    design.clamp();
    let side = side.clamp(64, 1024);
    let bytes = (side as usize).saturating_mul(side as usize).saturating_mul(4);
    if bytes == 0 || bytes > 16_000_000 {
        return None;
    }
    let mut buf = vec![0u8; bytes];
    let side_f = f64::from(side);
    let (cx, cy) = (side_f * 0.5, side_f * 0.5);
    let outer = (design.diameter * side_f * 0.5).max(8.0);
    let stroke = (design.ring * side_f).clamp(1.5, outer * 0.2);
    let inner = (outer - stroke).max(2.0);
    let mark = Mark { side, cx, cy, color: overlay.color };
    paint_ring(&mut buf, side, cx, cy, inner, outer, overlay.color);
    if design.rules {
        let thick = (side_f * 0.008).clamp(1.0, 4.0);
        paint_rule(&mut buf, &mark, inner, design.top_y, design.date_y, thick);
        paint_rule(&mut buf, &mark, inner, design.date_y, design.bottom_y, thick);
    }
    paint_caption(
        &mut buf,
        &mark,
        inner,
        Caption { text: &design.top, bold: design.top_bold, size: design.top_size, y: design.top_y, font: design.top_font.as_deref() },
    )?;
    paint_caption(
        &mut buf,
        &mark,
        inner,
        Caption { text: &overlay.date.format(), bold: design.date_bold, size: design.date_size, y: design.date_y, font: design.date_font.as_deref() },
    )?;
    paint_caption(
        &mut buf,
        &mark,
        inner,
        Caption { text: &design.bottom, bold: design.bottom_bold, size: design.bottom_size, y: design.bottom_y, font: design.bottom_font.as_deref() },
    )?;
    Some((side, side, buf))
}

struct Mark {
    side: u32,
    cx: f64,
    cy: f64,
    color: [u8; 3],
}

fn line_face(bold: bool) -> Option<(&'static [u8], bool)> {
    let face = pdfcraft_fonts::document_japanese_font_for_style(false, bold)?;
    Some((face.bytes, bold && face.style != "Bold"))
}

/// Which row of a drawn date stamp a font window is editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DateFontRow {
    Top,
    Date,
    Bottom,
}

/// How one line will be drawn. A missing glyph is never filled from another face.
pub(crate) enum LineDraw<'a> {
    Draw { bytes: &'a [u8], faux: bool, substituted: bool },
    Missing { chars: String },
    Unreadable,
}

/// A reason to show next to a row. `Missing` blocks registration. The stored id is not changed.
pub(crate) enum LineNote {
    /// The stored id cannot be rasterized, so the picture uses BIZ UDPGothic.
    Substituted { row: &'static str },
    /// The chosen face is real and lacks these characters.
    Missing { row: &'static str, chars: String },
    /// No face in this build can draw the line.
    Unreadable { row: &'static str, stored: bool },
}

struct Caption<'a> {
    text: &'a str,
    bold: bool,
    size: f64,
    y: f64,
    font: Option<&'a str>,
}

/// The id to store when the bold checkbox moves. An absent id stays absent: the checkbox
/// alone still selects Gothic Regular or Gothic Bold. Medium and semibold are not bold.
pub(crate) fn id_for_bold(stored: Option<&str>, bold: bool) -> Option<String> {
    let stored = stored?;
    if bold {
        return Some(pdfcraft_fonts::catalog::bold_partner(stored).unwrap_or(stored).to_string());
    }
    if pdfcraft_fonts::catalog::get(stored).is_some_and(|face| face.weight == 700) {
        return Some(pdfcraft_fonts::catalog::regular_partner(stored).unwrap_or(stored).to_string());
    }
    Some(stored.to_string())
}

fn effective_id(stored: &str, bold: bool) -> &str {
    if bold {
        pdfcraft_fonts::catalog::bold_partner(stored).unwrap_or(stored)
    } else if pdfcraft_fonts::catalog::get(stored).is_some_and(|face| face.weight == 700) {
        pdfcraft_fonts::catalog::regular_partner(stored).unwrap_or(stored)
    } else {
        stored
    }
}

fn covers_bytes(bytes: &[u8], text: &str) -> bool {
    let Ok(font) = FontRef::new(bytes) else { return false };
    let map = font.charmap();
    text.chars().filter(|ch| !ch.is_control() && !ch.is_whitespace()).all(|ch| map.map(ch).is_some_and(|gid| gid.to_u32() != 0))
}

fn resolve_line(stored: Option<&str>, bold: bool, text: &str) -> LineDraw<'static> {
    if let Some(id) = stored {
        let draw_id = effective_id(id, bold);
        if let Some(face) = pdfcraft_fonts::catalog::get(draw_id).filter(|face| face.available() && face.can_preview()) {
            let missing = pdfcraft_fonts::catalog::missing_glyphs(face, text);
            if !missing.is_empty() {
                return LineDraw::Missing { chars: missing };
            }
            let Some(bytes) = face.program() else { return LineDraw::Unreadable };
            return LineDraw::Draw { bytes, faux: bold && face.weight != 700, substituted: false };
        }
        return match line_face(bold) {
            Some((bytes, faux)) if covers_bytes(bytes, text) => LineDraw::Draw { bytes, faux, substituted: true },
            _ => LineDraw::Unreadable,
        };
    }
    match line_face(bold) {
        Some((bytes, faux)) if covers_bytes(bytes, text) => LineDraw::Draw { bytes, faux, substituted: false },
        _ => LineDraw::Unreadable,
    }
}

pub(crate) fn line_notes(design: &DrawnDate, date_text: &str) -> Vec<LineNote> {
    let mut notes = Vec::new();
    push_note(&mut notes, "上段", design.top_font.as_deref(), design.top_bold, &design.top);
    push_note(&mut notes, "日付", design.date_font.as_deref(), design.date_bold, date_text);
    push_note(&mut notes, "下段", design.bottom_font.as_deref(), design.bottom_bold, &design.bottom);
    notes
}

fn push_note(notes: &mut Vec<LineNote>, row: &'static str, stored: Option<&str>, bold: bool, text: &str) {
    if clean_line(text).is_empty() {
        return;
    }
    match resolve_line(stored, bold, text) {
        LineDraw::Draw { substituted: true, .. } => notes.push(LineNote::Substituted { row }),
        LineDraw::Draw { .. } => {}
        LineDraw::Missing { chars } => notes.push(LineNote::Missing { row, chars }),
        LineDraw::Unreadable => notes.push(LineNote::Unreadable { row, stored: stored.is_some() }),
    }
}

pub(crate) fn blocks_date_save(design: &DrawnDate, date_text: &str) -> bool {
    line_notes(design, date_text).iter().any(|note| matches!(note, LineNote::Missing { .. }))
}

fn paint_caption(buf: &mut [u8], mark: &Mark, inner: f64, line: Caption<'_>) -> Option<()> {
    let text = clean_line(line.text);
    if text.is_empty() {
        return Some(());
    }
    let (bytes, faux) = match resolve_line(line.font, line.bold, &text) {
        LineDraw::Draw { bytes, faux, .. } => (bytes, faux),
        LineDraw::Missing { .. } | LineDraw::Unreadable => return None,
    };
    let layout = layout_font(bytes, &text, faux)?;
    let side_f = f64::from(mark.side);
    let mut em = (line.size * side_f).clamp(6.0, side_f * 0.4);
    let y_center = line.y * side_f;
    for _ in 0..14 {
        let height = (layout.ascent - layout.descent) * em;
        let half = chord_half(mark.cx, mark.cy, inner - 1.5, y_center)
            .min(chord_half(mark.cx, mark.cy, inner - 1.5, y_center - height * 0.5))
            .min(chord_half(mark.cx, mark.cy, inner - 1.5, y_center + height * 0.5));
        let fits_w = layout.width * em <= half * 2.0 * 0.92;
        let fits_h = height <= side_f * 0.24;
        if fits_w && fits_h {
            break;
        }
        em *= 0.86;
        if em < 6.0 {
            em = 6.0;
            break;
        }
    }
    let text_w = layout.width * em;
    let text_h = (layout.ascent - layout.descent) * em;
    let left = mark.cx - text_w * 0.5;
    let baseline = y_center - text_h * 0.5 + layout.ascent * em;
    paint_glyphs(buf, mark, &layout, left, baseline, em, inner - 0.5);
    Some(())
}

fn chord_half(cx: f64, cy: f64, radius: f64, y: f64) -> f64 {
    let dy = y - cy;
    let t = radius * radius - dy * dy;
    if !t.is_finite() || t <= 1.0 { 0.0 } else { t.sqrt().min(cx) }
}

fn paint_ring(buf: &mut [u8], side: u32, cx: f64, cy: f64, inner: f64, outer: f64, color: [u8; 3]) {
    for y in 0..side {
        for x in 0..side {
            let d = dist(f64::from(x) + 0.5, f64::from(y) + 0.5, cx, cy);
            let cover = band_cover(d, inner, outer);
            blend(buf, side, x, y, color, cover);
        }
    }
}

fn paint_rule(buf: &mut [u8], mark: &Mark, inner: f64, y0_frac: f64, y1_frac: f64, thick: f64) {
    let y = (y0_frac + y1_frac) * 0.5 * f64::from(mark.side);
    let half = chord_half(mark.cx, mark.cy, inner - thick, y) * 0.86;
    if half < 4.0 {
        return;
    }
    let x0 = (mark.cx - half).floor().max(0.0) as u32;
    let x1 = (mark.cx + half).ceil().min(f64::from(mark.side)) as u32;
    let y0 = (y - thick - 1.0).floor().max(0.0) as u32;
    let y1 = (y + thick + 1.0).ceil().min(f64::from(mark.side)) as u32;
    for py in y0..y1 {
        let v = band_cover(f64::from(py) + 0.5, y - thick * 0.5, y + thick * 0.5);
        for px in x0..x1 {
            let h = band_cover(f64::from(px) + 0.5, mark.cx - half, mark.cx + half);
            blend(buf, mark.side, px, py, mark.color, v * h);
        }
    }
}

fn paint_glyphs(buf: &mut [u8], mark: &Mark, layout: &TextLayout, left: f64, baseline: f64, em: f64, limit: f64) {
    let mut min = [f64::MAX, f64::MAX];
    let mut max = [f64::MIN, f64::MIN];
    let mut placed = Vec::new();
    for contour in &layout.contours {
        let mut pts = Vec::with_capacity(contour.len());
        for p in contour {
            let at = [left + p[0] * em, baseline - p[1] * em];
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
    if placed.is_empty() {
        return;
    }
    let x0 = finite_px(min[0].floor() - 1.0, mark.side);
    let y0 = finite_px(min[1].floor() - 1.0, mark.side);
    let x1 = finite_px(max[0].ceil() + 1.0, mark.side);
    let y1 = finite_px(max[1].ceil() + 1.0, mark.side);
    for y in y0..y1 {
        for x in x0..x1 {
            let mut hit = 0u8;
            for sy in [0.25, 0.75] {
                for sx in [0.25, 0.75] {
                    let (px, py) = (f64::from(x) + sx, f64::from(y) + sy);
                    if dist(px, py, mark.cx, mark.cy) > limit {
                        continue;
                    }
                    if winding(&placed, px, py) {
                        hit = hit.saturating_add(1);
                    }
                }
            }
            blend(buf, mark.side, x, y, mark.color, f64::from(hit) / 4.0);
        }
    }
}

fn dist(x: f64, y: f64, cx: f64, cy: f64) -> f64 {
    let (dx, dy) = (x - cx, y - cy);
    (dx * dx + dy * dy).sqrt()
}

/// Coverage of the interval `[inner, outer]` for a sample one pixel wide.
fn band_cover(sample: f64, inner: f64, outer: f64) -> f64 {
    let inside_outer = (outer - sample + 0.5).clamp(0.0, 1.0);
    let outside_inner = (sample - inner + 0.5).clamp(0.0, 1.0);
    inside_outer * outside_inner
}

fn blend(buf: &mut [u8], side: u32, x: u32, y: u32, color: [u8; 3], coverage: f64) {
    if !coverage.is_finite() || coverage <= 0.0 {
        return;
    }
    let a = (coverage * 255.0).round().clamp(0.0, 255.0) as u16;
    if a == 0 {
        return;
    }
    let i = (y as usize).saturating_mul(side as usize).saturating_add(x as usize).saturating_mul(4);
    let Some(px) = buf.get_mut(i..i + 4) else { return };
    let inv = 255 - a;
    for channel in 0..3 {
        let src = u16::from(color[channel]) * a;
        let dst = u16::from(px[channel]) * inv;
        px[channel] = ((src + dst) / 255) as u8;
    }
    px[3] = (a + u16::from(px[3]) * inv / 255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_percent_matches_the_old_hundred_and_scales_from_it() {
        let small = nominal_size(0, Some((100, 50)));
        assert_eq!(small, (100.0, 50.0));
        let at = [100.0, 80.0];
        let full = place_rect(at, small, 100.0);
        assert_eq!(full, [50.0, 55.0, 150.0, 105.0]);
        for (percent, width) in [(50.0, 50.0), (80.0, 80.0), (120.0, 120.0), (200.0, 200.0)] {
            let rect = place_rect(at, small, percent);
            let got = rect[2] - rect[0];
            assert!((got - width).abs() < 1e-9, "{percent}% width {got}");
            assert!((percent_of(rect, small.0) - percent).abs() < 1e-9);
            let aspect = (rect[2] - rect[0]) / (rect[3] - rect[1]);
            assert!((aspect - 2.0).abs() < 1e-9);
        }
        let capped = nominal_size(0, Some((400, 200)));
        assert_eq!(capped, (200.0, 100.0));
        assert!((place_rect(at, capped, 50.0)[2] - place_rect(at, capped, 50.0)[0] - 100.0).abs() < 1e-9);
        assert!((place_rect(at, capped, 120.0)[2] - place_rect(at, capped, 120.0)[0] - 240.0).abs() < 1e-9);
        let turned = nominal_size(90, Some((100, 50)));
        assert_eq!(turned, (50.0, 100.0));
    }

    #[test]
    fn drag_width_reports_its_percent_and_an_out_of_range_value_is_not_a_default() {
        let nominal = (100.0, 50.0);
        let full = place_rect([40.0, 40.0], nominal, 100.0);
        let dragged = rect_at_scale(full, nominal, 80.0);
        assert!((percent_of(dragged, nominal.0) - 80.0).abs() < 1e-9);
        assert!(checked_scale(80.0).is_some());
        assert!(checked_scale(10.0).is_none());
        assert!(checked_scale(500.0).is_none());
        assert_eq!(scale_or_hundred(0.0), 100.0);
        assert_eq!(scale_or_hundred(f64::NAN), 100.0);
    }

    #[test]
    fn a_square_with_clear_corners_is_a_diameter() {
        let (w, h) = (32u32, 32u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let dx = x as f64 - 15.5;
                let dy = y as f64 - 15.5;
                if dx * dx + dy * dy <= 12.0 * 12.0 {
                    let i = ((y * w + x) * 4) as usize;
                    rgba[i] = 20;
                    rgba[i + 1] = 20;
                    rgba[i + 2] = 20;
                    rgba[i + 3] = 255;
                }
            }
        }
        assert!(looks_circular(w, h, &rgba));
        let rect = [0.0, 0.0, 72.0, 72.0];
        assert_eq!(size_label(rect, true), "直径 25.4 mm");
        assert_eq!(size_label([0.0, 0.0, 72.0, 36.0], false), "25.4 × 12.7 mm");
        assert!(!looks_circular(40, 20, &vec![255u8; 40 * 20 * 4]));
    }

    #[test]
    fn dates_reject_impossible_days_and_japan_is_nine_hours_ahead() {
        assert_eq!(parse_date("2026/10/1").unwrap().format(), "2026/10/1");
        assert_eq!(parse_date("2026/09/05").unwrap().format(), "2026/9/5");
        assert_eq!(parse_date("2024/2/29").unwrap().day, 29);
        assert!(parse_date("2026/2/29").is_none());
        assert!(parse_date("2026/2/30").is_none());
        assert!(parse_date("2026/4/31").is_none());
        assert!(parse_date("2026/0/1").is_none());
        assert!(parse_date("2026/13/1").is_none());
        assert!(parse_date("2026-10-1").is_none());
        assert!(parse_date("").is_none());
        assert_eq!(japan_date_at(0).format(), "1970/1/1");
        assert_eq!(japan_date_at(15 * 3_600 - 1).format(), "1970/1/1");
        assert_eq!(japan_date_at(15 * 3_600).format(), "1970/1/2");
        assert_eq!(weekday(StampDate { year: 1970, month: 1, day: 1 }), 4);
        assert_eq!(
            japan_today().format(),
            japan_date_at(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()).format()
        );
    }

    #[test]
    fn date_ink_stays_in_the_region_and_clear_pixels_stay_clear() {
        let (w, h) = (96u32, 64u32);
        let mut base = vec![0u8; (w * h * 4) as usize];
        base[0] = 128;
        base[1] = 128;
        base[2] = 128;
        base[3] = 255;
        let original = base.clone();
        let region = [0.20, 0.25, 0.60, 0.50];
        let color = [196u8, 30, 58];
        let out = composite_rgba(&base, w, h, StampDate { year: 2026, month: 10, day: 1 }, region, 0.28, color).unwrap();
        assert_eq!(base, original, "the base buffer is not modified");
        assert_eq!(&out[0..4], &original[0..4]);
        let (rx, ry, rw, rh) = (region[0] * w as f64, region[1] * h as f64, region[2] * w as f64, region[3] * h as f64);
        let mut ink = 0usize;
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if out[i..i + 4] == original[i..i + 4] {
                    continue;
                }
                let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
                assert!(cx >= rx && cy >= ry && cx < rx + rw && cy < ry + rh, "ink at {x},{y} left the region");
                assert_eq!(&out[i..i + 3], &color);
                assert_eq!(out[i + 3], 255);
                ink += 1;
            }
        }
        assert!(ink > 20, "the date should leave visible ink, got {ink}");
        let png = encode_rgba_png(w, h, &out).unwrap();
        let (_w, _h, decoded) = crate::seal_store::preview_rgba(&png).unwrap();
        assert_eq!(decoded[3], 255);
        let clear = decoded.chunks_exact(4).filter(|px| px[3] == 0).count();
        assert!(clear > 100, "transparent pixels of the base survive the PNG");
    }

    #[test]
    fn old_custom_stamp_json_without_a_scale_is_one_hundred() {
        let png = pdfcraft_xfa::fixtures::tiny_png();
        let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png);
        let old = format!(r#"[{{"category":"試験","name":"架空","file":"a.png","page":0,"data":"{data}"}}]"#);
        let v: serde_json::Value = serde_json::from_str(&old).unwrap();
        let stamps = crate::stamps_ui::decode(&v);
        assert_eq!(stamps.len(), 1);
        assert_eq!(stamps[0].scale, 100.0);
        assert_eq!(stamps[0].data.as_slice(), png.as_slice());
        let wild = format!(r#"[{{"category":"試験","name":"架空","file":"a.png","scale":1000,"data":"{data}"}}]"#);
        let v: serde_json::Value = serde_json::from_str(&wild).unwrap();
        assert_eq!(crate::stamps_ui::decode(&v)[0].scale, 100.0);
    }

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

    fn gray_png() -> Vec<u8> {
        let (w, h) = (80u32, 40u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if x < 4 || y < 4 || x + 4 >= w || y + 4 >= h {
                    rgba[i] = 80;
                    rgba[i + 1] = 80;
                    rgba[i + 2] = 80;
                    rgba[i + 3] = 255;
                }
            }
        }
        encode_rgba_png(w, h, &rgba).unwrap()
    }

    fn render_rgba(bytes: &[u8]) -> Vec<u8> {
        let mut renderer = pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes.to_vec()), Default::default());
        let shot = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 2.0, ..Default::default() });
        assert!(shot.error.is_none(), "{:?}", shot.error);
        shot.rgba.to_vec()
    }

    fn has_red(rgba: &[u8]) -> bool {
        rgba.chunks_exact(4).any(|px| px[3] > 20 && px[0] > px[1].saturating_add(40) && px[0] > px[2])
    }

    fn stamp_count(app: &crate::PdfCraftApp) -> usize {
        let id = app.views[0].id;
        app.session.get(id).unwrap().info.annotations.iter().filter(|a| a.subtype == "Stamp").count()
    }

    #[test]
    fn colour_print_preview_keeps_vermillion() {
        let img = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[196, 30, 58, 255]);
        let kept = crate::canvas::print_preview_image(crate::canvas::print_preview_tag(1.0, false), img);
        assert_eq!(kept.pixels[0].to_array(), [196, 30, 58, 255]);
    }

    #[test]
    fn scale_and_date_stamp_round_trip_without_touching_the_original() {
        let _policy = policy();
        let dir = std::env::temp_dir().join(format!("swiftpdf-date-{}-place", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let png = gray_png();
        let mut app = crate::PdfCraftApp::new();
        let original = app.session.create_blank(200.0, 300.0, 1).unwrap();
        let original_path = dir.join("opened.pdf");
        std::fs::write(&original_path, original.as_ref()).unwrap();
        use sha2::Digest;
        let original_sha = sha2::Sha256::digest(original.as_ref());
        app.open_bytes("opened.pdf", Some(original_path.to_string_lossy().into_owned()), original.to_vec()).unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        app.grant_edit_text();
        app.grant_save_as();
        app.grant_print();
        app.start_custom_stamp("mark.png".into(), png.clone());
        app.save_custom_stamp();
        app.dialog = None;
        assert_eq!(app.custom_stamps[0].scale, 100.0);
        for percent in [100.0, 50.0, 80.0, 120.0, 200.0] {
            app.custom_stamps[0].scale = percent;
            app.quick_tool = crate::QuickTool::CustomStamp(0);
            app.begin_stamp_place(0, 0, [100.0, 150.0], 0).unwrap();
            let rect = app.views[0].stamp_place.as_ref().unwrap().rect;
            assert!((rect[2] - rect[0] - 80.0 * percent / 100.0).abs() < 1e-6, "{percent}");
            assert!(((rect[2] - rect[0]) / (rect[3] - rect[1]) - 2.0).abs() < 1e-6);
            app.views[0].stamp_place = None;
        }
        app.custom_stamps[0].scale = 100.0;
        app.quick_tool = crate::QuickTool::CustomStamp(0);
        app.begin_stamp_place(0, 0, [100.0, 150.0], 0).unwrap();
        {
            let place = app.views[0].stamp_place.as_mut().unwrap();
            place.rect = rect_at_scale(place.rect, place.nominal, 80.0);
            place.scale = percent_of(place.rect, place.nominal.0);
            assert!((place.scale - 80.0).abs() < 1e-6);
        }
        app.save_place_scale();
        assert!((app.custom_stamps[0].scale - 80.0).abs() < 1e-6);

        app.start_date_stamp("date.png".into(), png.clone());
        assert_eq!(app.date_draft.date_text, japan_today().format());
        app.date_draft.scale = 100.0;
        app.save_date_stamp();
        app.dialog = None;
        let base = app.date_stamps[0].data.clone();
        assert_eq!(base.as_slice(), png.as_slice());
        app.quick_tool = crate::QuickTool::DateStamp(0);
        app.begin_stamp_place(0, 0, [100.0, 80.0], 0).unwrap();
        assert_eq!(app.views[0].stamp_place.as_ref().unwrap().date_overlay.as_ref().unwrap().date, japan_today());
        {
            let place = app.views[0].stamp_place.as_mut().unwrap();
            place.date_overlay.as_mut().unwrap().date = parse_date("2024/2/29").unwrap();
            let overlay = place.date_overlay.clone().unwrap();
            assert_eq!(overlay.base, png);
            place.pixels = render_overlay(&overlay.base, &overlay);
        }
        assert_eq!(app.date_stamps[0].data, base, "the registered image stays the base");
        app.commit_stamp_place();
        assert!(app.dialog.is_none(), "a date stamp does not ask for chairman approval: {:?}", app.toast);
        assert!(app.seal_store.library.history.is_empty());
        assert_eq!(stamp_count(&app), 1, "{:?}", app.toast);
        assert_eq!(app.date_stamps[0].data, base);
        let id = app.views[0].id;
        let baked = render_rgba(&app.session.get(id).unwrap().bytes);
        assert!(has_red(&baked), "the fixed date is red ink");
        assert!(app.execute("edit.undo"), "{:?}", app.toast);
        assert_eq!(stamp_count(&app), 0);
        assert!(app.execute("edit.redo"), "{:?}", app.toast);
        assert_eq!(stamp_count(&app), 1);
        assert_eq!(render_rgba(&app.session.get(id).unwrap().bytes), baked);
        let saved = app.session.save_edited_copy_bytes(id).unwrap();
        assert_eq!(sha2::Sha256::digest(std::fs::read(&original_path).unwrap()), original_sha);
        assert!(has_red(&render_rgba(&saved)));
        let mut reopened = crate::PdfCraftApp::new();
        reopened.open_bytes("copy.pdf", None, saved.to_vec()).unwrap();
        let reopened_bytes = reopened.session.get(reopened.views[0].id).unwrap().bytes.clone();
        assert!(has_red(&render_rgba(&reopened_bytes)));
        let settings = pdfcraft_engine::print::Settings {
            pages: vec![0],
            raster: Some(pdfcraft_engine::print::Raster { dpi: 72.0, grayscale: false }),
            ..Default::default()
        };
        let printed = app.session.print_pdf(id, &settings).unwrap();
        assert!(has_red(&render_rgba(&printed)), "colour print keeps the date colour");
        let json = app.persist();
        let mut restarted = crate::PdfCraftApp::new();
        restarted.restore(&json);
        assert!((restarted.custom_stamps[0].scale - 80.0).abs() < 1e-6);
        assert_eq!(restarted.date_stamps[0].data, base);
        let mut old = serde_json::from_str::<serde_json::Value>(&json).unwrap();
        old["custom_stamps"][0].as_object_mut().unwrap().remove("scale");
        old["date_stamps"][0].as_object_mut().unwrap().remove("scale");
        let mut legacy = crate::PdfCraftApp::new();
        legacy.restore(&old.to_string());
        assert_eq!(legacy.custom_stamps[0].scale, 100.0);
        assert_eq!(legacy.date_stamps[0].scale, 100.0);
        assert_eq!(legacy.date_stamps[0].data, base);
        assert!(!json.contains("SWSEAL1"));
        #[cfg(windows)]
        {
            app.set_seal_store(dir.join("seals.bin"));
            app.start_chairman_seal("seal.png".into(), png);
            app.accept_stamp_registration();
            app.views[0].stamp_place = None;
            app.quick_tool = crate::QuickTool::ChairmanSeal(0);
            app.begin_stamp_place(0, 0, [30.0, 30.0], 0).unwrap();
            let before = stamp_count(&app);
            app.ask_stamp_confirm();
            assert!(matches!(app.dialog, Some(crate::Dialog::ConfirmSeal)));
            assert_eq!(stamp_count(&app), before);
            assert!(app.seal_store.library.history.is_empty());
            app.dialog = None;
        }
        assert_eq!(sha2::Sha256::digest(std::fs::read(&original_path).unwrap()), original_sha);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restricted_and_signed_pdfs_still_refuse_stamps_and_blocked_printing() {
        let _policy = policy();
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
        banned.grant_print();
        banned.open_bytes("locked.pdf", None, locked.to_vec()).unwrap();
        banned.start_custom_stamp("mark.png".into(), gray_png());
        banned.save_custom_stamp();
        banned.quick_tool = crate::QuickTool::CustomStamp(0);
        let refused = banned.begin_stamp_place(0, 0, [40.0, 40.0], 0);
        assert!(refused.is_err(), "{refused:?}");
        assert_eq!(stamp_count(&banned), 0);
        let print = banned.session.print_pdf(banned.views[0].id, &pdfcraft_engine::print::Settings { pages: vec![0], ..Default::default() });
        assert!(matches!(print, Err(pdfcraft_engine::EditError::NotPermitted("printing"))), "{print:?}");

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
        signed_app.start_date_stamp("date.png".into(), gray_png());
        signed_app.save_date_stamp();
        signed_app.quick_tool = crate::QuickTool::DateStamp(0);
        assert!(signed_app.begin_stamp_place(0, 0, [40.0, 40.0], 0).is_err());
        signed_app.start_drawn_date();
        signed_app.save_date_stamp();
        signed_app.dialog = None;
        signed_app.quick_tool = crate::QuickTool::DateStamp(1);
        assert!(signed_app.begin_stamp_place(0, 0, [40.0, 40.0], 0).is_err());
        assert_eq!(stamp_count(&signed_app), 0);
        assert!(signed_app.seal_store.library.history.is_empty());
    }

    fn row_bytes(rgba: &[u8], side: u32, y0: f64, y1: f64) -> Vec<u8> {
        let start_y = (y0 * f64::from(side)).floor().clamp(0.0, f64::from(side)) as u32;
        let end_y = (y1 * f64::from(side)).ceil().clamp(0.0, f64::from(side)) as u32;
        let start = (start_y as usize).saturating_mul(side as usize).saturating_mul(4);
        let end = (end_y as usize).saturating_mul(side as usize).saturating_mul(4);
        rgba.get(start..end).unwrap_or_default().to_vec()
    }

    fn drawn_overlay(design: DrawnDate, date: &str) -> DateOverlay {
        let date = parse_date(date).unwrap();
        DateOverlay {
            date,
            entry: date.format(),
            region: default_region(),
            font: default_font(),
            color: [196, 30, 58],
            base: Vec::new(),
            drawn: Some(design),
            show_calendar: false,
            calendar_year: date.year,
            calendar_month: date.month,
        }
    }

    fn paint_or_skip(design: DrawnDate) -> Option<(u32, Vec<u8>)> {
        let overlay = drawn_overlay(design, "2026/10/1");
        match render_drawn_stamp(&overlay, 256) {
            Some((side, _, rgba)) => Some((side, rgba)),
            None => {
                assert!(pdfcraft_fonts::document_japanese_font_for_style(false, false).is_none());
                None
            }
        }
    }

    #[test]
    fn a_missing_bold_face_is_drawn_heavier_instead_of_ignored() {
        let regular = layout_font(pdfcraft_fonts::INTER_REGULAR, "A", false).unwrap();
        let heavy = layout_font(pdfcraft_fonts::INTER_REGULAR, "A", true).unwrap();
        assert!(heavy.contours.len() > regular.contours.len());
        assert!(heavy.width > regular.width);
    }

    #[test]
    fn drawn_date_json_keeps_old_png_stamps_and_does_not_rewrite_them() {
        let png = gray_png();
        let old = DateStamp {
            name: "旧印".into(),
            file: "old.png".into(),
            data: std::sync::Arc::new(png.clone()),
            scale: 80.0,
            region: default_region(),
            font: default_font(),
            color: default_color(),
            drawn: None,
        };
        let mut stored = serde_json::to_value(&old).unwrap();
        stored.as_object_mut().unwrap().remove("drawn");
        let raw = stored.to_string();
        let loaded = decode_dates(&serde_json::Value::Array(vec![serde_json::from_str(&raw).unwrap()]));
        assert_eq!(raw, stored.to_string());
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].drawn.is_none());
        assert_eq!(loaded[0].data.as_slice(), png.as_slice());
        assert!((loaded[0].scale - 80.0).abs() < 1e-6);

        let drawn =
            DrawnDate {
                top: "東京テラス".into(), bottom: "理事会".into(), top_bold: true, date_bold: true, date_size: 80.0, ..DrawnDate::default()
            };
        let fresh = DateStamp {
            name: "新しい印".into(),
            file: "date-stamp.png".into(),
            data: std::sync::Arc::new(Vec::new()),
            scale: 150.0,
            region: default_region(),
            font: default_font(),
            color: [196, 30, 58],
            drawn: Some(drawn),
        };
        let encoded = serde_json::to_value(vec![fresh]).unwrap();
        let back = decode_dates(&encoded);
        assert_eq!(back.len(), 1);
        assert!(back[0].data.is_empty());
        let design = back[0].drawn.clone().unwrap();
        assert_eq!(design.top, "東京テラス");
        assert!(design.top_bold);
        assert!(design.date_bold);
        assert!((design.date_size - 0.30).abs() < 1e-6, "a size past the limit is clamped, not trusted");
        assert!((back[0].scale - 150.0).abs() < 1e-6);

        let mut broken = encoded.clone();
        broken[0]["drawn"]["ring"] = serde_json::json!("wide");
        assert!(decode_dates(&broken).is_empty(), "a damaged drawn stamp is skipped");
        let mut blank = encoded.clone();
        blank[0]["drawn"] = serde_json::Value::Null;
        blank[0]["data"] = serde_json::json!("");
        assert!(decode_dates(&blank).is_empty(), "an image stamp still needs its PNG");
    }

    #[test]
    fn drawn_stamp_is_a_clear_vermillion_circle_with_independent_weights() {
        let design = DrawnDate {
            top: "済".into(),
            bottom: "管理センター".into(),
            top_bold: false,
            bottom_bold: false,
            date_bold: true,
            ..DrawnDate::default()
        };
        let Some((side, rgba)) = paint_or_skip(design.clone()) else { return };
        assert_eq!(rgba.len(), (side as usize) * (side as usize) * 4);
        assert_eq!(rgba[3], 0, "the corner stays transparent");
        let mut solid = false;
        let mut soft = false;
        let radius = design.diameter * f64::from(side) * 0.5 + 2.0;
        let center = f64::from(side) * 0.5;
        for y in 0..side {
            for x in 0..side {
                let i = (y as usize) * (side as usize) + x as usize;
                let px = &rgba[i * 4..i * 4 + 4];
                if px[3] == 255 {
                    solid = true;
                    assert_eq!(&px[..3], &[196, 30, 58]);
                }
                if (1..255).contains(&px[3]) {
                    soft = true;
                }
                if px[3] > 24 {
                    let (dx, dy) = (f64::from(x) + 0.5 - center, f64::from(y) + 0.5 - center);
                    assert!(dx * dx + dy * dy <= radius * radius, "ink outside the circle at {x},{y}");
                }
            }
        }
        assert!(solid, "the ring has a solid vermillion pixel");
        assert!(soft, "the ring is antialiased");
        assert!(row_bytes(&rgba, side, 0.42, 0.58).iter().step_by(4).any(|a| *a > 180), "the date is in the middle");
        let png = encode_rgba_png(side, side, &rgba).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

        let mut top_bold = design.clone();
        top_bold.top_bold = true;
        let (_, top_rgba) = paint_or_skip(top_bold).unwrap();
        assert_ne!(row_bytes(&rgba, side, 0.18, 0.38), row_bytes(&top_rgba, side, 0.18, 0.38));
        assert_eq!(row_bytes(&rgba, side, 0.62, 0.86), row_bytes(&top_rgba, side, 0.62, 0.86));

        let mut bottom_bold = design.clone();
        bottom_bold.bottom_bold = true;
        let (_, bottom_rgba) = paint_or_skip(bottom_bold).unwrap();
        assert_ne!(row_bytes(&rgba, side, 0.62, 0.86), row_bytes(&bottom_rgba, side, 0.62, 0.86));
        assert_eq!(row_bytes(&rgba, side, 0.18, 0.38), row_bytes(&bottom_rgba, side, 0.18, 0.38));

        let mut date_regular = design.clone();
        date_regular.date_bold = false;
        let (_, date_rgba) = paint_or_skip(date_regular).unwrap();
        assert_ne!(row_bytes(&rgba, side, 0.42, 0.58), row_bytes(&date_rgba, side, 0.42, 0.58));
        assert_eq!(row_bytes(&rgba, side, 0.18, 0.38), row_bytes(&date_rgba, side, 0.18, 0.38));
        assert_eq!(row_bytes(&rgba, side, 0.62, 0.86), row_bytes(&date_rgba, side, 0.62, 0.86));

        let mut small = design.clone();
        small.date_size = 0.10;
        let mut large = design.clone();
        large.date_size = 0.20;
        let (_, small_rgba) = paint_or_skip(small).unwrap();
        let (_, large_rgba) = paint_or_skip(large).unwrap();
        assert_ne!(row_bytes(&small_rgba, side, 0.42, 0.58), row_bytes(&large_rgba, side, 0.42, 0.58));
        assert_eq!(row_bytes(&small_rgba, side, 0.18, 0.38), row_bytes(&large_rgba, side, 0.18, 0.38));

        let mut crowded = design.clone();
        crowded.top = "東京テラス管理組合理事会".into();
        crowded.top_size = 0.22;
        let (_, crowded_rgba) = paint_or_skip(crowded).unwrap();
        for y in 0..side {
            for x in 0..side {
                let a = crowded_rgba[((y as usize) * (side as usize) + x as usize) * 4 + 3];
                if a > 24 {
                    let (dx, dy) = (f64::from(x) + 0.5 - center, f64::from(y) + 0.5 - center);
                    assert!(dx * dx + dy * dy <= radius * radius);
                }
            }
        }
    }

    #[test]
    fn a_drawn_date_stamp_bakes_its_date_and_a_later_edit_does_not_rewrite_the_page() {
        let _policy = policy();
        if pdfcraft_fonts::document_japanese_font_for_style(false, true).is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("swiftpdf-drawn-date-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = crate::PdfCraftApp::new();
        let original = app.session.create_blank(240.0, 320.0, 1).unwrap();
        let original_path = dir.join("opened.pdf");
        std::fs::write(&original_path, original.as_ref()).unwrap();
        use sha2::Digest;
        let original_sha = sha2::Sha256::digest(original.as_ref());
        app.open_bytes("opened.pdf", Some(original_path.to_string_lossy().into_owned()), original.to_vec()).unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        app.grant_save_as();
        app.grant_print();
        app.start_drawn_date();
        assert_eq!(app.date_draft.date_text, japan_today().format());
        assert!(app.date_draft.drawn.as_ref().unwrap().date_bold);
        app.date_draft.scale = 100.0;
        app.date_draft.drawn.as_mut().unwrap().bottom = "管理事務室".into();
        app.save_date_stamp();
        app.dialog = None;
        assert!(app.date_stamps[0].data.is_empty());
        assert!(app.date_stamps[0].drawn.is_some());
        let before_place = app.session.get(app.views[0].id).unwrap().bytes.clone();
        app.quick_tool = crate::QuickTool::DateStamp(0);
        app.begin_stamp_place(0, 0, [120.0, 160.0], 0).unwrap();
        {
            let place = app.views[0].stamp_place.as_ref().unwrap();
            assert!(place.circular);
            assert!((place.rect[2] - place.rect[0] - 200.0).abs() < 1e-6);
            assert_eq!(place.date_overlay.as_ref().unwrap().date, japan_today());
            assert!(place.pixels.is_some());
        }
        app.views[0].stamp_place = None;
        assert_eq!(app.session.get(app.views[0].id).unwrap().bytes, before_place, "cancel leaves the PDF");
        app.begin_stamp_place(0, 0, [120.0, 160.0], 0).unwrap();
        {
            let place = app.views[0].stamp_place.as_mut().unwrap();
            place.date_overlay.as_mut().unwrap().date = parse_date("2024/2/29").unwrap();
            place.date_overlay.as_mut().unwrap().entry = "2024/2/29".into();
            place.opacity = 0.5;
            place.rect = rect_at_scale(place.rect, place.nominal, 50.0);
            place.scale = 50.0;
        }
        app.commit_stamp_place();
        assert!(app.dialog.is_none());
        assert!(app.seal_store.library.history.is_empty());
        assert_eq!(stamp_count(&app), 1, "{:?}", app.toast);
        let id = app.views[0].id;
        let baked = render_rgba(&app.session.get(id).unwrap().bytes);
        assert!(has_red(&baked));
        app.open_date_draft(0);
        app.date_draft.drawn.as_mut().unwrap().top = "受領".into();
        app.date_draft.drawn.as_mut().unwrap().top_bold = true;
        app.date_draft.scale = 200.0;
        app.save_date_stamp();
        app.dialog = None;
        assert_eq!(app.date_stamps[0].drawn.as_ref().unwrap().top, "受領");
        assert!((app.date_stamps[0].scale - 200.0).abs() < 1e-6);
        assert_eq!(render_rgba(&app.session.get(id).unwrap().bytes), baked, "editing the library does not redraw a stamp already on the page");
        assert!(app.execute("edit.undo"), "{:?}", app.toast);
        assert_eq!(stamp_count(&app), 0);
        assert!(app.execute("edit.redo"), "{:?}", app.toast);
        assert_eq!(render_rgba(&app.session.get(id).unwrap().bytes), baked);
        let saved = app.session.save_edited_copy_bytes(id).unwrap();
        assert_eq!(sha2::Sha256::digest(std::fs::read(&original_path).unwrap()), original_sha);
        let mut reopened = crate::PdfCraftApp::new();
        reopened.open_bytes("copy.pdf", None, saved.to_vec()).unwrap();
        assert_eq!(render_rgba(&reopened.session.get(reopened.views[0].id).unwrap().bytes), baked);
        let settings = pdfcraft_engine::print::Settings {
            pages: vec![0],
            raster: Some(pdfcraft_engine::print::Raster { dpi: 72.0, grayscale: false }),
            ..Default::default()
        };
        let printed = app.session.print_pdf(id, &settings).unwrap();
        assert!(has_red(&render_rgba(&printed)));
        let gray = pdfcraft_engine::print::Settings {
            pages: vec![0],
            raster: Some(pdfcraft_engine::print::Raster { dpi: 72.0, grayscale: true }),
            ..Default::default()
        };
        let gray_bytes = app.session.print_pdf(id, &gray).unwrap();
        let gray_rgba = render_rgba(&gray_bytes);
        assert_ne!(gray_rgba, baked);
        let swatch = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[196, 30, 58, 255]);
        let kept = crate::canvas::print_preview_image(crate::canvas::print_preview_tag(1.0, false), swatch.clone());
        assert_eq!(kept.pixels[0].to_array(), [196, 30, 58, 255]);
        let washed = crate::canvas::print_preview_image(crate::canvas::print_preview_tag(1.0, true), swatch);
        assert_eq!(washed.pixels[0].r(), washed.pixels[0].g());
        assert_eq!(washed.pixels[0].g(), washed.pixels[0].b());
        let json = app.persist();
        let mut restarted = crate::PdfCraftApp::new();
        restarted.restore(&json);
        assert!((restarted.date_stamps[0].scale - 200.0).abs() < 1e-6);
        assert_eq!(restarted.date_stamps[0].drawn.as_ref().unwrap().top, "受領");
        assert_eq!(sha2::Sha256::digest(std::fs::read(&original_path).unwrap()), original_sha);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn font_ids_round_trip_and_old_files_omit_them() {
        let old = serde_json::json!({
            "top": "済",
            "top_bold": false,
            "bottom": "",
            "date_bold": true
        });
        let loaded: DrawnDate = serde_json::from_value(old).unwrap();
        assert!(loaded.top_font.is_none() && loaded.date_font.is_none() && loaded.bottom_font.is_none());
        let again = serde_json::to_value(&loaded).unwrap();
        assert!(again.get("top_font").is_none() && again.get("date_font").is_none() && again.get("bottom_font").is_none());

        let design = DrawnDate {
            top_font: Some("craft:shippori-mincho:regular".into()),
            date_font: Some("bundled:inter:semibold".into()),
            bottom_font: Some("craft:biz-udp-gothic:bold".into()),
            bottom_bold: true,
            ..DrawnDate::default()
        };
        let stored = serde_json::to_value(&design).unwrap();
        let back: DrawnDate = serde_json::from_value(stored).unwrap();
        assert_eq!(back.top_font.as_deref(), Some("craft:shippori-mincho:regular"));
        assert_eq!(back.date_font.as_deref(), Some("bundled:inter:semibold"));
        assert_eq!(back.bottom_font.as_deref(), Some("craft:biz-udp-gothic:bold"));
        assert!(back.date_bold);

        let mut unknown = DrawnDate { top_font: Some("no-such-face".into()), ..DrawnDate::default() };
        unknown.clamp();
        assert_eq!(unknown.top_font.as_deref(), Some("no-such-face"));
    }

    #[test]
    fn bold_checkbox_uses_a_real_bold_face_only() {
        assert_eq!(id_for_bold(None, true), None);
        assert_eq!(id_for_bold(Some("bundled:inter:regular"), true).as_deref(), Some("bundled:inter:regular"));
        assert_eq!(id_for_bold(Some("bundled:inter:medium"), true).as_deref(), Some("bundled:inter:medium"));
        assert_eq!(id_for_bold(Some("bundled:inter:semibold"), true).as_deref(), Some("bundled:inter:semibold"));
        let gothic = "craft:biz-udp-gothic:regular";
        let bold = "craft:biz-udp-gothic:bold";
        if pdfcraft_fonts::catalog::get(bold).is_some_and(|face| face.available()) {
            assert_eq!(id_for_bold(Some(gothic), true).as_deref(), Some(bold));
            assert_eq!(id_for_bold(Some(bold), false).as_deref(), Some(gothic));
        } else {
            assert_eq!(id_for_bold(Some(gothic), true).as_deref(), Some(gothic));
        }
        assert_eq!(id_for_bold(Some("craft:shippori-mincho:regular"), true).as_deref(), Some("craft:shippori-mincho:regular"));
    }

    #[test]
    fn an_unknown_font_id_is_kept_and_a_missing_glyph_is_not_substituted() {
        let mut app = crate::PdfCraftApp::new();
        app.start_drawn_date();
        {
            let drawn = app.date_draft.drawn.as_mut().unwrap();
            drawn.top = "東京".into();
            drawn.top_font = Some("bundled:inter:regular".into());
        }
        let before = app.date_stamps.len();
        app.save_date_stamp();
        assert_eq!(app.date_stamps.len(), before);
        assert_eq!(app.date_draft.name, "データー印");
        assert_eq!(app.date_draft.drawn.as_ref().unwrap().top_font.as_deref(), Some("bundled:inter:regular"));
        assert!(blocks_date_save(app.date_draft.drawn.as_ref().unwrap(), &app.date_draft.date_text));

        let missing = DrawnDate { top: "東京".into(), top_font: Some("bundled:inter:regular".into()), ..DrawnDate::default() };
        assert!(render_drawn_stamp(&drawn_overlay(missing.clone(), "2026/10/1"), 128).is_none());
        assert_eq!(missing.top_font.as_deref(), Some("bundled:inter:regular"));
        assert!(matches!(line_notes(&missing, "2026/10/1").first(), Some(LineNote::Missing { row: "上段", .. })));

        let mut unknown = DrawnDate { top: "東京".into(), top_font: Some("no-such-face".into()), ..DrawnDate::default() };
        let notes = line_notes(&unknown, "2026/10/1");
        if pdfcraft_fonts::document_japanese_font_for_style(false, false).is_some() {
            assert!(matches!(notes.first(), Some(LineNote::Substituted { row: "上段" })));
            assert!(render_drawn_stamp(&drawn_overlay(unknown.clone(), "2026/10/1"), 128).is_some());
        }
        unknown.clamp();
        assert_eq!(unknown.top_font.as_deref(), Some("no-such-face"));
    }

    #[test]
    fn drawn_lines_keep_their_own_faces() {
        let inter = DrawnDate {
            top: String::new(),
            bottom: String::new(),
            date_bold: false,
            date_font: Some("bundled:inter:regular".into()),
            ..DrawnDate::default()
        };
        let mut mono = inter.clone();
        mono.date_font = Some("bundled:jetbrains-mono:regular".into());
        let mut semi = inter.clone();
        semi.date_font = Some("bundled:inter:semibold".into());
        let mut faux = inter.clone();
        faux.date_bold = true;
        let (_, inter_rgba) = paint_or_skip(inter.clone()).unwrap();
        let (side, mono_rgba) = paint_or_skip(mono).unwrap();
        let (_, semi_rgba) = paint_or_skip(semi).unwrap();
        let (_, faux_rgba) = paint_or_skip(faux.clone()).unwrap();
        assert_ne!(row_bytes(&inter_rgba, side, 0.42, 0.58), row_bytes(&mono_rgba, side, 0.42, 0.58));
        assert_ne!(row_bytes(&inter_rgba, side, 0.42, 0.58), row_bytes(&semi_rgba, side, 0.42, 0.58));
        assert_ne!(row_bytes(&inter_rgba, side, 0.42, 0.58), row_bytes(&faux_rgba, side, 0.42, 0.58));
        assert_eq!(faux.date_font.as_deref(), Some("bundled:inter:regular"));
        assert_eq!(inter_rgba[3], 0);

        let Some(mincho) = pdfcraft_fonts::catalog::get("craft:shippori-mincho:regular").filter(|face| face.program().is_some()) else {
            return;
        };
        let gothic = pdfcraft_fonts::catalog::get("craft:biz-udp-gothic:regular").unwrap();
        let mut base = DrawnDate {
            top: "東京".into(),
            bottom: "東京".into(),
            top_bold: false,
            bottom_bold: false,
            date_bold: false,
            date_font: Some("bundled:inter:regular".into()),
            ..DrawnDate::default()
        };
        let (_, gothic_rgba) = paint_or_skip(base.clone()).unwrap();
        base.top_font = Some(mincho.id.to_string());
        let (_, mincho_rgba) = paint_or_skip(base.clone()).unwrap();
        assert_ne!(row_bytes(&gothic_rgba, side, 0.18, 0.38), row_bytes(&mincho_rgba, side, 0.18, 0.38));
        assert_eq!(row_bytes(&gothic_rgba, side, 0.42, 0.58), row_bytes(&mincho_rgba, side, 0.42, 0.58));
        assert_eq!(row_bytes(&gothic_rgba, side, 0.62, 0.86), row_bytes(&mincho_rgba, side, 0.62, 0.86));
        let mut heavy_top = base.clone();
        heavy_top.top_bold = true;
        let (_, heavy_rgba) = paint_or_skip(heavy_top).unwrap();
        assert_ne!(row_bytes(&mincho_rgba, side, 0.18, 0.38), row_bytes(&heavy_rgba, side, 0.18, 0.38));
        assert_eq!(row_bytes(&mincho_rgba, side, 0.62, 0.86), row_bytes(&heavy_rgba, side, 0.62, 0.86));
        let ud = pdfcraft_fonts::catalog::get("craft:biz-ud-mincho:regular").unwrap();
        if ud.program().is_some() {
            let mut other = base.clone();
            other.top_font = Some(ud.id.to_string());
            let (_, ud_rgba) = paint_or_skip(other).unwrap();
            assert_ne!(row_bytes(&mincho_rgba, side, 0.18, 0.38), row_bytes(&ud_rgba, side, 0.18, 0.38));
        }
        let mut bottom = base.clone();
        bottom.bottom_font = Some(gothic.id.to_string());
        bottom.bottom_bold = false;
        let mut bottom_bold = bottom.clone();
        bottom_bold.bottom_bold = true;
        bottom_bold.bottom_font = Some("craft:biz-udp-gothic:bold".into());
        let (_, bottom_rgba) = paint_or_skip(bottom).unwrap();
        let (_, bottom_heavy) = paint_or_skip(bottom_bold).unwrap();
        assert_ne!(row_bytes(&bottom_rgba, side, 0.62, 0.86), row_bytes(&bottom_heavy, side, 0.62, 0.86));
        assert_eq!(row_bytes(&bottom_rgba, side, 0.18, 0.38), row_bytes(&bottom_heavy, side, 0.18, 0.38));
    }

    #[test]
    fn a_date_stamp_stays_on_its_page_when_that_page_is_rotated() {
        let _policy = policy();
        let dir = std::env::temp_dir().join(format!("swiftpdf-date-rotate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = crate::PdfCraftApp::new();
        let original = app.session.create_blank(200.0, 300.0, 1).unwrap();
        let original_path = dir.join("opened.pdf");
        std::fs::write(&original_path, original.as_ref()).unwrap();
        use sha2::Digest;
        let original_sha = sha2::Sha256::digest(original.as_ref());
        app.open_bytes("opened.pdf", Some(original_path.to_string_lossy().into_owned()), original.to_vec()).unwrap();
        app.set_view_only(true);
        app.grant_stamp();
        app.start_date_stamp("date.png".into(), gray_png());
        app.save_date_stamp();
        app.dialog = None;
        app.quick_tool = crate::QuickTool::DateStamp(0);
        app.begin_stamp_place(0, 0, [100.0, 80.0], 0).unwrap();
        app.commit_stamp_place();
        assert_eq!(stamp_count(&app), 1, "{:?}", app.toast);
        assert!(app.seal_store.library.history.is_empty(), "a date stamp is not a chairman approval");
        app.session.grant(pdfcraft_engine::Capability::PageRotate);
        app.session.grant(pdfcraft_engine::Capability::SaveAs);
        assert!(app.execute("page.rotate"), "{:?}", app.toast);
        let id = app.views[0].id;
        let doc = app.session.get(id).unwrap();
        assert_eq!(doc.info.pages[0].rotation, 90);
        assert_eq!(doc.info.annotations.iter().filter(|a| a.subtype == "Stamp").map(|a| a.page).collect::<Vec<_>>(), [0]);
        assert!(page_has_ink(&doc.bytes, 0), "the date stamp is still drawn after rotation");
        assert!(app.seal_store.library.history.is_empty());
        let saved = app.session.save_edited_copy_bytes(id).unwrap();
        assert_eq!(sha2::Sha256::digest(std::fs::read(&original_path).unwrap()), original_sha);
        let mut again = crate::PdfCraftApp::new();
        again.open_bytes("copy.pdf", None, saved.to_vec()).unwrap();
        let copy = again.session.get(again.views[0].id).unwrap();
        assert_eq!(copy.info.pages[0].rotation, 90);
        assert_eq!(copy.info.annotations.iter().filter(|a| a.subtype == "Stamp").count(), 1);
        assert!(page_has_ink(&copy.bytes, 0), "reopening the copy still draws the date stamp");
        assert!(app.execute("edit.undo"), "{:?}", app.toast);
        assert_eq!(app.session.get(id).unwrap().info.pages[0].rotation, 0);
        assert_eq!(stamp_count(&app), 1, "undoing the rotation leaves the date stamp");
        assert!(page_has_ink(&app.session.get(id).unwrap().bytes, 0), "undoing the rotation keeps the date stamp visible");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn page_has_ink(bytes: &[u8], page: usize) -> bool {
        let mut renderer = pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes.to_vec()), Default::default());
        let shot = renderer.render(pdfcraft_render::RenderRequest { page, scale: 2.0, ..Default::default() });
        assert!(shot.error.is_none(), "{:?}", shot.error);
        shot.rgba.chunks_exact(4).any(|px| px[0] < 230 || px[1] < 230 || px[2] < 230)
    }
}
