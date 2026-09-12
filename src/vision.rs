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
    let h = h_raw.abs();
    let bpp = u16::from_le_bytes(data[28..30].try_into().ok()?);
    let compression = if hdr_sz >= 40 && data.len() >= 34 {
        u32::from_le_bytes(data[30..34].try_into().ok()?)
    } else {
        0
    };
    if compression != 0 || w <= 0 || h <= 0 {
        return None;
    }
    if bpp != 24 && bpp != 32 {
        return None;
    }
    let src_bpp = (bpp / 8) as i32;
    let stride = ((w * src_bpp + 3) / 4) * 4;
    let need = off.checked_add((stride * h) as usize)?;
    if need > data.len() {
        return None;
    }
    let mut rgb = vec![0u8; (w * h * 3) as usize];
    for y in 0..h {
        let src_y = if top { y } else { h - 1 - y };
        let start = off + (src_y as usize) * (stride as usize);
        for x in 0..w {
            let s = start + (x as usize) * (src_bpp as usize);
            if s + 2 >= data.len() {
                return None;
            }
            let d = ((y * w + x) * 3) as usize;
            rgb[d] = data[s + 2];
            rgb[d + 1] = data[s + 1];
            rgb[d + 2] = data[s];
        }
    }
    Some(RgbImage { w, h, rgb })
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
                if e < 6 * n {
                    return best;
                }
            }
            x += step;
        }
        y += step;
    }
    best
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

/// Returns (x, y, w, h) of the match, or None.
pub fn find_template(
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
        if let Some((x, y, _)) = search_box(screen, tmpl, &samples, x0, y0, x1, y1, 1, limit) {
            if let Some(hit) = accept(screen, tmpl, tm, &samples, x, y, 1, limit, max_mean) {
                if !flat || ((hit.0 - px).abs() <= extra && (hit.1 - py).abs() <= extra) {
                    return Some(hit);
                }
            }
        }
        if flat {
            return None;
        }
    }

    let (x, y, _) = search_box(
        screen,
        tmpl,
        &samples,
        0,
        0,
        screen.w - tmpl.w,
        screen.h - tmpl.h,
        1,
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
}
