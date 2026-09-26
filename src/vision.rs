//! BMP load + template match (no OpenCV).
//! no match → None → caller must not click.
//!
//! Search is coarse-to-fine SAD on distinctive pixels, then a 1px refine.
//! A recorded `prefer` point is tried first; flat fills stay on that point.

#[derive(Clone, Debug)]
pub struct RgbImage {
    pub w: i32,
    pub h: i32,
    pub rgb: Vec<u8>,
}

impl RgbImage {
    #[cfg(test)]
    fn get(&self, x: i32, y: i32) -> Option<(u8, u8, u8)> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        let i = ((y * self.w + x) * 3) as usize;
        Some((self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]))
    }

    pub fn crop(&self, x: i32, y: i32, w: i32, h: i32) -> Option<RgbImage> {
        if w < 1 || h < 1 {
            return None;
        }
        let x = x.max(0);
        let y = y.max(0);
        let w = w.min(self.w - x);
        let h = h.min(self.h - y);
        if w < 1 || h < 1 {
            return None;
        }
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        for row in 0..h {
            let src = (((y + row) * self.w + x) * 3) as usize;
            let dst = ((row * w) * 3) as usize;
            let n = (w * 3) as usize;
            rgb[dst..dst + n].copy_from_slice(&self.rgb[src..src + n]);
        }
        Some(RgbImage { w, h, rgb })
    }

    pub fn to_color_image(&self) -> egui::ColorImage {
        let mut pixels = Vec::with_capacity((self.w * self.h) as usize);
        for chunk in self.rgb.chunks_exact(3) {
            pixels.push(egui::Color32::from_rgb(chunk[0], chunk[1], chunk[2]));
        }
        egui::ColorImage {
            size: [self.w as usize, self.h as usize],
            pixels,
        }
    }
}

/// 24-bit bottom-up BMP, matching `load_bmp24`.
pub fn save_bmp24(path: &str, img: &RgbImage) -> bool {
    if img.w < 1 || img.h < 1 {
        return false;
    }
    let stride = (img.w as i64 * 3 + 3) / 4 * 4;
    let pix = stride * img.h as i64;
    if pix <= 0 || pix > (512 << 20) {
        return false;
    }
    let pix = pix as usize;
    let off = 54u32;
    let mut data = vec![0u8; off as usize + pix];
    data[0] = b'B';
    data[1] = b'M';
    let file_sz = data.len() as u32;
    data[2..6].copy_from_slice(&file_sz.to_le_bytes());
    data[10..14].copy_from_slice(&off.to_le_bytes());
    data[14..18].copy_from_slice(&40u32.to_le_bytes());
    data[18..22].copy_from_slice(&img.w.to_le_bytes());
    data[22..26].copy_from_slice(&img.h.to_le_bytes());
    data[26..28].copy_from_slice(&1u16.to_le_bytes());
    data[28..30].copy_from_slice(&24u16.to_le_bytes());
    for y in 0..img.h {
        let src_y = img.h - 1 - y;
        let dst = off as usize + (y as usize) * (stride as usize);
        for x in 0..img.w {
            let s = ((src_y * img.w + x) * 3) as usize;
            let d = dst + (x as usize) * 3;
            data[d] = img.rgb[s + 2];
            data[d + 1] = img.rgb[s + 1];
            data[d + 2] = img.rgb[s];
        }
    }
    std::fs::write(path, data).is_ok()
}

/// Timeout 0 = measure once, do not wait.
pub fn match_try_once(timeout_ms: u64) -> bool {
    timeout_ms == 0
}

/// First local pad around a recorded point (grows on later polls).
pub fn smart_search_pad(tw: i32, th: i32) -> i32 {
    tw.max(th).max(40)
}

pub fn load_bmp24(path: &str) -> Option<RgbImage> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 54 || &data[0..2] != b"BM" {
        return None;
    }
    let off = u32::from_le_bytes(data[10..14].try_into().ok()?) as usize;
    let hdr_sz = u32::from_le_bytes(data[14..18].try_into().ok()?);
    let w = i32::from_le_bytes(data[18..22].try_into().ok()?);
    let h_raw = i32::from_le_bytes(data[22..26].try_into().ok()?);
    let top = h_raw < 0;
    // i64 abs: i32::MIN.abs() itself panics.
    let h = (h_raw as i64).abs();
    let bpp = u16::from_le_bytes(data[28..30].try_into().ok()?);
    let compression = if hdr_sz >= 40 && data.len() >= 34 {
        u32::from_le_bytes(data[30..34].try_into().ok()?)
    } else {
        0
    };
    // File-controlled dimensions: reject absurd ones and compute the sizes in
    // i64 so a hostile header cannot overflow (or OOM) the buffers.
    if compression != 0 || w <= 0 || h <= 0 || w > 30_000 || h > 30_000 {
        return None;
    }
    if bpp != 24 && bpp != 32 {
        return None;
    }
    let src_bpp = (bpp / 8) as i64;
    let stride = (w as i64 * src_bpp + 3) / 4 * 4;
    let needed = stride.checked_mul(h)?;
    let need = (off as i64).checked_add(needed)?;
    if need > data.len() as i64 {
        return None;
    }
    let rgb_len = (w as i64 * h * 3) as usize;
    let mut rgb = vec![0u8; rgb_len];
    let w = w as i64;
    for y in 0..h {
        let src_y = if top { y } else { h - 1 - y };
        let start = off as i64 + src_y * stride;
        for x in 0..w {
            let s = (start + x * src_bpp) as usize;
            if s + 2 >= data.len() {
                return None;
            }
            let d = ((y * w + x) * 3) as usize;
            rgb[d] = data[s + 2];
            rgb[d + 1] = data[s + 1];
            rgb[d + 2] = data[s];
        }
    }
    Some(RgbImage {
        w: w as i32,
        h: h as i32,
        rgb,
    })
}

fn mean_rgb(img: &RgbImage) -> (f32, f32, f32) {
    let n = (img.w * img.h).max(1) as f32;
    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    for p in img.rgb.chunks_exact(3) {
        r += p[0] as f32;
        g += p[1] as f32;
        b += p[2] as f32;
    }
    (r / n, g / n, b / n)
}

fn template_texture(img: &RgbImage, mean: (f32, f32, f32)) -> f32 {
    let n = (img.w * img.h).max(1);
    let step = (n / 500).max(1) as usize;
    let mut acc = 0.0f32;
    let mut count = 0.0f32;
    let mut i = 0usize;
    while i < n as usize {
        let o = i * 3;
        acc += (img.rgb[o] as f32 - mean.0).abs()
            + (img.rgb[o + 1] as f32 - mean.1).abs()
            + (img.rgb[o + 2] as f32 - mean.2).abs();
        count += 1.0;
        i += step;
    }
    acc / count.max(1.0)
}

fn is_flat(texture: f32) -> bool {
    texture < 14.0
}

fn patch_mean(screen: &RgbImage, x: i32, y: i32, tw: i32, th: i32) -> (f32, f32, f32) {
    let n = (tw * th).max(1) as f32;
    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    let sw = screen.w;
    for yy in 0..th {
        let row = ((y + yy) * sw + x) * 3;
        for xx in 0..tw {
            let o = (row + xx * 3) as usize;
            r += screen.rgb[o] as f32;
            g += screen.rgb[o + 1] as f32;
            b += screen.rgb[o + 2] as f32;
        }
    }
    (r / n, g / n, b / n)
}

fn hue_conflict(a: (f32, f32, f32), b: (f32, f32, f32)) -> bool {
    let opp = |c: (f32, f32, f32)| {
        let rg = c.0 - c.1;
        let yb = (c.0 + c.1) * 0.5 - c.2;
        (rg, yb, rg.abs() + yb.abs())
    };
    let (rg1, yb1, ch1) = opp(a);
    let (rg2, yb2, ch2) = opp(b);
    if ch1 < 28.0 || ch2 < 28.0 {
        return false;
    }
    if rg1 * rg2 < 0.0 && rg1.abs() + rg2.abs() > 40.0 {
        return true;
    }
    if yb1 * yb2 < 0.0 && yb1.abs() + yb2.abs() > 40.0 {
        return true;
    }
    false
}

fn in_bounds(sw: i32, sh: i32, tw: i32, th: i32, x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && x + tw <= sw && y + th <= sh
}

fn color_ok(
    screen: &RgbImage,
    tmpl: &RgbImage,
    tm: (f32, f32, f32),
    x: i32,
    y: i32,
    max_mean: f32,
) -> bool {
    if !in_bounds(screen.w, screen.h, tmpl.w, tmpl.h, x, y) {
        return false;
    }
    if hue_conflict(tm, patch_mean(screen, x, y, tmpl.w, tmpl.h)) {
        return false;
    }
    let step_y = (tmpl.h / 8).max(1);
    let step_x = (tmpl.w / 8).max(1);
    let mut err = 0.0;
    let mut n = 0.0;
    let mut yy = 0;
    while yy < tmpl.h {
        let mut xx = 0;
        while xx < tmpl.w {
            let toff = ((yy * tmpl.w + xx) * 3) as usize;
            let soff = (((y + yy) * screen.w + (x + xx)) * 3) as usize;
            err += 2.0 * (screen.rgb[soff] as f32 - tmpl.rgb[toff] as f32).abs()
                + 4.0 * (screen.rgb[soff + 1] as f32 - tmpl.rgb[toff + 1] as f32).abs()
                + 3.0 * (screen.rgb[soff + 2] as f32 - tmpl.rgb[toff + 2] as f32).abs();
            n += 1.0;
            xx += step_x;
        }
        yy += step_y;
    }
    n > 0.0 && (err / n) <= max_mean * 1.15
}

#[derive(Clone, Copy)]
struct Sample {
    dx: i32,
    dy: i32,
    toff: usize,
}

fn push_sample(out: &mut Vec<Sample>, seen: &mut [bool], tw: i32, th: i32, x: i32, y: i32) {
    if x < 0 || y < 0 || x >= tw || y >= th {
        return;
    }
    let idx = (y * tw + x) as usize;
    if idx >= seen.len() || seen[idx] {
        return;
    }
    seen[idx] = true;
    out.push(Sample {
        dx: x,
        dy: y,
        toff: idx * 3,
    });
}

fn build_samples(tmpl: &RgbImage, mean: (f32, f32, f32)) -> Vec<Sample> {
    let tw = tmpl.w;
    let th = tmpl.h;
    let n = (tw * th) as usize;
    let mut seen = vec![false; n];
    let mut out = Vec::with_capacity(80);
    push_sample(&mut out, &mut seen, tw, th, 0, 0);
    push_sample(&mut out, &mut seen, tw, th, tw - 1, 0);
    push_sample(&mut out, &mut seen, tw, th, 0, th - 1);
    push_sample(&mut out, &mut seen, tw, th, tw - 1, th - 1);
    push_sample(&mut out, &mut seen, tw, th, tw / 2, th / 2);

    // High-variance first so mismatches abort after 1–2 samples.
    let mut ranked: Vec<(i32, i32, i32)> = Vec::new();
    let step = 2;
    let mut y = 0;
    while y < th {
        let mut x = 0;
        while x < tw {
            let o = ((y * tw + x) * 3) as usize;
            let var = (tmpl.rgb[o] as f32 - mean.0).abs()
                + (tmpl.rgb[o + 1] as f32 - mean.1).abs()
                + (tmpl.rgb[o + 2] as f32 - mean.2).abs();
            ranked.push((var as i32, x, y));
            x += step;
        }
        y += step;
    }
    ranked.sort_unstable_by_key(|b| std::cmp::Reverse(b.0));
    for (var, x, y) in ranked.into_iter().take(24) {
        if var < 8 {
            break;
        }
        push_sample(&mut out, &mut seen, tw, th, x, y);
        if out.len() >= 40 {
            break;
        }
    }
    let sy = (th / 6).max(1);
    let sx = (tw / 6).max(1);
    let mut yy = 0;
    while yy < th {
        let mut xx = 0;
        while xx < tw {
            push_sample(&mut out, &mut seen, tw, th, xx, yy);
            xx += sx;
        }
        yy += sy;
        if out.len() >= 64 {
            break;
        }
    }
    out
}

fn score_at(
    srgb: &[u8],
    sw: i32,
    trgb: &[u8],
    samples: &[Sample],
    x: i32,
    y: i32,
    limit: i32,
) -> i32 {
    let mut err = 0;
    for s in samples {
        let soff = (((y + s.dy) * sw + (x + s.dx)) * 3) as usize;
        err += (srgb[soff] as i32 - trgb[s.toff] as i32).abs()
            + (srgb[soff + 1] as i32 - trgb[s.toff + 1] as i32).abs()
            + (srgb[soff + 2] as i32 - trgb[s.toff + 2] as i32).abs();
        if err > limit {
            return err;
        }
    }
    err
}

#[allow(clippy::too_many_arguments)]
fn refine(
    srgb: &[u8],
    sw: i32,
    sh: i32,
    tw: i32,
    th: i32,
    trgb: &[u8],
    samples: &[Sample],
    x: i32,
    y: i32,
    radius: i32,
) -> (i32, i32, i32) {
    let mut best = (x, y);
    let mut best_e = score_at(srgb, sw, trgb, samples, x, y, i32::MAX);
    let mut dy = -radius;
    while dy <= radius {
        let mut dx = -radius;
        while dx <= radius {
            if dx != 0 || dy != 0 {
                let xx = x + dx;
                let yy = y + dy;
                if in_bounds(sw, sh, tw, th, xx, yy) {
                    let e = score_at(srgb, sw, trgb, samples, xx, yy, best_e);
                    if e < best_e {
                        best_e = e;
                        best = (xx, yy);
                    }
                }
            }
            dx += 1;
        }
        dy += 1;
    }
    (best.0, best.1, best_e)
}

#[allow(clippy::too_many_arguments)]
fn search_box(
    screen: &RgbImage,
    tmpl: &RgbImage,
    samples: &[Sample],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    step: i32,
    limit: i32,
    stop_early: bool,
) -> Option<(i32, i32, i32)> {
    if x1 < x0 || y1 < y0 {
        return None;
    }
    let n = samples.len().max(1) as i32;
    let mut best = None;
    let mut best_e = i32::MAX;
    let mut y = y0;
    while y <= y1 {
        let mut x = x0;
        while x <= x1 {
            let e = score_at(&screen.rgb, screen.w, &tmpl.rgb, samples, x, y, limit);
            if e <= limit && e < best_e {
                best_e = e;
                best = Some((x, y, e));
                // Zero error cannot be beaten: skip the rest of the sweep.
                if e == 0 || (stop_early && e < 6 * n) {
                    return best;
                }
            }
            x += step;
        }
        y += step;
    }
    best
}

fn better_hit(a: Option<(i32, i32, i32)>, b: Option<(i32, i32, i32)>) -> Option<(i32, i32, i32)> {
    match (a, b) {
        (Some(a), Some(b)) => {
            if a.2 <= b.2 {
                Some(a)
            } else {
                Some(b)
            }
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Step-1 near a recorded point. Larger areas use a coarse grid, then a
/// step-1 window around the best cell, so a full-screen miss is not a
/// per-pixel walk.
#[allow(clippy::too_many_arguments)]
fn search_fast(
    screen: &RgbImage,
    tmpl: &RgbImage,
    samples: &[Sample],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    limit: i32,
) -> Option<(i32, i32, i32)> {
    if x1 < x0 || y1 < y0 {
        return None;
    }
    let area = (x1 - x0 + 1) as i64 * (y1 - y0 + 1) as i64;
    if area <= 8_000 {
        return search_box(screen, tmpl, samples, x0, y0, x1, y1, 1, limit, true);
    }
    let step = 4;
    // A one-pixel shift of a sharp picture can miss a strict limit on the
    // coarse grid. Keep a wider net over all even residues mod 4, then confirm
    // with the real limit.
    let loose = limit.saturating_mul(3);
    let first = search_box(screen, tmpl, samples, x0, y0, x1, y1, step, loose, true);
    let near_exact = first
        .map(|hit| hit.2 < 6 * samples.len().max(1) as i32)
        .unwrap_or(false);
    let coarse = if near_exact {
        first
    } else {
        let mut best = first;
        for (ox, oy) in [(2, 0), (0, 2), (2, 2)] {
            let xs = (x0 + ox).min(x1);
            let ys = (y0 + oy).min(y1);
            best = better_hit(
                best,
                search_box(screen, tmpl, samples, xs, ys, x1, y1, step, loose, true),
            );
        }
        best
    }?;
    let (cx, cy, ce) = coarse;
    if let Some(hit) = search_box(
        screen,
        tmpl,
        samples,
        (cx - step).max(x0),
        (cy - step).max(y0),
        (cx + step).min(x1),
        (cy + step).min(y1),
        1,
        limit,
        false,
    ) {
        return Some(hit);
    }
    if ce <= limit {
        Some(coarse)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn accept(
    screen: &RgbImage,
    tmpl: &RgbImage,
    tm: (f32, f32, f32),
    samples: &[Sample],
    x: i32,
    y: i32,
    radius: i32,
    limit: i32,
    max_mean: f32,
) -> Option<(i32, i32, i32, i32)> {
    if !in_bounds(screen.w, screen.h, tmpl.w, tmpl.h, x, y) {
        return None;
    }
    let (x, y, e) = refine(
        &screen.rgb,
        screen.w,
        screen.h,
        tmpl.w,
        tmpl.h,
        &tmpl.rgb,
        samples,
        x,
        y,
        radius,
    );
    if e > limit {
        return None;
    }
    if !color_ok(screen, tmpl, tm, x, y, max_mean) {
        return None;
    }
    Some((x, y, tmpl.w, tmpl.h))
}

/// Each output pixel is the coverage-weighted mean of the source pixels it covers.
fn resample_area(img: &RgbImage, nw: i32, nh: i32) -> RgbImage {
    let nw = nw.max(1);
    let nh = nh.max(1);
    let mut rgb = vec![0u8; (nw * nh * 3) as usize];
    let sw = img.w as f32;
    let sh = img.h as f32;
    for y in 0..nh {
        let fy0 = y as f32 * sh / nh as f32;
        let fy1 = (y + 1) as f32 * sh / nh as f32;
        let iy0 = fy0.floor() as i32;
        let iy1 = (fy1.ceil() as i32).min(img.h);
        for x in 0..nw {
            let fx0 = x as f32 * sw / nw as f32;
            let fx1 = (x + 1) as f32 * sw / nw as f32;
            let ix0 = fx0.floor() as i32;
            let ix1 = (fx1.ceil() as i32).min(img.w);
            let mut acc = [0.0_f32; 3];
            let mut covered = 0.0_f32;
            for iy in iy0..iy1 {
                let y_overlap = fy1.min((iy + 1) as f32) - fy0.max(iy as f32);
                if y_overlap <= 0.0 {
                    continue;
                }
                for ix in ix0..ix1 {
                    let x_overlap = fx1.min((ix + 1) as f32) - fx0.max(ix as f32);
                    if x_overlap <= 0.0 {
                        continue;
                    }
                    let w = x_overlap * y_overlap;
                    let s = ((iy * img.w + ix) * 3) as usize;
                    acc[0] += img.rgb[s] as f32 * w;
                    acc[1] += img.rgb[s + 1] as f32 * w;
                    acc[2] += img.rgb[s + 2] as f32 * w;
                    covered += w;
                }
            }
            let d = ((y * nw + x) * 3) as usize;
            if covered > 0.0 {
                rgb[d] = (acc[0] / covered).round() as u8;
                rgb[d + 1] = (acc[1] / covered).round() as u8;
                rgb[d + 2] = (acc[2] / covered).round() as u8;
            }
        }
    }
    RgbImage { w: nw, h: nh, rgb }
}

fn box_downsample(img: &RgbImage, factor: i32) -> RgbImage {
    let f = factor.max(1);
    // Ceil: the right/bottom partial blocks must survive, or a scaled
    // template loses exactly the pixels that make it distinctive.
    let nw = ((img.w as i64 + f as i64 - 1) / f as i64).max(1) as i32;
    let nh = ((img.h as i64 + f as i64 - 1) / f as i64).max(1) as i32;
    let mut rgb = vec![0u8; (nw * nh * 3) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 3];
            let mut count = 0u32;
            for dy in 0..f {
                for dx in 0..f {
                    let sx = x * f + dx;
                    let sy = y * f + dy;
                    if sx >= img.w || sy >= img.h {
                        continue;
                    }
                    let s = ((sy * img.w + sx) * 3) as usize;
                    acc[0] += img.rgb[s] as u32;
                    acc[1] += img.rgb[s + 1] as u32;
                    acc[2] += img.rgb[s + 2] as u32;
                    count += 1;
                }
            }
            let n = count.max(1) as f32;
            let d = ((y * nw + x) * 3) as usize;
            rgb[d] = (acc[0] as f32 / n).round() as u8;
            rgb[d + 1] = (acc[1] as f32 / n).round() as u8;
            rgb[d + 2] = (acc[2] as f32 / n).round() as u8;
        }
    }
    RgbImage { w: nw, h: nh, rgb }
}

/// Click point inside a match. `ox`/`oy` are pixels in the captured picture
/// and scale with the on-screen size.
pub fn click_on_match(
    hit: (i32, i32, i32, i32),
    ox: Option<i32>,
    oy: Option<i32>,
    template_w: i32,
    template_h: i32,
) -> (i32, i32) {
    let dx = match ox {
        Some(ox) if template_w > 0 => (ox as f32 * hit.2 as f32 / template_w as f32).round() as i32,
        _ => hit.2 / 2,
    };
    let dy = match oy {
        Some(oy) if template_h > 0 => (oy as f32 * hit.3 as f32 / template_h as f32).round() as i32,
        _ => hit.3 / 2,
    };
    (hit.0 + dx, hit.1 + dy)
}

const NEAR_SCALES: [f32; 4] = [0.9, 1.1, 0.8, 1.25];

/// Returns (x, y, w, h) of the match in screen pixels, or None.
/// The captured size is decided with the other nearby sizes in one
/// downsampled pass on a large picture, then confirmed at full resolution.
pub fn find_template(
    screen: &RgbImage,
    tmpl: &RgbImage,
    confidence: f32,
    prefer: Option<(i32, i32)>,
) -> Option<(i32, i32, i32, i32)> {
    let area = (screen.w.saturating_sub(tmpl.w).max(1) as i64)
        * (screen.h.saturating_sub(tmpl.h).max(1) as i64);
    if area <= 8_000 {
        return best_near_scale(screen, tmpl, confidence, prefer);
    }
    find_large(screen, tmpl, confidence, prefer)
}

fn scaled_size(tmpl: &RgbImage, scale: f32) -> Option<(i32, i32)> {
    let nw = (tmpl.w as f32 * scale).round() as i32;
    let nh = (tmpl.h as f32 * scale).round() as i32;
    // Bound the resample allocation: a huge template must not OOM the
    // process before the screen-size guards downstream get a chance.
    if nw < 6 || nh < 6 || nw > 16_384 || nh > 16_384 || (nw == tmpl.w && nh == tmpl.h) {
        None
    } else {
        Some((nw, nh))
    }
}

/// Lowest-error size among the captured size and the nearby sizes.
/// The first size that passes is not enough: a 110% template can sit inside a 125% picture.
fn best_near_scale(
    screen: &RgbImage,
    tmpl: &RgbImage,
    confidence: f32,
    prefer: Option<(i32, i32)>,
) -> Option<(i32, i32, i32, i32)> {
    let mut best: Option<(i32, i32, i32, i32, f32)> = None;
    let mut note = |hit: (i32, i32, i32, i32), norm: f32| {
        let take = best.map(|b| norm < b.4).unwrap_or(true);
        if take {
            best = Some((hit.0, hit.1, hit.2, hit.3, norm));
        }
    };
    if let Some(hit) = find_sized(screen, tmpl, confidence, prefer) {
        note(hit, sample_norm(screen, tmpl, hit.0, hit.1));
    }
    for scale in NEAR_SCALES {
        let Some(sized) = scale_image(tmpl, scale) else {
            continue;
        };
        if sized.w > screen.w || sized.h > screen.h {
            continue;
        }
        let shifted =
            prefer.map(|(px, py)| (px - (sized.w - tmpl.w) / 2, py - (sized.h - tmpl.h) / 2));
        if let Some(hit) = find_sized(screen, &sized, confidence, shifted) {
            note(hit, sample_norm(screen, &sized, hit.0, hit.1));
        }
    }
    let hit = best?;
    let rect = (hit.0, hit.1, hit.2, hit.3);
    if lost_to_double(screen, tmpl, confidence, rect) {
        return None;
    }
    Some(rect)
}

fn scale_image(tmpl: &RgbImage, scale: f32) -> Option<RgbImage> {
    if (scale - 1.0).abs() < 0.001 {
        return Some(tmpl.clone());
    }
    let (nw, nh) = scaled_size(tmpl, scale)?;
    Some(resample_area(tmpl, nw, nh))
}

/// Coarse peak on the shared downsampled screen, then a step-1 window.
/// `norm` is the mean sample error so a smaller picture cannot win on a
/// shorter sample list.
fn probe_scale(
    screen: &RgbImage,
    small: &RgbImage,
    sized: &RgbImage,
    factor: i32,
    confidence: f32,
) -> Option<(i32, i32, i32, i32, f32)> {
    if sized.w < 6 || sized.h < 6 || sized.w > screen.w || sized.h > screen.h {
        return None;
    }
    let coarse = box_downsample(sized, factor);
    if coarse.w < 1 || coarse.h < 1 || coarse.w > small.w || coarse.h > small.h {
        return None;
    }
    let c_mean = mean_rgb(&coarse);
    let c_samples = build_samples(&coarse, c_mean);
    let mean = mean_rgb(sized);
    let samples = build_samples(sized, mean);
    if c_samples.is_empty() || samples.is_empty() {
        return None;
    }
    let max_mean = 255.0 * 3.0 * (1.0 - confidence);
    let max_avg = max_mean as i32;
    let limit = max_avg.saturating_mul(samples.len() as i32);
    let loose = max_avg
        .saturating_mul(c_samples.len() as i32)
        .saturating_mul(3);
    let (cx, cy, _) = search_box(
        small,
        &coarse,
        &c_samples,
        0,
        0,
        small.w - coarse.w,
        small.h - coarse.h,
        1,
        loose,
        false,
    )?;
    let pad = factor.saturating_mul(2);
    let origin_x = cx * factor;
    let origin_y = cy * factor;
    let (x, y, _) = search_box(
        screen,
        sized,
        &samples,
        (origin_x - pad).max(0),
        (origin_y - pad).max(0),
        (origin_x + pad).min(screen.w - sized.w),
        (origin_y + pad).min(screen.h - sized.h),
        1,
        limit,
        false,
    )?;
    let hit = accept(screen, sized, mean, &samples, x, y, 1, limit, max_mean)?;
    let err = score_at(
        &screen.rgb,
        screen.w,
        &sized.rgb,
        &samples,
        hit.0,
        hit.1,
        i32::MAX,
    );
    let norm = err as f32 / samples.len() as f32;
    Some((hit.0, hit.1, hit.2, hit.3, norm))
}

/// One downsampled walk per nearby size, then a step-1 check only around
/// that size's best cell. A picture about twice as large loses to that
/// larger size and is not a match.
fn find_large(
    screen: &RgbImage,
    tmpl: &RgbImage,
    confidence: f32,
    prefer: Option<(i32, i32)>,
) -> Option<(i32, i32, i32, i32)> {
    // The area heuristic above can route an oversized template here; without
    // this guard `screen.w - tmpl.w` goes negative and the scans panic.
    if tmpl.w > screen.w || tmpl.h > screen.h {
        return None;
    }
    let confidence = confidence.clamp(0.60, 0.99);
    let tm = mean_rgb(tmpl);
    let flat = is_flat(template_texture(tmpl, tm));
    // A flat fill stays on the recorded point. A textured picture still has to
    // compete with the nearby sizes, or an 80% copy loses to a loose exact hit.
    if flat {
        if let Some((px, py)) = prefer {
            let extra = tmpl.w.max(tmpl.h).max(16) + 4;
            let x0 = (px - extra).max(0);
            let y0 = (py - extra).max(0);
            let w = (tmpl.w + extra * 2).min(screen.w - x0);
            let h = (tmpl.h + extra * 2).min(screen.h - y0);
            if let Some(local) = screen.crop(x0, y0, w, h) {
                if let Some(hit) = find_sized(&local, tmpl, confidence, Some((px - x0, py - y0))) {
                    let abs = (hit.0 + x0, hit.1 + y0, hit.2, hit.3);
                    if lost_to_double(screen, tmpl, confidence, abs) {
                        return None;
                    }
                    return Some(abs);
                }
            }
            return None;
        }
    }
    if flat && prefer.is_none() {
        let samples = build_samples(tmpl, tm);
        if !samples.is_empty() {
            let max_mean = 255.0 * 3.0 * (1.0 - confidence);
            let limit = (max_mean as i32).saturating_mul(samples.len() as i32);
            let x1 = screen.w - tmpl.w;
            let y1 = screen.h - tmpl.h;
            let a = score_at(&screen.rgb, screen.w, &tmpl.rgb, &samples, 0, 0, limit);
            let b = score_at(&screen.rgb, screen.w, &tmpl.rgb, &samples, x1, y1, limit);
            if a <= limit && b <= limit {
                return None;
            }
        }
    }

    let factor = 4;
    let small = box_downsample(screen, factor);
    let mut best: Option<(i32, i32, i32, i32, f32)> = None;
    for scale in [1.0_f32, 0.9, 1.1, 0.8, 1.25] {
        let Some(sized) = scale_image(tmpl, scale) else {
            continue;
        };
        let Some(hit) = probe_scale(screen, &small, &sized, factor, confidence) else {
            continue;
        };
        let take = best.map(|b| hit.4 < b.4).unwrap_or(true);
        if take {
            best = Some(hit);
        }
    }
    let best = best?;
    if let Some(doubled) = scale_image(tmpl, 2.0) {
        if let Some(hit) = probe_scale(screen, &small, &doubled, factor, confidence) {
            if hit.4 < best.4 {
                return None;
            }
        }
    }
    Some((best.0, best.1, best.2, best.3))
}

fn sample_norm(screen: &RgbImage, sized: &RgbImage, x: i32, y: i32) -> f32 {
    let samples = build_samples(sized, mean_rgb(sized));
    if samples.is_empty() {
        return f32::MAX;
    }
    let err = score_at(&screen.rgb, screen.w, &sized.rgb, &samples, x, y, i32::MAX);
    err as f32 / samples.len() as f32
}

/// The on-screen picture is about twice the captured size, so `hit` is not a match.
fn lost_to_double(
    screen: &RgbImage,
    tmpl: &RgbImage,
    confidence: f32,
    hit: (i32, i32, i32, i32),
) -> bool {
    let Some(doubled) = scale_image(tmpl, 2.0) else {
        return false;
    };
    let confidence = confidence.clamp(0.60, 0.99);
    let big_norm = if (screen.w.saturating_sub(doubled.w).max(1) as i64)
        * (screen.h.saturating_sub(doubled.h).max(1) as i64)
        <= 8_000
    {
        let Some(big) = find_sized(screen, &doubled, confidence, None) else {
            return false;
        };
        sample_norm(screen, &doubled, big.0, big.1)
    } else {
        let small = box_downsample(screen, 4);
        let Some(big) = probe_scale(screen, &small, &doubled, 4, confidence) else {
            return false;
        };
        big.4
    };
    let sized = if hit.2 == tmpl.w && hit.3 == tmpl.h {
        tmpl.clone()
    } else {
        resample_area(tmpl, hit.2, hit.3)
    };
    big_norm < sample_norm(screen, &sized, hit.0, hit.1)
}

fn find_sized(
    screen: &RgbImage,
    tmpl: &RgbImage,
    confidence: f32,
    prefer: Option<(i32, i32)>,
) -> Option<(i32, i32, i32, i32)> {
    let confidence = confidence.clamp(0.60, 0.99);
    if tmpl.w < 6 || tmpl.h < 6 || tmpl.w > screen.w || tmpl.h > screen.h {
        return None;
    }
    let tm = mean_rgb(tmpl);
    let flat = is_flat(template_texture(tmpl, tm));
    let samples = build_samples(tmpl, tm);
    if samples.is_empty() {
        return None;
    }
    let max_mean = 255.0 * 3.0 * (1.0 - confidence);
    let max_avg = max_mean as i32;
    let limit = max_avg * samples.len() as i32;

    if screen.w == tmpl.w && screen.h == tmpl.h {
        return if color_ok(screen, tmpl, tm, 0, 0, max_mean) {
            Some((0, 0, tmpl.w, tmpl.h))
        } else {
            None
        };
    }

    if flat && prefer.is_none() {
        let x1 = screen.w - tmpl.w;
        let y1 = screen.h - tmpl.h;
        let a = score_at(&screen.rgb, screen.w, &tmpl.rgb, &samples, 0, 0, limit);
        let b = score_at(&screen.rgb, screen.w, &tmpl.rgb, &samples, x1, y1, limit);
        if a <= limit && b <= limit {
            return None;
        }
    }

    if let Some((px, py)) = prefer {
        if in_bounds(screen.w, screen.h, tmpl.w, tmpl.h, px, py) {
            let e = score_at(&screen.rgb, screen.w, &tmpl.rgb, &samples, px, py, limit);
            if e <= limit {
                if let Some(hit) = accept(screen, tmpl, tm, &samples, px, py, 2, limit, max_mean) {
                    return Some(hit);
                }
            }
        }
        let extra = if flat {
            (tmpl.w.max(tmpl.h) / 4).max(8)
        } else {
            tmpl.w.max(tmpl.h).max(16)
        };
        let x0 = (px - extra).max(0);
        let y0 = (py - extra).max(0);
        let x1 = (px + extra).min(screen.w - tmpl.w);
        let y1 = (py + extra).min(screen.h - tmpl.h);
        if let Some((x, y, _)) = search_fast(screen, tmpl, &samples, x0, y0, x1, y1, limit) {
            if let Some(hit) = accept(screen, tmpl, tm, &samples, x, y, 1, limit, max_mean) {
                // accept() refines by 1 px, which may step just outside the
                // window the search scanned.
                if !flat || ((hit.0 - px).abs() <= extra + 1 && (hit.1 - py).abs() <= extra + 1) {
                    return Some(hit);
                }
            }
        }
        let covered = x0 <= 0 && y0 <= 0 && x1 >= screen.w - tmpl.w && y1 >= screen.h - tmpl.h;
        if flat || covered {
            return None;
        }
    }

    let (x, y, _) = search_fast(
        screen,
        tmpl,
        &samples,
        0,
        0,
        screen.w - tmpl.w,
        screen.h - tmpl.h,
        limit,
    )?;
    accept(screen, tmpl, tm, &samples, x, y, 1, limit, max_mean)
}

pub fn resolve_image(path: &str, script_dir: Option<&std::path::Path>) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let p = std::path::Path::new(path);
    if p.is_file() {
        return Some(path.to_string());
    }
    if let Some(dir) = script_dir {
        let c = dir.join(path);
        if c.is_file() {
            return Some(c.to_string_lossy().into_owned());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let c = dir.join(path);
            if c.is_file() {
                return Some(c.to_string_lossy().into_owned());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: i32, h: i32, rgb: (u8, u8, u8)) -> RgbImage {
        let mut buf = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..(w * h) {
            buf.extend_from_slice(&[rgb.0, rgb.1, rgb.2]);
        }
        RgbImage { w, h, rgb: buf }
    }

    fn paste(screen: &RgbImage, tmpl: &RgbImage, x: i32, y: i32) -> RgbImage {
        let mut out = screen.clone();
        for yy in 0..tmpl.h {
            for xx in 0..tmpl.w {
                if let Some((r, g, b)) = tmpl.get(xx, yy) {
                    let i = (((y + yy) * out.w + (x + xx)) * 3) as usize;
                    if i + 2 < out.rgb.len() {
                        out.rgb[i] = r;
                        out.rgb[i + 1] = g;
                        out.rgb[i + 2] = b;
                    }
                }
            }
        }
        out
    }

    #[test]
    fn finds_exact_patch() {
        let red = solid(12, 12, (220, 30, 30));
        let screen = paste(&solid(48, 36, (20, 20, 20)), &red, 10, 8);
        let hit = find_template(&screen, &red, 0.90, None);
        assert!(hit.is_some());
        let hit = hit.unwrap();
        assert_eq!((hit.0, hit.1), (10, 8));
    }

    #[test]
    fn rejects_other_hue() {
        let red = solid(12, 12, (220, 30, 30));
        let blue = solid(12, 12, (30, 30, 220));
        let screen = paste(&solid(48, 36, (20, 20, 20)), &blue, 10, 8);
        assert!(find_template(&screen, &red, 0.90, None).is_none());
    }

    #[test]
    fn rejects_missing() {
        let needle = solid(12, 12, (200, 180, 20));
        let screen = solid(40, 30, (10, 10, 10));
        assert!(find_template(&screen, &needle, 0.90, None).is_none());
    }

    #[test]
    fn sharp_template_off_the_old_coarse_residues_is_still_found() {
        // A 2x2 checkerboard flips every sample under a 2 px shift, so the old
        // two-pass coarse grid (residues (0,0) and (2,2) mod 4) had no probe
        // within reach of a match at residue (2,0).
        let side = 70;
        let mut tb = Vec::with_capacity((side * side * 3) as usize);
        for y in 0..side {
            for x in 0..side {
                let v = if (x / 2 + y / 2) % 2 == 0 { 255u8 } else { 0u8 };
                tb.extend_from_slice(&[v, v, v]);
            }
        }
        let tmpl = RgbImage {
            w: side,
            h: side,
            rgb: tb,
        };
        let mut screen = solid(159, 159, (0, 0, 0));
        for y in 0..side {
            for x in 0..side {
                let i = (((36 + y) * 159 + (78 + x)) * 3) as usize;
                let j = ((y * side + x) * 3) as usize;
                screen.rgb[i..i + 3].copy_from_slice(&tmpl.rgb[j..j + 3]);
            }
        }
        // A prefer near the middle keeps the clipped search window large
        // enough (> 8000 cells) to take the coarse branch.
        let hit = find_template(&screen, &tmpl, 0.9, Some((45, 45)))
            .expect("match at an unprobed coarse residue must be found");
        assert_eq!((hit.0, hit.1), (78, 36));
        assert_eq!((hit.2, hit.3), (side, side));
    }

    #[test]
    fn scaled_size_bounds_the_resample_allocation() {
        let big = RgbImage {
            w: 30000,
            h: 30000,
            rgb: Vec::new(),
        };
        // A 1.25x resample of this used to try a ~4 GB allocation.
        assert!(scaled_size(&big, 1.25).is_none());
        assert!(scaled_size(&big, 2.0).is_none());
        // Normal scaling still works.
        assert_eq!(
            scaled_size(&solid(100, 100, (0, 0, 0)), 1.1).map(|s| (s.0, s.1)),
            Some((110, 110))
        );
    }

    #[test]
    fn box_downsample_keeps_partial_edge_blocks() {
        // 6x6 with factor 4: the right column pair must survive as a second
        // block, averaged over the 8 pixels it actually covers.
        let mut rgb = Vec::with_capacity(6 * 6 * 3);
        for _y in 0..6 {
            for x in 0..6 {
                let v = if x < 5 { 255u8 } else { 0u8 };
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        let out = box_downsample(&RgbImage { w: 6, h: 6, rgb }, 4);
        assert_eq!((out.w, out.h), (2, 2));
        let get = |x: i32, y: i32| out.rgb[((y * out.w + x) * 3) as usize];
        assert_eq!(get(0, 0), 255);
        // 4 white + 4 black pixels -> mean 127.5 -> 128.
        assert_eq!(get(1, 0), 128);
    }

    #[test]
    fn oversized_template_is_rejected_without_panicking() {
        // The area heuristic routes this to find_large even though the
        // template is wider than the screen.
        let screen = solid(100, 8100, (60, 60, 60));
        let tmpl = solid(200, 50, (60, 60, 60));
        assert!(find_template(&screen, &tmpl, 0.9, None).is_none());
    }

    #[test]
    fn hostile_bmp_headers_are_rejected_not_panicked() {
        let dir = std::env::temp_dir();
        let p1 = dir.join(format!("amk_hostile1_{}.bmp", std::process::id()));
        let mut d = vec![0u8; 54];
        d[0] = b'B';
        d[1] = b'M';
        d[18..22].copy_from_slice(&0x2000_0000i32.to_le_bytes());
        d[22..26].copy_from_slice(&2i32.to_le_bytes());
        d[28..30].copy_from_slice(&24u16.to_le_bytes());
        std::fs::write(&p1, &d).unwrap();
        assert!(load_bmp24(p1.to_str().unwrap()).is_none(), "width overflow");

        let p2 = dir.join(format!("amk_hostile2_{}.bmp", std::process::id()));
        let mut d = vec![0u8; 54];
        d[0] = b'B';
        d[1] = b'M';
        d[18..22].copy_from_slice(&2i32.to_le_bytes());
        d[22..26].copy_from_slice(&i32::MIN.to_le_bytes());
        d[28..30].copy_from_slice(&24u16.to_le_bytes());
        std::fs::write(&p2, &d).unwrap();
        assert!(
            load_bmp24(p2.to_str().unwrap()).is_none(),
            "i32::MIN height"
        );
        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
    }

    #[test]
    fn rejects_tiny_template() {
        let tiny = solid(4, 4, (255, 0, 0));
        let screen = solid(40, 30, (0, 0, 0));
        assert!(find_template(&screen, &tiny, 0.90, None).is_none());
    }

    #[test]
    fn rejects_green_when_seeking_red() {
        let red = solid(12, 12, (220, 30, 30));
        let green = solid(12, 12, (30, 200, 40));
        let screen = paste(&solid(48, 36, (20, 20, 20)), &green, 10, 8);
        assert!(find_template(&screen, &red, 0.85, None).is_none());
    }

    #[test]
    fn exact_size_compare() {
        let img = solid(16, 12, (200, 40, 40));
        assert_eq!(find_template(&img, &img, 0.90, None), Some((0, 0, 16, 12)));
        let other = solid(16, 12, (40, 40, 200));
        assert!(find_template(&other, &img, 0.90, None).is_none());
    }

    fn checker(w: i32, h: i32, a: (u8, u8, u8), b: (u8, u8, u8)) -> RgbImage {
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let on = ((x / 2) + (y / 2)) % 2 == 0;
                let (r, g, bch) = if on { a } else { b };
                let i = ((y * w + x) * 3) as usize;
                rgb[i] = r;
                rgb[i + 1] = g;
                rgb[i + 2] = bch;
            }
        }
        RgbImage { w, h, rgb }
    }

    #[test]
    fn finds_odd_origin_exactly() {
        let mark = checker(14, 12, (220, 40, 40), (30, 30, 80));
        let screen = paste(&solid(64, 48, (18, 18, 18)), &mark, 11, 9);
        let hit = find_template(&screen, &mark, 0.90, None).unwrap();
        assert_eq!((hit.0, hit.1, hit.2, hit.3), (11, 9, 14, 12));
    }

    #[test]
    fn prefer_recovers_when_image_shifted_a_few_pixels() {
        let mark = checker(14, 12, (220, 40, 40), (30, 30, 80));
        let screen = paste(&solid(80, 60, (18, 18, 18)), &mark, 30, 18);
        let hit = find_template(&screen, &mark, 0.88, Some((26, 14))).unwrap();
        assert_eq!((hit.0, hit.1), (30, 18));
    }

    #[test]
    fn flat_prefers_recorded_blob_not_other_copy() {
        let g = solid(12, 12, (40, 180, 90));
        let mut screen = solid(80, 60, (12, 12, 12));
        screen = paste(&screen, &g, 4, 4);
        screen = paste(&screen, &g, 50, 30);
        let hit = find_template(&screen, &g, 0.80, Some((50, 30))).unwrap();
        assert_eq!((hit.0, hit.1), (50, 30));
    }

    #[test]
    fn flat_template_stays_at_prefer_not_topleft() {
        let needle = solid(12, 12, (40, 180, 90));
        let screen = solid(80, 60, (40, 180, 90));
        let hit = find_template(&screen, &needle, 0.80, Some((40, 20))).unwrap();
        assert_eq!((hit.0, hit.1), (40, 20));
    }

    #[test]
    fn uniform_fill_without_prefer_is_not_a_match() {
        let needle = solid(12, 12, (40, 180, 90));
        let screen = solid(80, 60, (40, 180, 90));
        assert!(find_template(&screen, &needle, 0.80, None).is_none());
    }

    #[test]
    fn thin_feature_off_the_coarse_grid() {
        let mut needle = solid(16, 16, (36, 36, 36));
        let dot = solid(3, 3, (240, 30, 30));
        needle = paste(&needle, &dot, 3, 5);
        let screen = paste(&solid(70, 50, (20, 20, 20)), &needle, 17, 13);
        let hit = find_template(&screen, &needle, 0.90, None).unwrap();
        assert_eq!((hit.0, hit.1), (17, 13));
    }

    #[test]
    fn accepts_same_hue_darker() {
        let red = solid(12, 12, (220, 30, 30));
        let dark = solid(12, 12, (180, 24, 24));
        let screen = paste(&solid(48, 36, (20, 20, 20)), &dark, 10, 8);
        let hit = find_template(&screen, &red, 0.82, None);
        assert!(hit.is_some());
    }

    #[test]
    fn load_bmp32_keeps_rgb() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("amk_bmp32_{}.bmp", std::process::id()));
        let w: i32 = 8;
        let h: i32 = 6;
        let stride = ((w * 4 + 3) / 4) * 4;
        let mut body = Vec::new();
        for _y in 0..h {
            let mut row = vec![0u8; stride as usize];
            for x in 0..w {
                let i = (x * 4) as usize;
                row[i] = 9;
                row[i + 1] = 8;
                row[i + 2] = 7;
                row[i + 3] = 255;
            }
            body.extend_from_slice(&row);
        }
        let off: u32 = 54;
        let size = off + body.len() as u32;
        let mut file = Vec::new();
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&size.to_le_bytes());
        file.extend_from_slice(&[0u8; 4]);
        file.extend_from_slice(&off.to_le_bytes());
        file.extend_from_slice(&40u32.to_le_bytes());
        file.extend_from_slice(&w.to_le_bytes());
        file.extend_from_slice(&h.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&[0u8; 24]);
        file.extend_from_slice(&body);
        std::fs::write(&path, &file).unwrap();
        let back = load_bmp24(path.to_str().unwrap()).expect("32-bit bmp");
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.w, 8);
        assert_eq!(back.h, 6);
        assert_eq!(&back.rgb[0..3], &[7, 8, 9]);
    }

    #[test]
    fn timeout_zero_means_try_once() {
        assert!(match_try_once(0));
        assert!(!match_try_once(1));
        assert!(!match_try_once(3000));
    }

    #[test]
    fn search_pad_covers_template() {
        assert!(smart_search_pad(12, 12) >= 40);
        assert_eq!(smart_search_pad(80, 20), 80);
    }

    #[test]
    fn large_screen_hits_exact_pixel() {
        let mark = checker(20, 16, (210, 50, 40), (20, 40, 160));
        let screen = paste(&solid(320, 200, (16, 16, 16)), &mark, 201, 87);
        let tm = mean_rgb(&mark);
        let samples = build_samples(&mark, tm);
        let exact = score_at(
            &screen.rgb,
            screen.w,
            &mark.rgb,
            &samples,
            201,
            87,
            i32::MAX,
        );
        assert_eq!(exact, 0, "exact SAD should be 0, got {exact}");
        assert!(
            color_ok(&screen, &mark, tm, 201, 87, 255.0 * 3.0 * 0.10),
            "color_ok at exact"
        );
        let hit = find_template(&screen, &mark, 0.90, Some((196, 82)));
        assert_eq!(hit.map(|h| (h.0, h.1)), Some((201, 87)), "prefer path");
        let hit = find_template(&screen, &mark, 0.90, None);
        assert_eq!(hit.map(|h| (h.0, h.1)), Some((201, 87)), "full path");
    }

    /// Coverage-weighted mean. Written here so the fixture is not the matcher's resizer.
    fn area_average(src: &RgbImage, nw: i32, nh: i32) -> RgbImage {
        let mut rgb = vec![0u8; (nw * nh * 3) as usize];
        let sw = src.w as f32;
        let sh = src.h as f32;
        for y in 0..nh {
            let top = y as f32 * sh / nh as f32;
            let bot = (y + 1) as f32 * sh / nh as f32;
            let iy0 = top.floor() as i32;
            let iy1 = (bot.ceil() as i32).min(src.h);
            for x in 0..nw {
                let left = x as f32 * sw / nw as f32;
                let right = (x + 1) as f32 * sw / nw as f32;
                let ix0 = left.floor() as i32;
                let ix1 = (right.ceil() as i32).min(src.w);
                let mut sum = [0.0_f32; 3];
                let mut weight = 0.0_f32;
                for iy in iy0..iy1 {
                    let y_part = bot.min((iy + 1) as f32) - top.max(iy as f32);
                    if y_part <= 0.0 {
                        continue;
                    }
                    for ix in ix0..ix1 {
                        let x_part = right.min((ix + 1) as f32) - left.max(ix as f32);
                        if x_part <= 0.0 {
                            continue;
                        }
                        let cover = x_part * y_part;
                        let px = src.get(ix, iy).unwrap();
                        sum[0] += px.0 as f32 * cover;
                        sum[1] += px.1 as f32 * cover;
                        sum[2] += px.2 as f32 * cover;
                        weight += cover;
                    }
                }
                let o = ((y * nw + x) * 3) as usize;
                if weight > 0.0 {
                    rgb[o] = (sum[0] / weight).round() as u8;
                    rgb[o + 1] = (sum[1] / weight).round() as u8;
                    rgb[o + 2] = (sum[2] / weight).round() as u8;
                }
            }
        }
        RgbImage { w: nw, h: nh, rgb }
    }

    fn unique_mark() -> RgbImage {
        let mut mark = solid(16, 16, (28, 32, 36));
        mark = paste(&mark, &solid(4, 3, (230, 40, 36)), 1, 2);
        mark = paste(&mark, &solid(3, 5, (36, 48, 220)), 9, 7);
        mark = paste(&mark, &solid(2, 2, (240, 210, 30)), 6, 12);
        mark
    }

    #[test]
    fn finds_a_larger_onscreen_copy() {
        let mark = checker(20, 16, (200, 40, 40), (30, 30, 90));
        let big = area_average(&mark, 25, 20);
        let screen = paste(&solid(90, 70, (16, 16, 16)), &big, 18, 14);
        let hit = find_template(&screen, &mark, 0.90, None).unwrap();
        assert_eq!((hit.0, hit.1, hit.2, hit.3), (18, 14, 25, 20));
    }

    #[test]
    fn finds_a_smaller_onscreen_copy() {
        let mark = checker(20, 16, (200, 40, 40), (30, 30, 90));
        let small = area_average(&mark, 16, 13);
        let screen = paste(&solid(80, 60, (16, 16, 16)), &small, 22, 11);
        let hit = find_template(&screen, &mark, 0.90, None).unwrap();
        assert_eq!((hit.0, hit.1, hit.2, hit.3), (22, 11, 16, 13));
    }

    #[test]
    fn same_size_hit_is_exact_off_grid_and_a_miss_is_not_much_slower() {
        let mark = unique_mark();
        let origin = (733, 401);
        let screen = paste(&solid(960, 540, (12, 14, 18)), &mark, origin.0, origin.1);
        let started = std::time::Instant::now();
        let hit = find_template(&screen, &mark, 0.90, None).unwrap();
        let hit_ms = started.elapsed();
        assert_eq!((hit.0, hit.1), origin, "off-grid origin");
        assert!(
            hit_ms.as_millis() < 800,
            "hit took {} ms",
            hit_ms.as_millis()
        );

        let absent = solid(960, 540, (12, 14, 18));
        let miss_started = std::time::Instant::now();
        assert!(find_template(&absent, &mark, 0.90, None).is_none());
        let miss_ms = miss_started.elapsed();
        assert!(
            miss_ms <= hit_ms.saturating_mul(2),
            "miss {} ms, hit {} ms",
            miss_ms.as_millis(),
            hit_ms.as_millis()
        );

        let mut wrong = mark.clone();
        for px in wrong.rgb.chunks_exact_mut(3) {
            px.swap(0, 2);
        }
        let wrong_screen = paste(&solid(960, 540, (12, 14, 18)), &wrong, origin.0, origin.1);
        let color_started = std::time::Instant::now();
        assert!(find_template(&wrong_screen, &mark, 0.90, None).is_none());
        let color_ms = color_started.elapsed();
        assert!(
            color_ms <= hit_ms.saturating_mul(2),
            "wrong color {} ms, hit {} ms",
            color_ms.as_millis(),
            hit_ms.as_millis()
        );
    }

    #[test]
    fn area_averaged_scales_match_the_true_rectangle_and_center() {
        let mark = unique_mark();
        let hit_screen = paste(&solid(960, 540, (12, 14, 18)), &mark, 733, 401);
        let exact_started = std::time::Instant::now();
        let exact = find_template(&hit_screen, &mark, 0.90, None).unwrap();
        let exact_ms = exact_started.elapsed();
        assert_eq!((exact.0, exact.1), (733, 401));

        let big = area_average(&mark, 20, 20);
        let big_at = (180, 90);
        let big_screen = paste(&solid(960, 540, (12, 14, 18)), &big, big_at.0, big_at.1);
        let big_started = std::time::Instant::now();
        let big_hit = find_template(&big_screen, &mark, 0.90, None).unwrap();
        let big_ms = big_started.elapsed();
        assert_eq!(
            (big_hit.0, big_hit.1, big_hit.2, big_hit.3),
            (big_at.0, big_at.1, big.w, big.h)
        );
        assert_eq!(
            click_on_match(big_hit, Some(mark.w / 2), Some(mark.h / 2), mark.w, mark.h),
            (big_at.0 + big.w / 2, big_at.1 + big.h / 2)
        );
        assert!(
            big_ms <= exact_ms.saturating_mul(2),
            "125% {} ms, exact {} ms",
            big_ms.as_millis(),
            exact_ms.as_millis()
        );

        let small = area_average(&mark, 13, 13);
        let small_at = (240, 160);
        let small_screen = paste(
            &solid(960, 540, (12, 14, 18)),
            &small,
            small_at.0,
            small_at.1,
        );
        let small_hit = find_template(&small_screen, &mark, 0.90, None).unwrap();
        assert_eq!(
            (small_hit.0, small_hit.1, small_hit.2, small_hit.3),
            (small_at.0, small_at.1, small.w, small.h)
        );
        let (cx, cy) = click_on_match(
            small_hit,
            Some(mark.w / 2),
            Some(mark.h / 2),
            mark.w,
            mark.h,
        );
        assert!((cx - (small_at.0 + small.w / 2)).abs() <= 1);
        assert!((cy - (small_at.1 + small.h / 2)).abs() <= 1);

        let big_prefer = find_template(&big_screen, &mark, 0.90, Some(big_at)).unwrap();
        assert_eq!(
            (big_prefer.0, big_prefer.1, big_prefer.2, big_prefer.3),
            (big_at.0, big_at.1, big.w, big.h)
        );
        let small_prefer = find_template(&small_screen, &mark, 0.90, Some(small_at)).unwrap();
        assert_eq!(
            (
                small_prefer.0,
                small_prefer.1,
                small_prefer.2,
                small_prefer.3
            ),
            (small_at.0, small_at.1, small.w, small.h)
        );

        // First Click TM grab is a 96×96 pad: (96-16)×(96-16) = 6400 positions.
        let pad = solid(96, 96, (12, 14, 18));
        assert!((pad.w - mark.w) as i64 * (pad.h - mark.h) as i64 <= 8_000);
        let at = (40, 40);
        for copy in [&big, &small] {
            let local = paste(&pad, copy, at.0, at.1);
            for prefer in [None, Some(at)] {
                let hit = find_template(&local, &mark, 0.90, prefer).unwrap();
                assert_eq!((hit.0, hit.1, hit.2, hit.3), (at.0, at.1, copy.w, copy.h));
            }
        }
    }

    #[test]
    fn double_size_and_wrong_color_are_not_matches() {
        let mark = unique_mark();
        let doubled = area_average(&mark, mark.w * 2, mark.h * 2);
        let screen = paste(&solid(960, 540, (12, 14, 18)), &doubled, 120, 80);
        assert!(find_template(&screen, &mark, 0.90, None).is_none());
        assert!(find_template(&screen, &mark, 0.90, Some((120, 80))).is_none());
        let small = paste(&solid(100, 80, (12, 14, 18)), &doubled, 20, 16);
        assert!((small.w - mark.w) as i64 * (small.h - mark.h) as i64 <= 8_000);
        assert!(find_template(&small, &mark, 0.90, None).is_none());
        assert!(find_template(&small, &mark, 0.90, Some((20, 16))).is_none());
        let mut swapped = mark.clone();
        for px in swapped.rgb.chunks_exact_mut(3) {
            px.swap(0, 2);
        }
        let wrong = paste(&solid(960, 540, (12, 14, 18)), &swapped, 120, 80);
        assert!(find_template(&wrong, &mark, 0.90, None).is_none());
    }

    #[test]
    fn true_picture_beats_a_worse_lookalike() {
        let mark = unique_mark();
        let mut look = mark.clone();
        for px in look.rgb.chunks_exact_mut(3) {
            px[0] /= 3;
            px[1] = px[1] / 3 + 40;
            px[2] /= 2;
        }
        let mut screen = solid(960, 540, (12, 14, 18));
        screen = paste(&screen, &look, 30, 24);
        screen = paste(&screen, &mark, 510, 260);
        let hit = find_template(&screen, &mark, 0.90, None).unwrap();
        assert_eq!((hit.0, hit.1, hit.2, hit.3), (510, 260, mark.w, mark.h));
    }

    #[test]
    fn click_tracks_the_center_when_the_button_is_larger() {
        let hit = (100, 80, 30, 24);
        assert_eq!(click_on_match(hit, Some(10), Some(8), 20, 16), (115, 92));
        assert_eq!(click_on_match(hit, None, None, 20, 16), (115, 92));
    }

    #[test]
    fn wide_screen_search_finishes_quickly() {
        let mut mark = solid(24, 18, (32, 32, 32));
        mark = paste(&mark, &solid(4, 4, (240, 24, 24)), 3, 4);
        mark = paste(&mark, &solid(3, 5, (24, 24, 230)), 15, 10);
        let screen = paste(&solid(960, 540, (14, 14, 14)), &mark, 701, 333);
        let started = std::time::Instant::now();
        let hit = find_template(&screen, &mark, 0.90, None);
        let ms = started.elapsed().as_millis();
        assert_eq!(hit.map(|h| (h.0, h.1)), Some((701, 333)), "took {ms} ms");
        assert!(ms < 800, "search took {ms} ms");
    }

    #[test]
    fn crop_keeps_top_left_pixel() {
        let mut img = solid(10, 8, (1, 2, 3));
        img.rgb[0] = 9;
        img.rgb[1] = 8;
        img.rgb[2] = 7;
        let c = img.crop(0, 0, 3, 2).unwrap();
        assert_eq!((c.w, c.h), (3, 2));
        assert_eq!(c.get(0, 0), Some((9, 8, 7)));
    }

    #[test]
    fn save_load_bmp24_roundtrip() {
        let img = checker(6, 5, (200, 10, 10), (10, 200, 10));
        let path = std::env::temp_dir().join(format!("amk_save_{}.bmp", std::process::id()));
        assert!(save_bmp24(path.to_str().unwrap(), &img));
        let loaded = load_bmp24(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((loaded.w, loaded.h), (img.w, img.h));
        assert_eq!(loaded.get(0, 0), img.get(0, 0));
        assert_eq!(loaded.get(1, 1), img.get(1, 1));
    }
}
