//! Ink effects for a chairman seal, applied to a copy of its pixels.
//!
//! The original registration PNG is not an input that this module writes. The same pixels,
//! [`SealEffects`] and seed always produce the same buffer. Callers choose the seed when a
//! placement starts or when the user asks for another pattern, not on every frame.
//!
//! [`Applied::Unchanged`] means the stamp can keep the original PNG bytes and the rectangle it
//! already computed. No re-encode is required.

use crate::seal_store::SealEffects;

/// The largest image [`crate::seal_store::preview_rgba`] will decode.
pub const MAX_PIXELS: u32 = 4_000_000;
const MAX_SIDE: u32 = 8000;

/// Why a seal image was not processed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectError {
    /// Width or height is zero, or a side is above [`MAX_SIDE`].
    BadSize,
    /// `pixels` is not `width * height * 4` bytes.
    BadLength,
    /// The source, or the padded result, would exceed [`MAX_PIXELS`].
    TooLarge,
}

/// A processed seal, or a notice that the original image should be used as it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Applied {
    /// Effects are off, or every strength is zero. Keep the original PNG and rectangle.
    Unchanged,
    /// A new RGBA buffer. The original slice is left untouched.
    Image(EffectImage),
}

/// Pixels after rotation, wear and bleed, and how the buffer sits around the source.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Degrees, positive clockwise in image space. Always inside the configured range.
    pub angle_deg: f64,
    pub source_width: u32,
    pub source_height: u32,
    /// Transparent pixels added around the source: left, top, right, bottom.
    /// The source centre stays at the centre of this buffer, so a PDF rectangle grows by
    /// [`Self::axis_scale`] about its own centre and the ink keeps its size.
    pub pad: [u32; 4],
}

impl EffectImage {
    /// How many times wider and taller this buffer is than the source.
    /// Page rotation of 90° or 270° swaps the two, the same way the placement rectangle does.
    pub fn axis_scale(&self) -> (f64, f64) {
        let sx = if self.source_width == 0 { 1.0 } else { f64::from(self.width) / f64::from(self.source_width) };
        let sy = if self.source_height == 0 { 1.0 } else { f64::from(self.height) / f64::from(self.source_height) };
        (sx, sy)
    }
}

/// Historical spread radius. Five percent of the slider is 0.4% of the short side.
/// The press slip does not use this distance; tests keep the formula stable.
#[cfg_attr(not(test), allow(dead_code))]
pub fn bleed_radius_px(strength: f64, width: u32, height: u32) -> f64 {
    let short = f64::from(width.min(height));
    (strength / 100.0) * short * 0.08
}

/// Process `pixels` (`width` by `height`, straight RGBA). The slice is not written.
pub fn apply(pixels: &[u8], width: u32, height: u32, effects: &SealEffects, seed: u64) -> Result<Applied, EffectError> {
    #[cfg(test)]
    testing::record();
    check_buffer(pixels, width, height)?;
    if effects.is_passthrough() {
        return Ok(Applied::Unchanged);
    }
    let fade = unit_range(effects.fade, 30.0);
    // v0.7.0 paints rotation and wear only. The stored bleed value and `bleed` stay so an
    // older file still loads, and so the slip can still be measured, but this path never uses it.
    let bleed_strength = painted_bleed(effects);
    let angle = if effects.manual {
        if effects.manual_deg.is_finite() { effects.manual_deg.clamp(-30.0, 30.0) } else { 0.0 }
    } else {
        angle_deg(seed, unit_range(effects.rotation_deg, 30.0))
    };
    if angle.abs() < 1.0e-9 && fade == 0.0 && bleed_strength == 0.0 {
        return Ok(Applied::Unchanged);
    }
    // Wear is the only stage that needs a full copy of an otherwise unchanged image.
    // Bleed reads that copy (or the caller's slice) and drops it before rotation allocates.
    let faded = if fade > 0.0 { Some(wear(pixels, width, height, fade, seed)) } else { None };
    let expanded = if bleed_strength > 0.0 {
        let worn = faded.as_deref().unwrap_or(pixels);
        Some(bleed(worn, width, height, bleed_strength, seed)?)
    } else {
        None
    };
    let (rgba, out_w, out_h) = if angle.abs() < 1.0e-9 {
        match (expanded, faded) {
            (Some((buf, w, h)), _) => (buf, w, h),
            (None, Some(buf)) => (buf, width, height),
            (None, None) => return Ok(Applied::Unchanged),
        }
    } else if let Some((buf, w, h)) = expanded.as_ref() {
        rotate(buf, *w, *h, angle)?
    } else if let Some(buf) = faded.as_deref() {
        rotate(buf, width, height, angle)?
    } else {
        rotate(pixels, width, height, angle)?
    };
    Ok(Applied::Image(EffectImage {
        width: out_w,
        height: out_h,
        rgba,
        angle_deg: angle,
        source_width: width,
        source_height: height,
        pad: pads(width, height, out_w, out_h),
    }))
}

/// The spread slider is kept on disk. Placement never paints it.
fn painted_bleed(effects: &SealEffects) -> f64 {
    let _ = effects.bleed;
    0.0
}

fn unit_range(value: f64, max: f64) -> f64 {
    if value.is_finite() { value.clamp(0.0, max) } else { 0.0 }
}

fn check_buffer(pixels: &[u8], width: u32, height: u32) -> Result<(), EffectError> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(EffectError::BadSize);
    }
    let count = width.checked_mul(height).ok_or(EffectError::TooLarge)?;
    if count > MAX_PIXELS {
        return Err(EffectError::TooLarge);
    }
    let bytes = (count as usize).checked_mul(4).ok_or(EffectError::TooLarge)?;
    if pixels.len() != bytes {
        return Err(EffectError::BadLength);
    }
    Ok(())
}

fn check_extent(width: u32, height: u32) -> Result<(), EffectError> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(EffectError::BadSize);
    }
    let count = width.checked_mul(height).ok_or(EffectError::TooLarge)?;
    if count > MAX_PIXELS {
        return Err(EffectError::TooLarge);
    }
    Ok(())
}

/// Degrees in `[-range, range]`. The top 53 bits of the mix map onto `[0, 1)`.
fn angle_deg(seed: u64, range: f64) -> f64 {
    if range <= 0.0 {
        return 0.0;
    }
    let unit = (mix(seed) >> 11) as f64 * (1.0 / ((1u64 << 53) as f64));
    (unit * 2.0 - 1.0) * range
}

fn pads(src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> [u32; 4] {
    let extra_w = out_w.saturating_sub(src_w);
    let extra_h = out_h.saturating_sub(src_h);
    let left = extra_w / 2;
    let top = extra_h / 2;
    [left, top, extra_w - left, extra_h - top]
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn noise01(seed: u64, x: i32, y: i32) -> f64 {
    let h = mix(seed ^ (x as u32 as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (y as u32 as u64).wrapping_mul(0xBF58476D1CE4E5B9));
    (h >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))
}

/// Smooth noise in 0..=1. A cell is 12 pixels, with a second, wider octave.
fn low_noise(seed: u64, x: i32, y: i32) -> f64 {
    let fine = value_noise(seed, x, y, 12);
    let wide = value_noise(seed ^ 0xA5A5_A5A5_A5A5_A5A5, x, y, 28);
    (fine * 0.65 + wide * 0.35).clamp(0.0, 1.0)
}

fn value_noise(seed: u64, x: i32, y: i32, cell: i32) -> f64 {
    let cell = cell.max(1);
    let x0 = x.div_euclid(cell);
    let y0 = y.div_euclid(cell);
    let tx = f64::from(x - x0 * cell) / f64::from(cell);
    let ty = f64::from(y - y0 * cell) / f64::from(cell);
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sy = ty * ty * (3.0 - 2.0 * ty);
    let v00 = noise01(seed, x0, y0);
    let v10 = noise01(seed, x0 + 1, y0);
    let v01 = noise01(seed, x0, y0 + 1);
    let v11 = noise01(seed, x0 + 1, y0 + 1);
    let a = v00 + (v10 - v00) * sx;
    let b = v01 + (v11 - v01) * sx;
    a + (b - a) * sy
}

fn index(width: u32, x: u32, y: u32) -> usize {
    (y as usize).saturating_mul(width as usize).saturating_mul(4).saturating_add((x as usize).saturating_mul(4))
}

/// How strongly `strength` (0–30) wears the ink. Zero leaves every pixel alone.
fn wear_amount(strength: f64) -> f64 {
    (strength / 30.0).clamp(0.0, 1.0)
}

/// One press of the seal. The axis is the weak-to-strong direction, chosen once from the seed.
#[derive(Clone, Copy)]
struct Press {
    axis_x: f64,
    axis_y: f64,
    center_light: bool,
}

fn press_from_seed(seed: u64) -> Press {
    let turn = (mix(seed) >> 11) as f64 * (1.0 / ((1u64 << 53) as f64)) * std::f64::consts::TAU;
    Press { axis_x: turn.cos(), axis_y: turn.sin(), center_light: mix(seed ^ 0xB155) & 1 == 0 }
}

/// Keep-factor from the broad pressure field. One side of the seal stays darker. The centre or
/// the rim loses a little more, so the impression is uneven without wiping out a whole half.
fn press_keep(amount: f64, width: u32, height: u32, x: u32, y: u32, press: Press) -> f64 {
    let nx = (f64::from(x) + 0.5) / f64::from(width.max(1)) * 2.0 - 1.0;
    let ny = (f64::from(y) + 0.5) / f64::from(height.max(1)) * 2.0 - 1.0;
    let span = nx * press.axis_x + ny * press.axis_y;
    let weak = ((0.65 - span) / 1.6).clamp(0.0, 1.0);
    let radial = (nx * nx + ny * ny).clamp(0.0, 1.0);
    let (bowl, bowl_amp) = if press.center_light { (1.0 - radial, 0.10) } else { (radial, 0.08) };
    (1.0 - amount * 0.34 * weak) * (1.0 - amount * bowl_amp * bowl)
}

struct WearInputs {
    amount: f64,
    edge: bool,
    pressure: f64,
    shade: f64,
    blob: f64,
    streak: f64,
    grain: f64,
}

/// Multiplier for one pixel's alpha. Pressure covers the whole impression, including the rim.
/// Blobs and streaks open larger gaps in the body. Grain only speckles. A pixel touching
/// transparency is nicked, not erased, so a thin stroke and the outer circle stay readable.
fn wear_factor(sample: WearInputs) -> f64 {
    let amount = if sample.amount.is_finite() { sample.amount.clamp(0.0, 1.0) } else { 0.0 };
    if amount <= 0.0 {
        return 1.0;
    }
    let shade_amp = if sample.edge { 0.10 } else { 0.16 };
    let mut factor = sample.pressure.clamp(0.0, 1.0) * (1.0 - amount * shade_amp * sample.shade.clamp(0.0, 1.0));
    if sample.edge {
        if sample.blob < 0.22 * amount {
            factor = factor.min(0.08);
        }
        if sample.streak < 0.05 * amount {
            factor = factor.min(0.36);
        }
        if sample.grain < 0.035 * amount {
            factor = factor.min(0.08);
        }
    } else if sample.blob < 0.22 * amount {
        factor = factor.min(0.12);
    } else if sample.streak < 0.14 * amount {
        factor = factor.min(0.28);
    } else if sample.grain < 0.045 * amount {
        factor = factor.min(0.15);
    }
    factor.clamp(0.0, 1.0)
}

/// True when transparency is one step away on a cardinal axis. Only the hairline is softened;
/// the rest of a stroke, including most of a frame, takes the full impression.
fn near_edge(src: &[u8], width: u32, height: u32, x: u32, y: u32) -> bool {
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let xx = x as i32 + dx;
        let yy = y as i32 + dy;
        if xx < 0 || yy < 0 || xx >= width as i32 || yy >= height as i32 {
            return true;
        }
        let i = index(width, xx as u32, yy as u32);
        if src.get(i + 3).copied().unwrap_or(0) < 16 {
            return true;
        }
    }
    false
}

/// Impression wear: a broad pressure bias, medium gaps and streaks, then fine grain.
/// The source slice is copied; RGB and already-clear pixels stay as they are.
fn wear(src: &[u8], width: u32, height: u32, strength: f64, seed: u64) -> Vec<u8> {
    let mut out = src.to_vec();
    let amount = wear_amount(strength);
    if amount <= 0.0 {
        return out;
    }
    let press = press_from_seed(seed);
    let turn = (mix(seed ^ 0x5712) >> 11) as f64 * (1.0 / ((1u64 << 53) as f64)) * std::f64::consts::TAU;
    let (along_x, along_y) = (turn.cos(), turn.sin());
    for y in 0..height {
        for x in 0..width {
            let i = index(width, x, y);
            let alpha = out.get(i + 3).copied().unwrap_or(0);
            if alpha == 0 {
                continue;
            }
            let xi = x as i32;
            let yi = y as i32;
            let fx = f64::from(xi);
            let fy = f64::from(yi);
            let along = (fx * along_x + fy * along_y).round() as i32;
            let across = ((fx * -along_y + fy * along_x) / 9.0).round() as i32;
            let factor = wear_factor(WearInputs {
                amount,
                edge: near_edge(src, width, height, x, y),
                pressure: press_keep(amount, width, height, x, y, press),
                shade: low_noise(seed ^ 0x51ED, xi, yi),
                blob: value_noise(seed ^ 0xC117, xi, yi, 20),
                streak: value_noise(seed ^ 0x5712, along.div_euclid(40), across, 1),
                grain: noise01(seed ^ 0x6A11, xi, yi),
            });
            let mut next = f64::from(alpha) * factor;
            // A punched pixel stays claimed. Bleed then does not flood the chip with new ink.
            if alpha >= 16 && next < 1.0 {
                next = 1.0;
            }
            if let Some(slot) = out.get_mut(i + 3) {
                *slot = byte_u8(next);
            }
        }
    }
    out
}

/// One drag direction for the whole impression. Low-frequency noise only wobbles it.
fn smear_axis(seed: u64) -> (f64, f64) {
    let turn = (mix(seed ^ 0x5EED) >> 11) as f64 * (1.0 / ((1u64 << 53) as f64)) * std::f64::consts::TAU;
    (turn.cos(), turn.sin())
}

fn smear_amount(strength: f64) -> f64 {
    (strength / 30.0).clamp(0.0, 1.0)
}

/// How far an active patch may slip, in source pixels. Strength 10 stays near 2px, 20 near 4px,
/// and 30 near 6px, so a 1254px seal does not grow a second outline.
fn slip_px(strength: f64) -> f64 {
    let t = smear_amount(strength);
    if t <= 0.0 {
        return 0.0;
    }
    t.powf(1.15) * 6.2
}

/// Patches below this stay put. The bar drops as strength rises, but the back of the drag stays quiet.
fn slip_gate(amount: f64) -> f64 {
    0.52 - 0.18 * amount.clamp(0.0, 1.0)
}

/// Where the slip is allowed. The back of the drag is zero, and the forward side is broken into
/// a few low-frequency blobs so the whole outline does not move together.
fn slip_mask(seed: u64, x: i32, y: i32, width: u32, height: u32) -> f64 {
    let cx = (f64::from(width) - 1.0) * 0.5;
    let cy = (f64::from(height) - 1.0) * 0.5;
    let dx = f64::from(x) - cx;
    let dy = f64::from(y) - cy;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let (axis_x, axis_y) = smear_axis(seed);
    let facing = ((dx / len) * axis_x + (dy / len) * axis_y).clamp(-1.0, 1.0);
    if facing < -0.15 {
        return 0.0;
    }
    let side = ((facing + 0.15) / 1.15).clamp(0.0, 1.0);
    let cell = (width.max(height) as i32 / 3).clamp(48, 360);
    let blob = value_noise(seed ^ 0xB10D, x, y, cell);
    (side * blob).clamp(0.0, 1.0)
}

fn slip_dir(seed: u64, x: i32, y: i32) -> (f64, f64) {
    let (axis_x, axis_y) = smear_axis(seed);
    let wobble = (low_noise(seed ^ 0xA11, x, y) - 0.5) * 0.96;
    (axis_x * wobble.cos() - axis_y * wobble.sin(), axis_x * wobble.sin() + axis_y * wobble.cos())
}

fn alpha_near(src: &[u8], width: u32, height: u32, x: f64, y: f64) -> u8 {
    if !x.is_finite() || !y.is_finite() {
        return 0;
    }
    let ix = x.round();
    let iy = y.round();
    if !(ix >= f64::from(i32::MIN) && ix <= f64::from(i32::MAX) && iy >= f64::from(i32::MIN) && iy <= f64::from(i32::MAX)) {
        return 0;
    }
    pixel_at(src, width, height, ix as i32, iy as i32).map(|px| px[3]).unwrap_or(0)
}

/// Slip active patches a few pixels along one press direction. The trailing side of a stroke
/// loses ink and the leading side gains a short, fading smear. A walk of at most about seven
/// pixels replaces the full-frame distance transform. Wear chips (alpha ≤ 40) are not sources
/// and are not painted over.
fn bleed(src: &[u8], width: u32, height: u32, strength: f64, seed: u64) -> Result<(Vec<u8>, u32, u32), EffectError> {
    let shift = slip_px(strength);
    if !shift.is_finite() || shift < 0.4 {
        return Ok((src.to_vec(), width, height));
    }
    let margin = limited_u32(shift.ceil(), 16).ok_or(EffectError::TooLarge)?;
    let out_w = width.checked_add(margin).and_then(|v| v.checked_add(margin)).ok_or(EffectError::TooLarge)?;
    let out_h = height.checked_add(margin).and_then(|v| v.checked_add(margin)).ok_or(EffectError::TooLarge)?;
    check_extent(out_w, out_h)?;
    let count = (out_w as usize).saturating_mul(out_h as usize);
    let mut out = vec![0u8; count.saturating_mul(4)];
    for y in 0..height {
        for x in 0..width {
            let Some(px) = pixel_at(src, width, height, x as i32, y as i32) else { continue };
            if px[3] == 0 {
                continue;
            }
            put(&mut out, out_w, x.saturating_add(margin), y.saturating_add(margin), px);
        }
    }
    let amount = smear_amount(strength);
    slip_patches(&mut InkAdd { out: &mut out, src, out_w, out_h, width, height, margin: margin as i32 }, shift, amount, slip_gate(amount), seed);
    Ok((out, out_w, out_h))
}

struct InkAdd<'a> {
    out: &'a mut [u8],
    src: &'a [u8],
    out_w: u32,
    out_h: u32,
    width: u32,
    height: u32,
    margin: i32,
}

struct Smear {
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    local: f64,
    gain: f64,
    color: [u8; 4],
}

fn slip_patches(ink: &mut InkAdd<'_>, shift: f64, amount: f64, gate_at: f64, seed: u64) {
    let width = ink.width;
    let height = ink.height;
    for y in 0..height {
        for x in 0..width {
            let Some(px) = pixel_at(ink.src, width, height, x as i32, y as i32) else { continue };
            if px[3] <= 40 {
                continue;
            }
            let gate = slip_mask(seed, x as i32, y as i32, width, height);
            if gate < gate_at {
                continue;
            }
            let (dx, dy) = slip_dir(seed, x as i32, y as i32);
            let local = shift * (0.55 + 0.45 * gate);
            if local < 0.2 {
                continue;
            }
            let smear = Smear { x: f64::from(x), y: f64::from(y), dx, dy, local, gain: amount * gate, color: px };
            if alpha_near(ink.src, width, height, smear.x + dx, smear.y + dy) < 16 {
                thin_at(ink, smear.x, smear.y, 1.0 - 0.48 * smear.gain);
                let mut step = 1.0;
                while step <= local && step <= 8.0 {
                    let bx = smear.x - dx * step;
                    let by = smear.y - dy * step;
                    if alpha_near(ink.src, width, height, bx, by) <= 40 {
                        break;
                    }
                    let t = (1.0 - step / local).clamp(0.0, 1.0);
                    thin_at(ink, bx, by, 1.0 - 0.52 * smear.gain * t);
                    step += 1.0;
                }
                lay_smear(ink, &smear);
            }
        }
    }
}

fn thin_at(ink: &mut InkAdd<'_>, x: f64, y: f64, keep: f64) {
    if !x.is_finite() || !y.is_finite() || !keep.is_finite() {
        return;
    }
    let ix = x.round();
    let iy = y.round();
    if !(ix >= f64::from(i32::MIN) && ix <= f64::from(i32::MAX) && iy >= f64::from(i32::MIN) && iy <= f64::from(i32::MAX)) {
        return;
    }
    let ix = ix as i32;
    let iy = iy as i32;
    let Some(px) = pixel_at(ink.src, ink.width, ink.height, ix, iy) else { return };
    if px[3] <= 40 {
        return;
    }
    let alpha = byte_u8(f64::from(px[3]) * keep).max(1);
    let ox = ix + ink.margin;
    let oy = iy + ink.margin;
    if ox < 0 || oy < 0 || ox >= ink.out_w as i32 || oy >= ink.out_h as i32 {
        return;
    }
    let Some(cur) = pixel_at(ink.out, ink.out_w, ink.out_h, ox, oy) else { return };
    if cur[3] <= alpha {
        return;
    }
    put(ink.out, ink.out_w, ox as u32, oy as u32, [px[0], px[1], px[2], alpha]);
}

fn lay_smear(ink: &mut InkAdd<'_>, smear: &Smear) {
    let alpha = f64::from(smear.color[3]);
    let rgb = [smear.color[0], smear.color[1], smear.color[2]];
    let ox = smear.x + f64::from(ink.margin.max(0));
    let oy = smear.y + f64::from(ink.margin.max(0));
    let root = alpha * (0.78 + 0.18 * smear.gain);
    add_clear(ink, ox + smear.dx, oy + smear.dy, rgb, root);
    let mut dist = 2.0;
    while dist <= smear.local + 0.01 && dist <= 8.0 {
        let t = (1.0 - dist / smear.local).clamp(0.0, 1.0);
        let add = alpha * (0.22 + 0.20 * smear.gain) * t.powf(1.35);
        add_clear(ink, ox + smear.dx * dist, oy + smear.dy * dist, rgb, add);
        dist += 1.0;
    }
}

fn add_clear(ink: &mut InkAdd<'_>, x: f64, y: f64, rgb: [u8; 3], add: f64) {
    if !x.is_finite() || !y.is_finite() || add < 0.5 {
        return;
    }
    let x0 = x.floor();
    let y0 = y.floor();
    if !(x0 >= f64::from(i32::MIN) && y0 >= f64::from(i32::MIN)) {
        return;
    }
    let x0 = x0 as i32;
    let y0 = y0 as i32;
    let tx = (x - f64::from(x0)).clamp(0.0, 1.0);
    let ty = (y - f64::from(y0)).clamp(0.0, 1.0);
    let corners = [
        (x0, y0, (1.0 - tx) * (1.0 - ty)),
        (x0.saturating_add(1), y0, tx * (1.0 - ty)),
        (x0, y0.saturating_add(1), (1.0 - tx) * ty),
        (x0.saturating_add(1), y0.saturating_add(1), tx * ty),
    ];
    for (ix, iy, weight) in corners {
        if weight < 1.0e-4 {
            continue;
        }
        let sx = ix - ink.margin;
        let sy = iy - ink.margin;
        let blocked = sx >= 0
            && sy >= 0
            && sx < ink.width as i32
            && sy < ink.height as i32
            && pixel_at(ink.src, ink.width, ink.height, sx, sy).is_some_and(|px| px[3] > 0);
        if blocked || ix < 0 || iy < 0 || ix >= ink.out_w as i32 || iy >= ink.out_h as i32 {
            continue;
        }
        let Some(cur) = pixel_at(ink.out, ink.out_w, ink.out_h, ix, iy) else { continue };
        let rgb = if cur[3] == 0 { rgb } else { [cur[0], cur[1], cur[2]] };
        let alpha = byte_u8(f64::from(cur[3]) + add * weight);
        put(ink.out, ink.out_w, ix as u32, iy as u32, [rgb[0], rgb[1], rgb[2], alpha]);
    }
}

fn byte_u8(value: f64) -> u8 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= 255.0 {
        return 255;
    }
    let rounded = value.round();
    if rounded >= 255.0 { 255 } else { rounded as u8 }
}

fn limited_u32(value: f64, max: u32) -> Option<u32> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    if value >= f64::from(max) {
        return Some(max);
    }
    Some(value as u32)
}

#[cfg(test)]
fn plane_slot(width: u32, height: u32, x: i32, y: i32) -> Option<usize> {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return None;
    }
    let x = x as usize;
    let y = y as usize;
    Some(y.saturating_mul(width as usize).saturating_add(x))
}

fn pixel_at(src: &[u8], width: u32, height: u32, x: i32, y: i32) -> Option<[u8; 4]> {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return None;
    }
    let i = index(width, x as u32, y as u32);
    Some([
        src.get(i).copied().unwrap_or(0),
        src.get(i + 1).copied().unwrap_or(0),
        src.get(i + 2).copied().unwrap_or(0),
        src.get(i + 3).copied().unwrap_or(0),
    ])
}

fn put(out: &mut [u8], width: u32, x: u32, y: u32, px: [u8; 4]) {
    let i = index(width, x, y);
    if let Some(slot) = out.get_mut(i..i + 4) {
        slot.copy_from_slice(&px);
    }
}

fn rotate(src: &[u8], width: u32, height: u32, angle_deg: f64) -> Result<(Vec<u8>, u32, u32), EffectError> {
    let rad = angle_deg.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    let out_w = rotated_side(width, height, cos.abs(), sin.abs())?;
    let out_h = rotated_side(height, width, cos.abs(), sin.abs())?;
    check_extent(out_w, out_h)?;
    let mut out = vec![0u8; (out_w as usize).saturating_mul(out_h as usize).saturating_mul(4)];
    let cx_s = (f64::from(width) - 1.0) * 0.5;
    let cy_s = (f64::from(height) - 1.0) * 0.5;
    let cx_d = (f64::from(out_w) - 1.0) * 0.5;
    let cy_d = (f64::from(out_h) - 1.0) * 0.5;
    for y in 0..out_h {
        for x in 0..out_w {
            let dx = f64::from(x) - cx_d;
            let dy = f64::from(y) - cy_d;
            let sx = cos * dx + sin * dy + cx_s;
            let sy = -sin * dx + cos * dy + cy_s;
            put(&mut out, out_w, x, y, sample(src, width, height, sx, sy));
        }
    }
    Ok((out, out_w, out_h))
}

fn rotated_side(width: u32, height: u32, abs_cos: f64, abs_sin: f64) -> Result<u32, EffectError> {
    // The bounding box of the rectangle, plus one pixel on every side. The extra pixel keeps
    // the bilinear footprint of a corner inside the buffer instead of cutting it off.
    let side = f64::from(width) * abs_cos + f64::from(height) * abs_sin + 2.0;
    if !side.is_finite() || side < 1.0 || side > f64::from(MAX_SIDE) {
        return Err(EffectError::BadSize);
    }
    Ok(side.ceil() as u32)
}

/// Straight RGBA sample. Coverage is interpolated in premultiplied form, then divided back,
/// so a red edge over transparency stays red instead of darkening.
fn sample(src: &[u8], width: u32, height: u32, x: f64, y: f64) -> [u8; 4] {
    if !x.is_finite() || !y.is_finite() {
        return [0, 0, 0, 0];
    }
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = x - f64::from(x0);
    let ty = y - f64::from(y0);
    let mut acc = [0.0; 4];
    for (iy, wy) in [(y0, 1.0 - ty), (y0 + 1, ty)] {
        for (ix, wx) in [(x0, 1.0 - tx), (x0 + 1, tx)] {
            let w = wx * wy;
            if w <= 0.0 {
                continue;
            }
            let Some(px) = pixel_at(src, width, height, ix, iy) else { continue };
            let a = f64::from(px[3]) / 255.0;
            let covered = a * w;
            acc[0] += f64::from(px[0]) / 255.0 * covered;
            acc[1] += f64::from(px[1]) / 255.0 * covered;
            acc[2] += f64::from(px[2]) / 255.0 * covered;
            acc[3] += covered;
        }
    }
    if acc[3] <= 1.0 / 512.0 {
        return [0, 0, 0, 0];
    }
    let channel = |v: f64| (v / acc[3] * 255.0).round().clamp(0.0, 255.0) as u8;
    [channel(acc[0]), channel(acc[1]), channel(acc[2]), (acc[3] * 255.0).round().clamp(0.0, 255.0) as u8]
}

/// Counts [`apply`] calls so a test can show that a frame, a window move, or a slider drag
/// did not rebuild the image.
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::Cell;

    thread_local! {
        static APPLY_CALLS: Cell<u64> = const { Cell::new(0) };
    }

    pub(crate) fn reset() {
        APPLY_CALLS.with(|calls| calls.set(0));
    }

    pub(crate) fn calls() -> u64 {
        APPLY_CALLS.with(|calls| calls.get())
    }

    pub(crate) fn record() {
        APPLY_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));
    }
}

/// The previous per-ink window. Kept so a test can time it on a small disk. A 1254² seal is
/// not run through this path.
#[cfg(test)]
fn bleed_splat(src: &[u8], width: u32, height: u32, strength: f64, seed: u64) -> Result<(Vec<u8>, u32, u32), EffectError> {
    let radius = bleed_radius_px(strength, width, height);
    if !radius.is_finite() || radius < 0.5 {
        return Ok((src.to_vec(), width, height));
    }
    let margin = limited_u32(radius.ceil(), 512).ok_or(EffectError::TooLarge)?;
    let out_w = width.checked_add(margin).and_then(|v| v.checked_add(margin)).ok_or(EffectError::TooLarge)?;
    let out_h = height.checked_add(margin).and_then(|v| v.checked_add(margin)).ok_or(EffectError::TooLarge)?;
    check_extent(out_w, out_h)?;
    let count = (out_w as usize).saturating_mul(out_h as usize);
    let mut out = vec![0u8; count.saturating_mul(4)];
    let mut nearest = vec![f32::MAX; count];
    let reach = i32::try_from(margin).unwrap_or(0);
    for y in 0..height {
        for x in 0..width {
            let Some(px) = pixel_at(src, width, height, x as i32, y as i32) else { continue };
            if px[3] == 0 {
                continue;
            }
            put(&mut out, out_w, x + margin, y + margin, px);
            let mut dy = -reach;
            while dy <= reach {
                let mut dx = -reach;
                while dx <= reach {
                    let dist = ((dx * dx + dy * dy) as f64).sqrt();
                    if dist > 0.0 && dist <= radius {
                        let at_x = x as i32 + dx + reach;
                        let at_y = y as i32 + dy + reach;
                        if pixel_at(src, width, height, at_x - reach, at_y - reach).is_some_and(|orig| orig[3] > 0) {
                            dx += 1;
                            continue;
                        }
                        let Some(slot) = plane_slot(out_w, out_h, at_x, at_y) else {
                            dx += 1;
                            continue;
                        };
                        let Some(best) = nearest.get_mut(slot) else {
                            dx += 1;
                            continue;
                        };
                        if dist >= f64::from(*best) {
                            dx += 1;
                            continue;
                        }
                        let falloff = (1.0 - dist / radius).clamp(0.0, 1.0);
                        let wobble = 0.85 + 0.15 * low_noise(seed ^ 0xB10D, at_x, at_y);
                        let alpha = byte_u8(f64::from(px[3]) * falloff * wobble);
                        if alpha == 0 {
                            dx += 1;
                            continue;
                        }
                        *best = dist as f32;
                        let Ok(px_x) = u32::try_from(at_x) else {
                            dx += 1;
                            continue;
                        };
                        let Ok(px_y) = u32::try_from(at_y) else {
                            dx += 1;
                            continue;
                        };
                        put(&mut out, out_w, px_x, px_y, [px[0], px[1], px[2], alpha]);
                    }
                    dx += 1;
                }
                dy += 1;
            }
        }
    }
    Ok((out, out_w, out_h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        let mut buf = Vec::new();
        for _ in 0..w.saturating_mul(h) {
            buf.extend_from_slice(&px);
        }
        buf
    }

    fn at(buf: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = index(w, x, y);
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn off_and_zero_strengths_keep_the_original() {
        let src = solid(8, 6, [10, 20, 30, 255]);
        let before = src.clone();
        let off = apply(&src, 8, 6, &SealEffects::off(), 7).unwrap();
        assert!(matches!(off, Applied::Unchanged));
        let enabled_zero = SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
        assert!(matches!(apply(&src, 8, 6, &enabled_zero, 7).unwrap(), Applied::Unchanged));
        assert_eq!(src, before);
    }

    #[test]
    fn stored_bleed_does_not_change_the_image() {
        let src = disk(48);
        let with_bleed = SealEffects { enabled: true, rotation_deg: 2.0, fade: 10.0, bleed: 30.0, manual: false, manual_deg: 0.0 };
        let without = SealEffects { enabled: true, rotation_deg: 2.0, fade: 10.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
        assert_eq!(apply(&src, 48, 48, &with_bleed, 4).unwrap(), apply(&src, 48, 48, &without, 4).unwrap());
        let only = SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 30.0, manual: false, manual_deg: 0.0 };
        assert!(only.is_passthrough());
        assert!(matches!(apply(&src, 48, 48, &only, 4).unwrap(), Applied::Unchanged));
    }

    #[test]
    fn rejected_sizes_allocate_nothing_useful() {
        let px = [0u8, 0, 0, 255];
        assert_eq!(apply(&px, 0, 1, &SealEffects::off(), 1), Err(EffectError::BadSize));
        assert_eq!(apply(&px, 8001, 1, &SealEffects::off(), 1), Err(EffectError::BadSize));
        assert_eq!(apply(&[0, 0, 0], 1, 1, &SealEffects::off(), 1), Err(EffectError::BadLength));
        assert_eq!(apply(&[], 2001, 2001, &SealEffects::off(), 1), Err(EffectError::TooLarge));
    }

    #[test]
    fn the_same_seed_matches_and_another_seed_does_not() {
        let src = solid(24, 24, [180, 20, 20, 255]);
        let effects = SealEffects::enabled_preset();
        let a = apply(&src, 24, 24, &effects, 11).unwrap();
        let b = apply(&src, 24, 24, &effects, 11).unwrap();
        let c = apply(&src, 24, 24, &effects, 99).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        let before = src.clone();
        assert_eq!(src, before);
    }

    #[test]
    fn rotation_stays_inside_its_range_and_keeps_the_corners() {
        let src = solid(32, 32, [200, 30, 30, 255]);
        for (range, seed) in [(0.0, 1u64), (2.0, 3), (2.0, 8), (10.0, 5), (10.0, 42), (30.0, 5), (30.0, 42)] {
            let effects = SealEffects { enabled: true, rotation_deg: range, fade: 0.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
            let applied = apply(&src, 32, 32, &effects, seed).unwrap();
            if range == 0.0 {
                assert!(matches!(applied, Applied::Unchanged));
                continue;
            }
            let Applied::Image(image) = applied else { panic!("rotated") };
            assert!(image.angle_deg.abs() <= range + 1.0e-9, "{}", image.angle_deg);
            assert!(image.width >= 32 && image.height >= 32);
            if image.angle_deg.abs() > 0.2 {
                assert!(image.width > 32 && image.height > 32, "range {range} seed {seed} {:?}", (image.width, image.height));
                assert_eq!(at(&image.rgba, image.width, 0, 0)[3], 0, "the new corner stays transparent");
            }
            let rad = image.angle_deg.to_radians();
            let (cos, sin) = (rad.cos(), rad.sin());
            let cx_s = 15.5;
            let cx_d = (f64::from(image.width) - 1.0) * 0.5;
            let cy_d = (f64::from(image.height) - 1.0) * 0.5;
            for (sx, sy) in [(0.0, 0.0), (31.0, 0.0), (0.0, 31.0), (31.0, 31.0)] {
                let x = sx - cx_s;
                let y = sy - cx_s;
                let xd = cos * x - sin * y + cx_d;
                let yd = sin * x + cos * y + cy_d;
                assert!(
                    xd >= -0.6 && yd >= -0.6 && xd < f64::from(image.width) + 0.6 && yd < f64::from(image.height) + 0.6,
                    "corner outside {xd},{yd}"
                );
                let vx = xd - cx_d;
                let vy = yd - cy_d;
                let len = (vx * vx + vy * vy).sqrt().max(1.0e-6);
                let ix = (xd - vx / len * 1.5).round().clamp(0.0, f64::from(image.width - 1)) as u32;
                let iy = (yd - vy / len * 1.5).round().clamp(0.0, f64::from(image.height - 1)) as u32;
                let ink = at(&image.rgba, image.width, ix, iy);
                assert!(ink[3] > 240, "corner ink missing at {ix},{iy} angle {}", image.angle_deg);
                assert_eq!([ink[0], ink[1], ink[2]], [200, 30, 30]);
            }
            assert_eq!(image.pad[0] + image.pad[2], image.width - 32);
            assert_eq!(image.pad[1] + image.pad[3], image.height - 32);
            let (sx, sy) = image.axis_scale();
            assert!((sx - f64::from(image.width) / 32.0).abs() < 1e-9);
            assert!((sy - f64::from(image.height) / 32.0).abs() < 1e-9);
        }
    }

    #[test]
    fn both_signs_of_rotation_occur() {
        let src = solid(16, 16, [1, 2, 3, 255]);
        for range in [10.0, 30.0] {
            let effects = SealEffects { enabled: true, rotation_deg: range, fade: 0.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
            let mut positive = false;
            let mut negative = false;
            for seed in 0..40 {
                let Applied::Image(image) = apply(&src, 16, 16, &effects, seed).unwrap() else { continue };
                assert!(image.angle_deg.abs() <= range + 1.0e-9, "{}", image.angle_deg);
                positive |= image.angle_deg > 0.5;
                negative |= image.angle_deg < -0.5;
            }
            assert!(positive && negative, "range {range}");
        }
    }

    #[test]
    fn manual_angles_stay_fixed_when_the_seed_changes() {
        let src = solid(32, 32, [200, 30, 30, 255]);
        let before = src.clone();
        for deg in [-30.0, -15.0, 0.0, 15.0, 30.0] {
            let effects = SealEffects { enabled: true, rotation_deg: 30.0, fade: 0.0, bleed: 3.0, manual: true, manual_deg: deg };
            let first = apply(&src, 32, 32, &effects, 1).unwrap();
            let second = apply(&src, 32, 32, &effects, 99).unwrap();
            assert_eq!(first, second, "{deg}");
            assert_eq!(src, before);
            if deg == 0.0 {
                assert!(matches!(first, Applied::Unchanged));
                continue;
            }
            let Applied::Image(image) = first else { panic!("rotated") };
            assert!((image.angle_deg - deg).abs() < 1.0e-9, "{}", image.angle_deg);
            assert!(image.width > 32 && image.height > 32);
            assert_eq!(at(&image.rgba, image.width, 0, 0)[3], 0);
            let rad = image.angle_deg.to_radians();
            let (cos, sin) = (rad.cos(), rad.sin());
            let cx_s = 15.5;
            let cx_d = (f64::from(image.width) - 1.0) * 0.5;
            let cy_d = (f64::from(image.height) - 1.0) * 0.5;
            for (sx, sy) in [(0.0, 0.0), (31.0, 0.0), (0.0, 31.0), (31.0, 31.0)] {
                let x = sx - cx_s;
                let y = sy - cx_s;
                let xd = cos * x - sin * y + cx_d;
                let yd = sin * x + cos * y + cy_d;
                assert!(
                    xd >= -0.6 && yd >= -0.6 && xd < f64::from(image.width) + 0.6 && yd < f64::from(image.height) + 0.6,
                    "corner outside {xd},{yd}"
                );
                let vx = xd - cx_d;
                let vy = yd - cy_d;
                let len = (vx * vx + vy * vy).sqrt().max(1.0e-6);
                let ix = (xd - vx / len * 1.5).round().clamp(0.0, f64::from(image.width - 1)) as u32;
                let iy = (yd - vy / len * 1.5).round().clamp(0.0, f64::from(image.height - 1)) as u32;
                let ink = at(&image.rgba, image.width, ix, iy);
                assert!(ink[3] > 240, "corner ink missing at {ix},{iy} angle {}", image.angle_deg);
            }
        }
    }

    #[test]
    fn an_old_switched_off_file_ignores_leftover_wear_and_rotation() {
        let src = solid(8, 8, [9, 8, 7, 255]);
        let legacy = SealEffects { enabled: false, rotation_deg: 2.0, fade: 10.0, bleed: 5.0, manual: false, manual_deg: 0.0 };
        assert!(legacy.legacy_off());
        assert!(legacy.is_passthrough());
        assert!(matches!(apply(&src, 8, 8, &legacy, 3).unwrap(), Applied::Unchanged));
        let on = SealEffects { enabled: true, rotation_deg: 2.0, fade: 10.0, bleed: 5.0, manual: false, manual_deg: 0.0 };
        assert!(!on.is_passthrough());
        assert!(matches!(apply(&src, 8, 8, &on, 3).unwrap(), Applied::Image(_)));
    }

    #[test]
    fn a_turn_that_exceeds_the_pixel_cap_is_refused_before_the_buffer_is_filled() {
        let src = solid(3000, 100, [1, 2, 3, 255]);
        let before = src.clone();
        let effects = SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 0.0, manual: true, manual_deg: 30.0 };
        assert_eq!(apply(&src, 3000, 100, &effects, 1), Err(EffectError::TooLarge));
        assert_eq!(src, before);
    }

    #[test]
    fn a_red_edge_does_not_darken_when_it_rotates() {
        let mut src = solid(24, 24, [0, 0, 0, 0]);
        for y in 0..24 {
            for x in 0..12 {
                let i = index(24, x, y);
                src[i] = 255;
                src[i + 3] = 255;
            }
        }
        let effects = SealEffects { enabled: true, rotation_deg: 10.0, fade: 0.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
        let Applied::Image(image) = apply(&src, 24, 24, &effects, 5).unwrap() else { panic!("angle") };
        assert!(image.angle_deg.abs() > 1.0, "{}", image.angle_deg);
        let mut blended = 0;
        for y in 0..image.height {
            for x in 0..image.width {
                let px = at(&image.rgba, image.width, x, y);
                if px[3] == 0 {
                    assert_eq!([px[0], px[1], px[2]], [0, 0, 0]);
                    continue;
                }
                assert!(px[0] >= 250 && px[1] <= 2 && px[2] <= 2, "{px:?}");
                if (40..230).contains(&px[3]) {
                    blended += 1;
                }
            }
        }
        assert!(blended > 0, "expected a partial edge");
    }

    #[test]
    fn wear_biases_pressure_and_nicks_the_rim_without_erasing_a_thin_stroke() {
        let src = solid(96, 96, [180, 20, 20, 255]);
        let before = src.clone();
        let press = press_from_seed(4);
        let mut holes = [0.0; 3];
        let mut bias = [0.0; 3];
        let mut rim_open = [0.0; 3];
        let mut rim_gap = [0.0; 3];
        for (slot, strength) in [10.0, 20.0, 30.0].into_iter().enumerate() {
            let effects = SealEffects { enabled: true, rotation_deg: 0.0, fade: strength, bleed: 0.0, manual: false, manual_deg: 0.0 };
            let Applied::Image(image) = apply(&src, 96, 96, &effects, 4).unwrap() else { panic!("fade") };
            assert_eq!(src, before);
            assert_eq!((image.width, image.height), (96, 96));
            let mut interior = 0u32;
            let mut punched = 0u32;
            let mut strong = 0u32;
            let mut rim = 0u32;
            let mut rim_cut = 0u32;
            let mut rim_gone = 0u32;
            let mut weak_sum = 0.0;
            let mut weak_n = 0.0;
            let mut firm_sum = 0.0;
            let mut firm_n = 0.0;
            for y in 0..96 {
                for x in 0..96 {
                    let px = at(&image.rgba, 96, x, y);
                    assert_eq!([px[0], px[1], px[2]], [180, 20, 20], "wear keeps the ink colour");
                    let nx = (f64::from(x) + 0.5) / 96.0 * 2.0 - 1.0;
                    let ny = (f64::from(y) + 0.5) / 96.0 * 2.0 - 1.0;
                    let span = nx * press.axis_x + ny * press.axis_y;
                    if span < -0.35 {
                        weak_sum += f64::from(px[3]);
                        weak_n += 1.0;
                    } else if span > 0.35 {
                        firm_sum += f64::from(px[3]);
                        firm_n += 1.0;
                    }
                    if near_edge(&src, 96, 96, x, y) {
                        rim += 1;
                        if px[3] < 210 {
                            rim_cut += 1;
                        }
                        if px[3] < 40 {
                            rim_gone += 1;
                        }
                        continue;
                    }
                    interior += 1;
                    if px[3] <= 40 {
                        punched += 1;
                    }
                    if px[3] >= 140 {
                        strong += 1;
                    }
                }
            }
            let punch = f64::from(punched) / f64::from(interior);
            let gap = firm_sum / firm_n - weak_sum / weak_n;
            holes[slot] = punch;
            bias[slot] = gap;
            rim_open[slot] = f64::from(rim_cut) / f64::from(rim.max(1));
            rim_gap[slot] = f64::from(rim_gone) / f64::from(rim.max(1));
            eprintln!("SEAL_WEAR strength {strength} punched {punch:.3} bias {gap:.1} rim_cut {:.3} rim_gone {:.3}", rim_open[slot], rim_gap[slot]);
            assert!(punch > 0.005, "strength {strength} should open some paper");
            assert!(punch < 0.40, "strength {strength} removed too much ({punch})");
            assert!(f64::from(strong) / f64::from(interior) > 0.45, "the fill must stay readable");
            assert!(gap > 8.0, "strength {strength} pressure bias {gap}");
            assert!(rim_gap[slot] < 0.15, "the outline broke apart ({})", rim_gap[slot]);
        }
        assert!(holes[2] > holes[1] && holes[1] > holes[0], "stronger wear opens more paper: {holes:?}");
        assert!(bias[2] > bias[1] && bias[1] > bias[0], "stronger wear leans harder: {bias:?}");
        assert!(rim_open[2] > rim_open[0], "strength 30 should nick the rim more than strength 10");
        assert!(rim_open[2] > 0.04, "the rim should show some wear at strength 30");
        assert!(rim_gap[2] > rim_gap[0] && rim_gap[2] > 0.008, "strength 30 should open a few gaps in the outline: {rim_gap:?}");

        let shown = box_mean(&src, 96, &{
            let effects = SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
            let Applied::Image(image) = apply(&src, 96, 96, &effects, 4).unwrap() else { panic!("show") };
            image.rgba
        });
        assert!(shown >= 18.0, "downsampled strength 30 should still differ, mean delta {shown}");

        let mut line = solid(80, 24, [0, 0, 0, 0]);
        for x in 4..76 {
            for y in 11..13 {
                let i = index(80, x, y);
                line[i..i + 4].copy_from_slice(&[200, 30, 30, 255]);
            }
        }
        let heavy = SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
        let Applied::Image(image) = apply(&line, 80, 24, &heavy, 9).unwrap() else { panic!("line") };
        let mut stroke = 0u32;
        let mut alive = 0u32;
        let mut changed = 0u32;
        for x in 4..76 {
            for y in 11..13 {
                stroke += 1;
                let px = at(&image.rgba, 80, x, y);
                assert_eq!([px[0], px[1], px[2]], [200, 30, 30]);
                if px[3] >= 100 {
                    alive += 1;
                }
                if px[3] < 245 {
                    changed += 1;
                }
            }
        }
        let survival = f64::from(alive) / f64::from(stroke);
        let moved = f64::from(changed) / f64::from(stroke);
        eprintln!("SEAL_WEAR thin survival {survival:.3} changed {moved:.3}");
        assert!(survival >= 0.65, "thin stroke survival {survival}");
        assert!(moved >= 0.15, "a thin stroke should show some wear, changed {moved}");
        assert_eq!(at(&image.rgba, 80, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn fade_only_stays_inside_the_ink_and_bleed_only_grows_past_it() {
        let src = disk(96);
        let fade = SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
        let Applied::Image(faded) = apply(&src, 96, 96, &fade, 6).unwrap() else { panic!("fade") };
        assert_eq!((faded.width, faded.height), (96, 96));
        let mut thinned = 0u32;
        for y in 0..96 {
            for x in 0..96 {
                let px = at(&faded.rgba, 96, x, y);
                let orig = at(&src, 96, x, y)[3];
                if orig == 0 {
                    assert_eq!(px[3], 0, "fade must not paint the background");
                } else if px[3] < 200 {
                    thinned += 1;
                }
            }
        }
        assert!(thinned > 40, "fade should thin existing ink, thinned {thinned}");

        let spread_effects = SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 30.0, manual: false, manual_deg: 0.0 };
        assert!(matches!(apply(&src, 96, 96, &spread_effects, 6).unwrap(), Applied::Unchanged));
        let (rgba, width, height) = bleed(&src, 96, 96, 30.0, 6).unwrap();
        assert!(width > 96 && height > 96);
        let pad = pads(96, 96, width, height);
        let ox = pad[0];
        let oy = pad[1];
        let mut grown = 0u32;
        let centre = 47.5;
        for y in 0..96 {
            for x in 0..96 {
                if at(&src, 96, x, y)[3] == 0 {
                    continue;
                }
                let px = at(&rgba, width, x + ox, y + oy);
                assert_eq!([px[0], px[1], px[2]], [180, 20, 20]);
                let dx = f64::from(x) - centre;
                let dy = f64::from(y) - centre;
                if (dx * dx + dy * dy).sqrt() < f64::from(96) * 0.46 - 9.0 {
                    assert_eq!(px[3], 255, "drag does not repaint the body");
                } else {
                    assert!(px[3] >= 90, "the contour stays readable");
                }
            }
        }
        for y in 0..height {
            for x in 0..width {
                let sx = x as i32 - ox as i32;
                let sy = y as i32 - oy as i32;
                let inside = sx >= 0 && sy >= 0 && (sx as u32) < 96 && (sy as u32) < 96 && at(&src, 96, sx as u32, sy as u32)[3] > 0;
                if !inside && at(&rgba, width, x, y)[3] > 0 {
                    grown += 1;
                }
            }
        }
        assert!(grown > 20, "bleed should add ink outside the contour, grown {grown}");
    }

    /// Mean absolute alpha change after a 4×4 box filter, over blocks that started opaque.
    fn box_mean(src: &[u8], width: u32, worn: &[u8]) -> f64 {
        let mut sum = 0.0;
        let mut n = 0.0;
        let step = 4u32;
        let mut y = 0;
        while y + step <= width {
            let mut x = 0;
            while x + step <= width {
                let mut a0 = 0.0;
                let mut a1 = 0.0;
                for dy in 0..step {
                    for dx in 0..step {
                        a0 += f64::from(at(src, width, x + dx, y + dy)[3]);
                        a1 += f64::from(at(worn, width, x + dx, y + dy)[3]);
                    }
                }
                if a0 > 0.0 {
                    sum += (a0 - a1).abs() / f64::from(step * step);
                    n += 1.0;
                }
                x += step;
            }
            y += step;
        }
        if n == 0.0 { 0.0 } else { sum / n }
    }

    #[test]
    fn bleed_uses_nearby_ink_and_leaves_the_far_background_clear() {
        assert!((bleed_radius_px(5.0, 1000, 800) - 3.2).abs() < 1e-9);
        assert!((3.2_f64 / 800.0 - 0.004).abs() < 1e-9);
        let mut src = solid(48, 48, [0, 0, 0, 0]);
        let i = index(48, 24, 24);
        src[i] = 20;
        src[i + 1] = 40;
        src[i + 2] = 200;
        src[i + 3] = 255;
        let strong = SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 30.0, manual: false, manual_deg: 0.0 };
        let before = src.clone();
        assert!(matches!(apply(&src, 48, 48, &strong, 2).unwrap(), Applied::Unchanged));
        let (rgba, width, height) = bleed(&src, 48, 48, 30.0, 2).unwrap();
        assert_eq!(src, before);
        assert!(width > 48 && height > 48);
        let pad = pads(48, 48, width, height);
        let ox = pad[0];
        let oy = pad[1];
        let centre = at(&rgba, width, 24 + ox, 24 + oy);
        assert_eq!([centre[0], centre[1], centre[2]], [20, 40, 200]);
        assert!(centre[3] >= 100, "{centre:?}");
        assert_eq!(at(&rgba, width, 0, 0)[3], 0);
        let mut neighbours = 0u32;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let px = at(&rgba, width, (24 + dx) as u32 + ox, (24 + dy) as u32 + oy);
            if px[3] > 0 {
                neighbours += 1;
                assert_eq!([px[0], px[1], px[2]], [20, 40, 200]);
            }
        }
        assert!(neighbours < 4, "a single dot must not grow into a uniform blot, neighbours {neighbours}");

        let none = SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 0.0, manual: false, manual_deg: 0.0 };
        assert!(matches!(apply(&src, 48, 48, &none, 2).unwrap(), Applied::Unchanged));
    }

    fn disk(side: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (side as usize).saturating_mul(side as usize).saturating_mul(4)];
        let c = (f64::from(side) - 1.0) * 0.5;
        let radius = f64::from(side) * 0.46;
        let limit = radius * radius;
        for y in 0..side {
            for x in 0..side {
                let dx = f64::from(x) - c;
                let dy = f64::from(y) - c;
                if dx * dx + dy * dy <= limit {
                    let i = index(side, x, y);
                    if let Some(slot) = buf.get_mut(i..i + 4) {
                        slot.copy_from_slice(&[180, 20, 20, 255]);
                    }
                }
            }
        }
        buf
    }

    #[test]
    fn bleed_drags_along_one_direction_without_copying_the_seal() {
        let dot = {
            let mut src = solid(48, 48, [0, 0, 0, 0]);
            let i = index(48, 24, 24);
            src[i..i + 4].copy_from_slice(&[20, 40, 200, 255]);
            src
        };
        let before = dot.clone();
        let (buf, w, h) = bleed(&dot, 48, 48, 30.0, 2).unwrap();
        assert_eq!(dot, before);
        let ox = (w - 48) / 2;
        let oy = (h - 48) / 2;
        let centre = at(&buf, w, 24 + ox, 24 + oy);
        assert_eq!([centre[0], centre[1], centre[2]], [20, 40, 200]);
        assert!(centre[3] >= 100, "{centre:?}");
        assert_eq!(at(&buf, w, 0, 0)[3], 0);
        let again = bleed(&dot, 48, 48, 30.0, 2).unwrap();
        assert_eq!(buf, again.0);
        let other = bleed(&dot, 48, 48, 30.0, 9).unwrap();
        assert_ne!(buf, other.0, "another seed should move the drag");

        let (axis_x, axis_y) = smear_axis(6);
        let mut spread = [0u32; 3];
        let mut forward = [0u32; 3];
        let mut backward = [0u32; 3];
        let disk_r = 180.0 * 0.46;
        let centre = (180.0 - 1.0) * 0.5;
        for (slot, strength) in [10.0, 20.0, 30.0].into_iter().enumerate() {
            let round = disk(180);
            let (buf, w, h) = bleed(&round, 180, 180, strength, 6).unwrap();
            let ox = (w - 180) / 2;
            let oy = (h - 180) / 2;
            let mut grown = 0u32;
            let mut alphas = Vec::new();
            let mut ahead = 0u32;
            let mut behind = 0u32;
            let mut eased = 0u32;
            let mut mass = 0.0;
            let mut mx = 0.0;
            let mut my = 0.0;
            let mut reach = 0.0f64;
            for y in 0..h {
                for x in 0..w {
                    let px = at(&buf, w, x, y);
                    let sx = x as i32 - ox as i32;
                    let sy = y as i32 - oy as i32;
                    let inside = sx >= 0 && sy >= 0 && (sx as u32) < 180 && (sy as u32) < 180 && at(&round, 180, sx as u32, sy as u32)[3] > 0;
                    if inside {
                        assert_eq!([px[0], px[1], px[2]], [180, 20, 20]);
                        let dx = f64::from(sx) - centre;
                        let dy = f64::from(sy) - centre;
                        let len_in = (dx * dx + dy * dy).sqrt();
                        if len_in < disk_r - 8.0 {
                            assert_eq!(px[3], 255, "the body is not a second impression");
                        } else {
                            assert!(px[3] >= 90, "contour alpha {}", px[3]);
                            if px[3] < 250 {
                                eased += 1;
                            }
                        }
                        mass += f64::from(px[3]);
                        mx += f64::from(sx) * f64::from(px[3]);
                        my += f64::from(sy) * f64::from(px[3]);
                        continue;
                    }
                    if px[3] > 0 {
                        grown += 1;
                        assert_eq!([px[0], px[1], px[2]], [180, 20, 20]);
                        alphas.push(px[3]);
                        let dx = f64::from(sx) - centre;
                        let dy = f64::from(sy) - centre;
                        let len = (dx * dx + dy * dy).sqrt().max(1.0e-6);
                        let align = dx / len * axis_x + dy / len * axis_y;
                        if align > 0.35 {
                            ahead += 1;
                        } else if align < -0.35 {
                            behind += 1;
                        }
                        assert!(len <= disk_r + slip_px(strength) + 2.8, "drag ran past its slip, len {len:.2}");
                        reach = reach.max(len - disk_r);
                        mass += f64::from(px[3]);
                        mx += f64::from(sx) * f64::from(px[3]);
                        my += f64::from(sy) * f64::from(px[3]);
                    }
                }
            }
            let shift = ((mx / mass - centre).powi(2) + (my / mass - centre).powi(2)).sqrt();
            spread[slot] = grown;
            forward[slot] = ahead;
            backward[slot] = behind;
            let mean = alphas.iter().map(|a| f64::from(*a)).sum::<f64>() / f64::from(alphas.len().max(1) as u32);
            eprintln!(
                "SEAL_BLEED strength {strength} grown {grown} forward {ahead} back {behind} mean_alpha {mean:.1} eased {eased} reach {reach:.2} shift {shift:.2}"
            );
            assert!(shift < 2.0, "the body must not translate as a second stamp, shift {shift}");
            assert!(eased > 30, "the trailing band should lighten, eased {eased}");
            assert!(reach + 0.2 >= slip_px(strength) * 0.45, "forward slip {reach} at {strength}");
            assert_eq!(at(&buf, w, 0, 0)[3], 0, "the far corner stays clear");
            assert!(grown > 0, "strength {strength}");
            assert!(ahead > behind.saturating_mul(3), "the drag should face one way, forward {ahead} back {behind}");
            assert!(mean > 35.0, "the tail should read as ink, mean {mean}");
        }
        assert!(spread[2] > spread[1] && spread[1] > spread[0], "stronger drag reaches further: {spread:?}");
        assert!(forward[2] > forward[0], "strength 30 drags more of the forward arc than strength 10");
        assert!(backward[2] * 4 < forward[2].max(1), "the back of the seal should stay almost clean");

        let mut ring = solid(160, 160, [0, 0, 0, 0]);
        let c = 79.5;
        for y in 0..160 {
            for x in 0..160 {
                let dx = f64::from(x) - c;
                let dy = f64::from(y) - c;
                let d2 = dx * dx + dy * dy;
                if (24.0 * 24.0..=70.0 * 70.0).contains(&d2) {
                    let i = index(160, x, y);
                    ring[i..i + 4].copy_from_slice(&[180, 20, 20, 255]);
                }
            }
        }
        let (buf, w, h) = bleed(&ring, 160, 160, 30.0, 3).unwrap();
        let ox = (w - 160) / 2;
        let oy = (h - 160) / 2;
        assert_eq!(at(&buf, w, 80 + ox, 80 + oy)[3], 0, "a character-sized counter stays open");
        let mut low = 255u8;
        let mut high = 0u8;
        for step in 0..72 {
            let ang = f64::from(step) * std::f64::consts::TAU / 72.0;
            let x = (c + 71.5 * ang.cos()).round() as i32 + ox as i32;
            let y = (c + 71.5 * ang.sin()).round() as i32 + oy as i32;
            if x < 0 || y < 0 || x as u32 >= w || y as u32 >= h {
                continue;
            }
            let alpha = at(&buf, w, x as u32, y as u32)[3];
            low = low.min(alpha);
            high = high.max(alpha);
        }
        assert!(i32::from(high) - i32::from(low) > 30, "fringe alpha {low}..={high} should vary around the contour");
    }

    #[test]
    fn drag_does_not_fill_holes_left_by_wear() {
        let src = disk(120);
        let worn = wear(&src, 120, 120, 30.0, 4);
        let mut holes = 0u32;
        for y in 0..120 {
            for x in 0..120 {
                if at(&src, 120, x, y)[3] > 0 && at(&worn, 120, x, y)[3] <= 40 {
                    holes += 1;
                }
            }
        }
        assert!(holes > 10, "expected wear holes, got {holes}");
        let (buf, w, h) = bleed(&worn, 120, 120, 30.0, 4).unwrap();
        let ox = (w - 120) / 2;
        let oy = (h - 120) / 2;
        let mut still = 0u32;
        for y in 0..120 {
            for x in 0..120 {
                if at(&src, 120, x, y)[3] == 0 || at(&worn, 120, x, y)[3] > 40 {
                    continue;
                }
                if at(&buf, w, x + ox, y + oy)[3] <= 40 {
                    still += 1;
                }
            }
        }
        assert_eq!(still, holes, "drag refilled wear holes: before {holes} after {still}");
    }

    fn millis_of(work: impl FnOnce()) -> u128 {
        let start = std::time::Instant::now();
        work();
        start.elapsed().as_millis()
    }

    #[test]
    fn a_1254_disk_finishes_each_stage_without_the_old_spread() {
        let side = 1254u32;
        let src = disk(side);
        let before = src.clone();
        let profiles = [
            ("off", SealEffects::off()),
            ("fade10", SealEffects { enabled: true, rotation_deg: 0.0, fade: 10.0, bleed: 0.0, manual: false, manual_deg: 0.0 }),
            ("fade20", SealEffects { enabled: true, rotation_deg: 0.0, fade: 20.0, bleed: 0.0, manual: false, manual_deg: 0.0 }),
            ("fade30", SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 0.0, manual: false, manual_deg: 0.0 }),
            ("bleed10", SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 10.0, manual: false, manual_deg: 0.0 }),
            ("bleed20", SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 20.0, manual: false, manual_deg: 0.0 }),
            ("bleed30", SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 30.0, manual: false, manual_deg: 0.0 }),
            ("fade20-bleed20", SealEffects { enabled: true, rotation_deg: 0.0, fade: 20.0, bleed: 20.0, manual: false, manual_deg: 0.0 }),
            ("fade30-bleed30", SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 30.0, manual: false, manual_deg: 0.0 }),
            ("all", SealEffects { enabled: true, rotation_deg: 10.0, fade: 30.0, bleed: 30.0, manual: false, manual_deg: 0.0 }),
        ];
        for (name, effects) in profiles {
            let ms = millis_of(|| {
                let applied = apply(&src, side, side, &effects, 11).unwrap();
                if let Applied::Image(image) = &applied {
                    assert!(image.rgba.len() >= src.len() / 2, "{name}");
                }
            });
            eprintln!("SEAL_TIMING after {name} {ms}ms");
            assert!(ms < 3_000, "{name} took {ms}ms");
        }
        assert_eq!(src, before);

        let worn = millis_of(|| {
            let faded = wear(&src, side, side, 16.0, 11);
            assert_eq!(faded.len(), src.len());
        });
        let spread = millis_of(|| {
            let (buf, _, _) = bleed(&src, side, side, 30.0, 11).unwrap();
            assert!(buf.len() > src.len());
        });
        let turned = millis_of(|| {
            let (buf, _, _) = rotate(&src, side, side, 4.0).unwrap();
            assert!(buf.len() > src.len());
        });
        let copied = millis_of(|| {
            let image = egui::ColorImage::from_rgba_unmultiplied([side as usize, side as usize], &src);
            assert_eq!(image.pixels.len(), (side as usize) * (side as usize));
        });
        eprintln!("SEAL_TIMING after wear {worn}ms bleed30 {spread}ms rotate {turned}ms texture {copied}ms");
        assert!(worn < 3_000 && spread < 3_000 && turned < 3_000 && copied < 3_000);

        let sample = disk(400);
        let old = millis_of(|| {
            let (buf, _, _) = bleed_splat(&sample, 400, 400, 30.0, 11).unwrap();
            assert!(!buf.is_empty());
        });
        let scaled = (f64::from(side) / 400.0).powi(4) * old as f64;
        eprintln!("SEAL_TIMING before bleed30 side400 {old}ms extrapolated1254 {scaled:.0}ms");
        assert!(old < 20_000, "the comparison disk took {old}ms");
        let old_bleed5 = millis_of(|| {
            let (buf, _, _) = bleed_splat(&src, side, side, 5.0, 11).unwrap();
            assert!(buf.len() > src.len());
        });
        eprintln!("SEAL_TIMING before bleed5 1254 {old_bleed5}ms");
        assert!(old_bleed5 < 15_000, "old bleed 5 took {old_bleed5}ms");
    }

    fn seal_like(side: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (side as usize).saturating_mul(side as usize).saturating_mul(4)];
        let c = (f64::from(side) - 1.0) * 0.5;
        let outer = f64::from(side) * 0.46;
        let inner = f64::from(side) * 0.34;
        for y in 0..side {
            for x in 0..side {
                let dx = f64::from(x) - c;
                let dy = f64::from(y) - c;
                let d2 = dx * dx + dy * dy;
                let frame = d2 <= outer * outer && d2 >= inner * inner;
                let thick = dx.abs() < f64::from(side) * 0.06 && dy > -inner * 0.8 && dy < inner * 0.55;
                let thin = dy.abs() < 1.5 && dx > -inner * 0.7 && dx < inner * 0.7;
                let small = (dx - inner * 0.45).abs() < 2.0 && (dy - inner * 0.35).abs() < 2.0;
                let hole = dx.abs() < f64::from(side) * 0.05 && dy.abs() < f64::from(side) * 0.05;
                if (frame || thick || thin || small) && !hole {
                    let i = index(side, x, y);
                    if let Some(slot) = buf.get_mut(i..i + 4) {
                        slot.copy_from_slice(&[180, 20, 20, 255]);
                    }
                }
            }
        }
        buf
    }

    fn save_rgba(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) {
        let image = image::RgbaImage::from_raw(width, height, rgba.to_vec()).expect("rgba");
        image.save(path).expect("png");
    }

    #[test]
    fn a_synthetic_seal_keeps_its_holes_and_writes_a_comparison() {
        let side = 220u32;
        let src = seal_like(side);
        let before = src.clone();
        let heavy = SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 30.0, manual: false, manual_deg: 0.0 };
        let Applied::Image(image) = apply(&src, side, side, &heavy, 8).unwrap() else { panic!("both") };
        assert_eq!(src, before);
        let mut rim = 0u32;
        let mut rim_alive = 0u32;
        let mut body = 0u32;
        let mut body_alive = 0u32;
        let mut hole = 0u32;
        let mut hole_clear = 0u32;
        let c = (f64::from(side) - 1.0) * 0.5;
        let outer = f64::from(side) * 0.46;
        for y in 0..side {
            for x in 0..side {
                let dx = f64::from(x) - c;
                let dy = f64::from(y) - c;
                let radius = (dx * dx + dy * dy).sqrt();
                let ox = x + image.pad[0];
                let oy = y + image.pad[1];
                let px = at(&image.rgba, image.width, ox, oy);
                if radius >= outer - 2.5 && radius <= outer {
                    rim += 1;
                    if px[3] >= 70 {
                        rim_alive += 1;
                    }
                } else if radius >= outer - 14.0 && radius < outer - 4.0 {
                    body += 1;
                    if px[3] >= 160 {
                        body_alive += 1;
                    }
                }
                if dx.abs() <= 1.5 && dy.abs() <= 1.5 {
                    hole += 1;
                    if px[3] == 0 {
                        hole_clear += 1;
                    }
                }
            }
        }
        let rim_keep = f64::from(rim_alive) / f64::from(rim.max(1));
        let body_keep = f64::from(body_alive) / f64::from(body.max(1));
        let hole_open = f64::from(hole_clear) / f64::from(hole.max(1));
        eprintln!("SEAL_LOOK rim {rim_keep:.3} body {body_keep:.3} hole {hole_open:.3}");
        assert!(rim_keep > 0.80, "outer frame survival {rim_keep}");
        assert!(body_keep > 0.55, "the ring should stay readable, kept {body_keep}");
        assert!(hole_open > 0.80, "counter stayed open {hole_open}");

        let root = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
        let dir = std::path::Path::new(&root).join("seal-compare");
        std::fs::create_dir_all(&dir).unwrap();
        save_rgba(&dir.join("before.png"), side, side, &src);
        for (name, effects) in [
            ("fade10", SealEffects { enabled: true, rotation_deg: 0.0, fade: 10.0, bleed: 0.0, manual: false, manual_deg: 0.0 }),
            ("fade20", SealEffects { enabled: true, rotation_deg: 0.0, fade: 20.0, bleed: 0.0, manual: false, manual_deg: 0.0 }),
            ("fade30", SealEffects { enabled: true, rotation_deg: 0.0, fade: 30.0, bleed: 0.0, manual: false, manual_deg: 0.0 }),
            ("bleed10", SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 10.0, manual: false, manual_deg: 0.0 }),
            ("bleed20", SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 20.0, manual: false, manual_deg: 0.0 }),
            ("bleed30", SealEffects { enabled: true, rotation_deg: 0.0, fade: 0.0, bleed: 30.0, manual: false, manual_deg: 0.0 }),
            ("fade20-bleed20", SealEffects { enabled: true, rotation_deg: 0.0, fade: 20.0, bleed: 20.0, manual: false, manual_deg: 0.0 }),
            ("both30", heavy),
        ] {
            if effects.fade == 0.0 && effects.rotation_deg == 0.0 {
                assert!(matches!(apply(&src, side, side, &effects, 8).unwrap(), Applied::Unchanged), "{name}");
                let (buf, width, height) = bleed(&src, side, side, effects.bleed, 8).unwrap();
                save_rgba(&dir.join(format!("{name}.png")), width, height, &buf);
            } else {
                let Applied::Image(image) = apply(&src, side, side, &effects, 8).unwrap() else { panic!("{name}") };
                save_rgba(&dir.join(format!("{name}.png")), image.width, image.height, &image.rgba);
            }
        }
        assert!(dir.join("both30.png").is_file());
    }

    fn diag_seal(side: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (side as usize).saturating_mul(side as usize).saturating_mul(4)];
        let c = (f64::from(side) - 1.0) * 0.5;
        let outer = f64::from(side) * 0.46;
        let inner = f64::from(side) * 0.34;
        for y in 0..side {
            for x in 0..side {
                let dx = f64::from(x) - c;
                let dy = f64::from(y) - c;
                let d2 = dx * dx + dy * dy;
                let frame = d2 <= outer * outer && d2 >= inner * inner;
                let thick = dx.abs() < f64::from(side) * 0.06 && dy > -inner * 0.8 && dy < inner * 0.55;
                let thin = dy.abs() < 1.5 && dx > -inner * 0.7 && dx < inner * 0.7;
                let small = (dx - inner * 0.45).abs() < 2.0 && (dy - inner * 0.35).abs() < 2.0;
                let hole = dx.abs() < f64::from(side) * 0.05 && dy.abs() < f64::from(side) * 0.05;
                if (frame || thick || thin || small) && !hole {
                    let i = index(side, x, y);
                    if let Some(slot) = buf.get_mut(i..i + 4) {
                        slot.copy_from_slice(&[180, 20, 20, 255]);
                    }
                }
            }
        }
        buf
    }

    fn onto_white(rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(rgba.len());
        for px in rgba.chunks_exact(4) {
            let a = f64::from(px[3]) / 255.0;
            let mix = |c: u8| (f64::from(c) * a + 255.0 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
            out.extend([mix(px[0]), mix(px[1]), mix(px[2]), 255]);
        }
        out
    }

    fn blit_rgba(dst: &mut [u8], dw: u32, src: &[u8], sw: u32, sh: u32, origin: [i32; 2]) {
        let dh = dst.len() / (dw as usize).saturating_mul(4).max(1);
        for y in 0..sh {
            for x in 0..sw {
                let dx = x as i32 + origin[0];
                let dy = y as i32 + origin[1];
                if dx < 0 || dy < 0 || dx >= dw as i32 || dy as usize >= dh {
                    continue;
                }
                let px = at(src, sw, x, y);
                let a = f64::from(px[3]) / 255.0;
                let i = index(dw, dx as u32, dy as u32);
                let Some(slot) = dst.get_mut(i..i + 4) else { continue };
                let mix = |c: u8, b: u8| (f64::from(c) * a + f64::from(b) * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
                let rgb = [mix(px[0], slot[0]), mix(px[1], slot[1]), mix(px[2], slot[2]), 255];
                slot.copy_from_slice(&rgb);
            }
        }
    }

    fn white_canvas(w: u32, h: u32) -> Vec<u8> {
        vec![255u8; (w as usize).saturating_mul(h as usize).saturating_mul(4)]
    }

    fn resize_area(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
        let mut out = white_canvas(dw, dh);
        for y in 0..dh {
            let y0 = f64::from(y) * f64::from(sh) / f64::from(dh);
            let y1 = f64::from(y + 1) * f64::from(sh) / f64::from(dh);
            for x in 0..dw {
                let x0 = f64::from(x) * f64::from(sw) / f64::from(dw);
                let x1 = f64::from(x + 1) * f64::from(sw) / f64::from(dw);
                let mut acc = [0.0; 4];
                let mut weight = 0.0;
                let iy0 = y0.floor() as i32;
                let iy1 = (y1.ceil() as i32 - 1).max(iy0);
                let ix0 = x0.floor() as i32;
                let ix1 = (x1.ceil() as i32 - 1).max(ix0);
                for iy in iy0..=iy1 {
                    for ix in ix0..=ix1 {
                        if ix < 0 || iy < 0 || ix >= sw as i32 || iy >= sh as i32 {
                            continue;
                        }
                        let cover_x = ((ix + 1) as f64).min(x1) - (ix as f64).max(x0);
                        let cover_y = ((iy + 1) as f64).min(y1) - (iy as f64).max(y0);
                        let w = cover_x.max(0.0) * cover_y.max(0.0);
                        if w <= 0.0 {
                            continue;
                        }
                        let px = at(src, sw, ix as u32, iy as u32);
                        for (slot, channel) in acc.iter_mut().zip(px) {
                            *slot += f64::from(channel) * w;
                        }
                        weight += w;
                    }
                }
                if weight <= 0.0 {
                    continue;
                }
                let i = index(dw, x, y);
                if let Some(slot) = out.get_mut(i..i + 4) {
                    for c in 0..4 {
                        slot[c] = (acc[c] / weight).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
        out
    }

    fn resize_bilinear(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
        let mut out = white_canvas(dw, dh);
        for y in 0..dh {
            let fy = (f64::from(y) + 0.5) * f64::from(sh) / f64::from(dh) - 0.5;
            let y0 = fy.floor();
            let ty = (fy - y0).clamp(0.0, 1.0);
            let y0 = y0 as i32;
            for x in 0..dw {
                let fx = (f64::from(x) + 0.5) * f64::from(sw) / f64::from(dw) - 0.5;
                let x0 = fx.floor();
                let tx = (fx - x0).clamp(0.0, 1.0);
                let x0 = x0 as i32;
                let mut acc = [0.0; 4];
                for (iy, wy) in [(y0, 1.0 - ty), (y0.saturating_add(1), ty)] {
                    for (ix, wx) in [(x0, 1.0 - tx), (x0.saturating_add(1), tx)] {
                        let w = wx * wy;
                        if w <= 0.0 || ix < 0 || iy < 0 || ix >= sw as i32 || iy >= sh as i32 {
                            continue;
                        }
                        let px = at(src, sw, ix as u32, iy as u32);
                        for (slot, channel) in acc.iter_mut().zip(px) {
                            *slot += f64::from(channel) * w;
                        }
                    }
                }
                let i = index(dw, x, y);
                if let Some(slot) = out.get_mut(i..i + 4) {
                    for c in 0..4 {
                        slot[c] = acc[c].round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
        out
    }

    fn view_delta(before: &[u8], after: &[u8], w: u32) -> (u8, u32, u32, f64) {
        let mut max_d = 0u8;
        let mut n12 = 0u32;
        let mut n24 = 0u32;
        let mut sum = 0.0;
        let mut n = 0.0;
        let h = before.len() / (w as usize).saturating_mul(4);
        for y in 0..h as u32 {
            for x in 0..w {
                let a = at(before, w, x, y);
                let b = at(after, w, x, y);
                let d = a.iter().zip(b).map(|(p, q)| p.abs_diff(q)).max().unwrap_or(0);
                max_d = max_d.max(d);
                if d >= 12 {
                    n12 += 1;
                }
                if d >= 24 {
                    n24 += 1;
                }
                sum += f64::from(d);
                n += 1.0;
            }
        }
        (max_d, n12, n24, if n == 0.0 { 0.0 } else { sum / n })
    }

    fn amplify_diff(before: &[u8], after: &[u8], w: u32, h: u32) -> Vec<u8> {
        let mut out = white_canvas(w, h);
        for y in 0..h {
            for x in 0..w {
                let a = at(before, w, x, y);
                let b = at(after, w, x, y);
                let d = a.iter().zip(b).map(|(p, q)| p.abs_diff(q)).max().unwrap_or(0);
                let v = u16::from(d).saturating_mul(8).min(255) as u8;
                let i = index(w, x, y);
                if let Some(slot) = out.get_mut(i..i + 4) {
                    slot.copy_from_slice(&[255, 255u8.saturating_sub(v), 255u8.saturating_sub(v), 255]);
                }
            }
        }
        out
    }

    fn placed(src: &[u8], sw: u32, sh: u32, canvas_w: u32, canvas_h: u32, ox: i32, oy: i32) -> Vec<u8> {
        let mut canvas = white_canvas(canvas_w, canvas_h);
        blit_rgba(&mut canvas, canvas_w, src, sw, sh, [ox, oy]);
        canvas
    }

    /// The slip routine, for the visibility measurement. Product [`apply`] does not call it.
    fn slip_applied(src: &[u8], side: u32, bleed_strength: f64, fade: f64, seed: u64) -> Result<Applied, EffectError> {
        check_buffer(src, side, side)?;
        let worn = if fade > 0.0 { Some(wear(src, side, side, fade, seed)) } else { None };
        let (rgba, width, height) = bleed(worn.as_deref().unwrap_or(src), side, side, bleed_strength, seed)?;
        Ok(Applied::Image(EffectImage {
            width,
            height,
            rgba,
            angle_deg: 0.0,
            source_width: side,
            source_height: side,
            pad: pads(side, side, width, height),
        }))
    }

    /// Diagnostic only. It reads the stored slip and writes pictures under `target/seal-compare`.
    /// Product placement does not paint that slip.
    #[test]
    fn diagnostic_slip_visibility_is_measured_without_changing_pixels() {
        let root = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
        let dir = std::path::Path::new(&root).join("seal-compare");
        std::fs::create_dir_all(&dir).unwrap();
        let mut report = String::new();
        let seed = 8u64;
        let other = 9u64;
        report.push_str("slip_px formula (source pixels, independent of image size)\n");
        for strength in [10.0, 20.0, 30.0] {
            report.push_str(&format!("  strength {strength:.0} slip_px {:.3}\n", slip_px(strength)));
        }
        report.push_str("display: compare fit long side is 168 egui points; texture is LINEAR with mipmap_mode None\n");
        report.push_str("display: compare 100% is 1 source pixel per point, 200% is 2\n");
        report.push_str("display: default stamp long side is 200 PDF points = 70.556 mm\n");

        let effects_of = |bleed: f64, fade: f64| SealEffects { enabled: true, rotation_deg: 0.0, fade, bleed, manual: false, manual_deg: 0.0 };
        for side in [256u32, 512, 1254, 1800, 2000] {
            let src = diag_seal(side);
            let before = src.clone();
            report.push_str(&format!("\n== side {side} seed {seed} ==\n"));
            for (name, effects) in [
                ("bleed10", effects_of(10.0, 0.0)),
                ("bleed20", effects_of(20.0, 0.0)),
                ("bleed30", effects_of(30.0, 0.0)),
                ("fade20-bleed20", effects_of(20.0, 20.0)),
            ] {
                match slip_applied(&src, side, effects.bleed, effects.fade, seed) {
                    Err(error) => report.push_str(&format!("{name} ERR {error:?}\n")),
                    Ok(Applied::Unchanged) => report.push_str(&format!("{name} UNCHANGED\n")),
                    Ok(Applied::Image(image)) => {
                        assert_eq!(src, before);
                        let again = slip_applied(&src, side, effects.bleed, effects.fade, seed).unwrap();
                        let Applied::Image(again) = again else { panic!("repeat") };
                        assert_eq!(image.rgba, again.rgba);
                        let line = measure_slip(&src, side, &image, name);
                        report.push_str(&line);
                        if side == 1254 || (side != 2000 && name == "bleed30") {
                            let diff = slip_diff(&src, side, &image);
                            save_rgba(&dir.join(format!("diag-{side}-diff-{name}.png")), image.width, image.height, &diff);
                        }
                        if side == 1254 {
                            let paper = onto_white(&image.rgba);
                            save_rgba(&dir.join(format!("diag-{side}-{name}.png")), image.width, image.height, &image.rgba);
                            save_rgba(&dir.join(format!("diag-{side}-{name}-paper.png")), image.width, image.height, &paper);
                        }
                    }
                }
            }
            if side == 1254 {
                save_rgba(&dir.join("diag-1254-before.png"), side, side, &src);
                save_rgba(&dir.join("diag-1254-before-paper.png"), side, side, &onto_white(&src));
                let changed = slip_applied(&src, side, 30.0, 0.0, other).unwrap();
                let same = slip_applied(&src, side, 30.0, 0.0, seed).unwrap();
                assert_ne!(same, changed, "another seed must move the slip");
                assert!(matches!(apply(&src, side, side, &effects_of(30.0, 0.0), seed).unwrap(), Applied::Unchanged));
                assert!(matches!(apply(&src, side, side, &SealEffects::off(), seed).unwrap(), Applied::Unchanged));
                let Applied::Image(image) = slip_applied(&src, side, 30.0, 0.0, seed).unwrap() else { panic!("bleed30") };
                let Applied::Image(mild) = slip_applied(&src, side, 10.0, 0.0, seed).unwrap() else { panic!("bleed10") };
                report.push_str(&view_report(&src, side, &mild, "bleed10"));
                report.push_str(&view_report(&src, side, &image, "bleed30"));
                for (label, long) in [("fit168", 168u32), ("page200", 200u32), ("page286", 286u32)] {
                    save_view(&dir, &src, side, &image, label, long, "bleed30");
                    if label == "fit168" {
                        save_view(&dir, &src, side, &mild, label, long, "bleed10");
                    }
                }
            }
            assert_eq!(src, before);
        }
        let path = dir.join("diag-metrics.txt");
        std::fs::write(&path, &report).unwrap();
        assert!(path.is_file());
        assert!(dir.join("diag-1254-diff-bleed30.png").is_file());
        eprintln!("{report}");
    }

    fn save_view(dir: &std::path::Path, src: &[u8], side: u32, image: &EffectImage, label: &str, long: u32, name: &str) {
        let (dw, dh) = fit_long(image.width, image.height, long);
        let before = placed(src, side, side, image.width, image.height, image.pad[0] as i32, image.pad[1] as i32);
        let after = onto_white(&image.rgba);
        let before_paper = onto_white(&before);
        for (filter, small_before, small_after) in [
            ("area", resize_area(&before_paper, image.width, image.height, dw, dh), resize_area(&after, image.width, image.height, dw, dh)),
            (
                "bilinear",
                resize_bilinear(&before_paper, image.width, image.height, dw, dh),
                resize_bilinear(&after, image.width, image.height, dw, dh),
            ),
        ] {
            save_rgba(&dir.join(format!("diag-1254-view-{label}-{filter}-{name}.png")), dw, dh, &small_after);
            save_rgba(
                &dir.join(format!("diag-1254-view-{label}-{filter}-diff-{name}.png")),
                dw,
                dh,
                &amplify_diff(&small_before, &small_after, dw, dh),
            );
        }
    }

    fn fit_long(w: u32, h: u32, long: u32) -> (u32, u32) {
        let scale = f64::from(long) / f64::from(w.max(h).max(1));
        (((f64::from(w) * scale).round() as u32).max(1), ((f64::from(h) * scale).round() as u32).max(1))
    }

    fn view_report(src: &[u8], side: u32, image: &EffectImage, name: &str) -> String {
        let before = placed(src, side, side, image.width, image.height, image.pad[0] as i32, image.pad[1] as i32);
        let before = onto_white(&before);
        let after = onto_white(&image.rgba);
        let mut out = String::new();
        for (label, long) in [
            ("fit168", 168u32),
            ("fit252", 252),
            ("fit336", 336),
            ("page200", 200),
            ("page286", 286),
            ("page429", 429),
            ("mm15", 43),
            ("mm20", 57),
            ("mm30", 85),
        ] {
            let (dw, dh) = fit_long(image.width, image.height, long);
            let a = resize_area(&before, image.width, image.height, dw, dh);
            let b = resize_area(&after, image.width, image.height, dw, dh);
            let (max_d, n12, n24, mean) = view_delta(&a, &b, dw);
            let c = resize_bilinear(&before, image.width, image.height, dw, dh);
            let d = resize_bilinear(&after, image.width, image.height, dw, dh);
            let (bmax, bn12, bn24, bmean) = view_delta(&c, &d, dw);
            out.push_str(&format!(
                "view {name} {label} {dw}x{dh} area max {max_d} px12 {n12} px24 {n24} mean {mean:.2} | bilinear max {bmax} px12 {bn12} px24 {bn24} mean {bmean:.2}\n"
            ));
        }
        out
    }

    fn slip_diff(src: &[u8], side: u32, image: &EffectImage) -> Vec<u8> {
        let mut out = white_canvas(image.width, image.height);
        let (px, py) = (image.pad[0] as i32, image.pad[1] as i32);
        for y in 0..image.height {
            for x in 0..image.width {
                let now = at(&image.rgba, image.width, x, y);
                let sx = x as i32 - px;
                let sy = y as i32 - py;
                let old =
                    if sx >= 0 && sy >= 0 && (sx as u32) < side && (sy as u32) < side { at(src, side, sx as u32, sy as u32) } else { [0, 0, 0, 0] };
                let color = if old[3] < 12 && now[3] >= 12 {
                    let t = f64::from(now[3]) / 255.0;
                    [20, (40.0 + 80.0 * t) as u8, (180.0 + 60.0 * t) as u8, 255]
                } else if old[3] >= 16 && i32::from(old[3]) - i32::from(now[3]) >= 12 {
                    let t = (f64::from(old[3] - now[3]) / 255.0).clamp(0.0, 1.0);
                    [(180.0 + 60.0 * t) as u8, (150.0 + 40.0 * t) as u8, 30, 255]
                } else if old[3] >= 16 {
                    [232, 232, 232, 255]
                } else {
                    [255, 255, 255, 255]
                };
                let i = index(image.width, x, y);
                if let Some(slot) = out.get_mut(i..i + 4) {
                    slot.copy_from_slice(&color);
                }
            }
        }
        out
    }

    fn measure_slip(src: &[u8], side: u32, image: &EffectImage, name: &str) -> String {
        let (px, py) = (image.pad[0] as i32, image.pad[1] as i32);
        let mut ink = 0u32;
        let mut contour = 0u32;
        let mut changed = 0u32;
        let mut changed_ink = 0u32;
        let mut changed_contour = 0u32;
        let mut fresh = 0u32;
        let mut thinned = 0u32;
        let mut sum_abs = 0.0;
        let mut sum_ink = 0.0;
        let mut sum_contour = 0.0;
        let mut sum_changed = 0.0;
        let mut max_abs = 0u8;
        let mut travel_sum = 0.0;
        let mut travel_n = 0.0;
        let mut travel_max = 0.0f64;
        let mut sector_fresh = [0u32; 8];
        let mut sector_thin = [0u32; 8];
        let centre = (f64::from(side) - 1.0) * 0.5;
        for y in 0..image.height {
            for x in 0..image.width {
                let now = at(&image.rgba, image.width, x, y);
                let sx = x as i32 - px;
                let sy = y as i32 - py;
                let inside = sx >= 0 && sy >= 0 && (sx as u32) < side && (sy as u32) < side;
                let old = if inside { at(src, side, sx as u32, sy as u32) } else { [0, 0, 0, 0] };
                let delta = old[3].abs_diff(now[3]);
                let dx = f64::from(sx) - centre;
                let dy = f64::from(sy) - centre;
                let mut ang = dy.atan2(dx);
                if ang < 0.0 {
                    ang += std::f64::consts::TAU;
                }
                let sector = ((ang / std::f64::consts::TAU) * 8.0).floor() as usize;
                let sector = sector.min(7);
                if old[3] >= 16 {
                    ink += 1;
                    sum_ink += f64::from(delta);
                    let near = contour_near(src, side, sx, sy);
                    if near {
                        contour += 1;
                        sum_contour += f64::from(delta);
                    }
                    if delta >= 12 {
                        changed_ink += 1;
                        if near {
                            changed_contour += 1;
                        }
                        thinned += 1;
                        sector_thin[sector] += 1;
                    }
                } else if now[3] >= 12 {
                    fresh += 1;
                    sector_fresh[sector] += 1;
                    if let Some(dist) = nearest_ink(src, side, sx, sy) {
                        travel_sum += dist;
                        travel_n += 1.0;
                        travel_max = travel_max.max(dist);
                    }
                }
                if delta >= 12 || (old[3] < 12 && now[3] >= 12) {
                    changed += 1;
                    sum_changed += f64::from(delta.max(now[3]));
                    max_abs = max_abs.max(delta.max(now[3]));
                }
                sum_abs += f64::from(delta);
            }
        }
        let pixels = f64::from(image.width) * f64::from(image.height);
        let mean_all = sum_abs / pixels.max(1.0);
        let mean_ink = if ink == 0 { 0.0 } else { sum_ink / f64::from(ink) };
        let mean_contour = if contour == 0 { 0.0 } else { sum_contour / f64::from(contour) };
        let mean_changed = if changed == 0 { 0.0 } else { sum_changed / f64::from(changed) };
        let mean_travel = if travel_n == 0.0 { 0.0 } else { travel_sum / travel_n };
        format!(
            "{name} out {}x{} pad {:?} ink {ink} contour {contour} changed {changed} ({:.3}% of frame, {:.2}% of ink) fresh {fresh} thinned {thinned} max_dA {max_abs} mean_dA_frame {mean_all:.3} mean_dA_ink {mean_ink:.2} mean_dA_contour {mean_contour:.2} mean_dA_changed {mean_changed:.1} travel_max {travel_max:.2} travel_mean {mean_travel:.2} changed_contour {changed_contour} sectors_fresh {sector_fresh:?} sectors_thin {sector_thin:?}\n",
            image.width,
            image.height,
            image.pad,
            f64::from(changed) / pixels.max(1.0) * 100.0,
            if ink == 0 { 0.0 } else { f64::from(changed_ink) / f64::from(ink) * 100.0 }
        )
    }

    fn contour_near(src: &[u8], side: u32, x: i32, y: i32) -> bool {
        let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
        for (dx, dy) in dirs {
            for step in 1..=8 {
                let nx = x + dx * step;
                let ny = y + dy * step;
                let clear = nx < 0 || ny < 0 || nx >= side as i32 || ny >= side as i32 || at(src, side, nx as u32, ny as u32)[3] < 12;
                if clear {
                    return true;
                }
            }
        }
        false
    }

    fn nearest_ink(src: &[u8], side: u32, x: i32, y: i32) -> Option<f64> {
        let mut best = f64::MAX;
        for dy in -12..=12 {
            for dx in -12..=12 {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= side as i32 || ny >= side as i32 {
                    continue;
                }
                if at(src, side, nx as u32, ny as u32)[3] >= 16 {
                    let dist = (f64::from(dx * dx + dy * dy)).sqrt();
                    best = best.min(dist);
                }
            }
        }
        if best.is_finite() && best < 100.0 { Some(best) } else { None }
    }
}
