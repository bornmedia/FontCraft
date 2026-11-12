//! Font finder: finds the letters in a picture, reads them and compares their shapes
//! with fonts on this computer (and the letter sets of Google Fonts). Everything runs
//! locally; the picture never leaves the computer.
//!
//! How it works
//! 1. The picture (or the part the person marked) is turned into ink and background
//!    (Otsu threshold, the background is what touches the edges).
//! 2. Connected ink blobs become letters; blobs stacked on top of each other (i, j, ä…)
//!    are merged, letters are grouped into lines and words.
//! 3. Each letter is scaled into a small square picture. A font is compared by drawing
//!    the same letters and measuring how much ink overlaps (soft Jaccard), how wide each
//!    letter is and where it sits relative to the baseline.
//! 4. When the letters aren't known yet they are read first, by trying every letter of a
//!    handful of common fonts and keeping the closest. The best matching fonts then read
//!    them again, which fixes most mistakes.

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Side of the square a letter is compared in.
const N: usize = 32;
/// Size of the grid a whole word is compared in (for joined-up letters).
const WW: usize = 96;
const WH: usize = 24;
/// Size fonts are drawn at for comparing.
const RENDER_PX: f32 = 84.0;
/// At most this many letters are compared (more barely helps and costs time).
const MAX_GLYPHS: usize = 48;

/// The letters the finder can read by itself.
pub const CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789&?!";

// ------------------------------------------------------------------ the picture

#[derive(Clone, Debug)]
pub struct Glyph {
    /// x0, y0, x1, y1 as fractions of the whole picture
    pub rect: [f32; 4],
    pub line: usize,
    bmp: Vec<f32>,
    aspect: f32,
    /// height above / below the baseline in pixels (up is positive)
    top: f32,
    bot: f32,
    /// wider than a letter usually is: probably several joined letters
    pub wide: bool,
}

#[derive(Clone, Debug)]
pub struct Word {
    pub glyphs: std::ops::Range<usize>,
    pub line: usize,
    bmp: Vec<f32>,
    aspect: f32,
    top: f32,
    bot: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Sample {
    pub glyphs: Vec<Glyph>,
    pub words: Vec<Word>,
    /// every line as one piece (for joined-up script typed as a whole)
    pub line_words: Vec<Word>,
    pub lines: usize,
    /// dark letters on a light background
    #[allow(dead_code)]
    pub dark_ink: bool,
}

fn otsu(hist: &[u32; 256], total: u32) -> u8 {
    let sum: f64 = hist.iter().enumerate().map(|(i, h)| i as f64 * *h as f64).sum();
    let (mut sb, mut wb, mut best, mut t) = (0.0f64, 0.0f64, -1.0f64, 128u8);
    for i in 0..256 {
        wb += hist[i] as f64;
        if wb == 0.0 {
            continue;
        }
        let wf = total as f64 - wb;
        if wf == 0.0 {
            break;
        }
        sb += i as f64 * hist[i] as f64;
        let mb = sb / wb;
        let mf = (sum - sb) / wf;
        let between = wb * wf * (mb - mf) * (mb - mf);
        if between > best {
            best = between;
            t = i as u8;
        }
    }
    t
}

/// Scale `src` (w×h, values 0..1) into an ow×oh grid by averaging, then soften it a little.
/// With `keep_aspect` the shape is centred without stretching.
fn fit(src: &[f32], w: usize, h: usize, ow: usize, oh: usize, keep_aspect: bool) -> Vec<f32> {
    let mut out = vec![0.0f32; ow * oh];
    if w == 0 || h == 0 {
        return out;
    }
    // integral image
    let mut ii = vec![0.0f64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0.0;
        for x in 0..w {
            row += src[y * w + x] as f64;
            ii[(y + 1) * (w + 1) + x + 1] = ii[y * (w + 1) + x + 1] + row;
        }
    }
    let rect_sum = |x0: usize, y0: usize, x1: usize, y1: usize| {
        ii[y1 * (w + 1) + x1] - ii[y0 * (w + 1) + x1] - ii[y1 * (w + 1) + x0] + ii[y0 * (w + 1) + x0]
    };
    let (sx, sy, ox, oy) = if keep_aspect {
        let s = ((ow - 4) as f32 / w as f32).min((oh - 4) as f32 / h as f32);
        (s, s, (ow as f32 - w as f32 * s) / 2.0, (oh as f32 - h as f32 * s) / 2.0)
    } else {
        ((ow - 2) as f32 / w as f32, (oh - 2) as f32 / h as f32, 1.0, 1.0)
    };
    for ty in 0..oh {
        let fy0 = (ty as f32 - oy) / sy;
        let fy1 = (ty as f32 + 1.0 - oy) / sy;
        if fy1 <= 0.0 || fy0 >= h as f32 {
            continue;
        }
        for tx in 0..ow {
            let fx0 = (tx as f32 - ox) / sx;
            let fx1 = (tx as f32 + 1.0 - ox) / sx;
            if fx1 <= 0.0 || fx0 >= w as f32 {
                continue;
            }
            let x0 = (fx0.max(0.0).floor() as usize).min(w - 1);
            let y0 = (fy0.max(0.0).floor() as usize).min(h - 1);
            let x1 = (fx1.min(w as f32).ceil() as usize).clamp(x0 + 1, w);
            let y1 = (fy1.min(h as f32).ceil() as usize).clamp(y0 + 1, h);
            let area = ((x1 - x0) * (y1 - y0)) as f64;
            out[ty * ow + tx] = (rect_sum(x0, y0, x1, y1) / area) as f32;
        }
    }
    blur(&out, ow, oh)
}

fn blur(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (mut s, mut n) = (0.0, 0.0);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                    if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                        let k = if dx == 0 && dy == 0 { 2.0 } else { 1.0 };
                        s += src[yy as usize * w + xx as usize] * k;
                        n += k;
                    }
                }
            }
            out[y * w + x] = s / n;
        }
    }
    out
}

struct Comp {
    x0: usize,
    y0: usize,
    x1: usize, // exclusive
    y1: usize,
    area: usize,
    labels: Vec<u32>,
}

impl Comp {
    fn w(&self) -> usize {
        self.x1 - self.x0
    }
    fn h(&self) -> usize {
        self.y1 - self.y0
    }
    fn merge(&mut self, o: Comp) {
        self.x0 = self.x0.min(o.x0);
        self.y0 = self.y0.min(o.y0);
        self.x1 = self.x1.max(o.x1);
        self.y1 = self.y1.max(o.y1);
        self.area += o.area;
        self.labels.extend(o.labels);
    }
}

fn overlap(a0: usize, a1: usize, b0: usize, b1: usize) -> f32 {
    (a1.min(b1) as f32 - a0.max(b0) as f32).max(0.0)
}

/// Changes the person made to the letter boxes. Points are fractions of the whole
/// picture, so they stay put when another part of the picture is chosen.
/// They are applied in the order they were made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxEdits(pub Vec<BoxEdit>);

#[derive(Clone, Debug, PartialEq)]
pub enum BoxEdit {
    /// cut the box under this point into this many letters
    Split([f32; 2], usize),
    /// make the boxes under these two points one letter
    Join([f32; 2], [f32; 2]),
}

/// Ink of `c` in every column of its box.
fn column_ink(c: &Comp, label: &[u32], w: usize) -> Vec<u32> {
    (c.x0..c.x1)
        .map(|x| (c.y0..c.y1).filter(|y| { let l = label[y * w + x]; l != 0 && c.labels.contains(&l) }).count() as u32)
        .collect()
}

/// Typical stroke width of these blobs: the middle length of their horizontal ink runs.
fn stroke_width(comps: &[Comp], label: &[u32], w: usize) -> f32 {
    let mut runs = vec![];
    for c in comps {
        for y in c.y0..c.y1 {
            let mut run = 0u32;
            for x in c.x0..=c.x1 {
                let on = x < c.x1 && { let l = label[y * w + x]; l != 0 && c.labels.contains(&l) };
                if on {
                    run += 1;
                } else if run > 0 {
                    runs.push(run);
                    run = 0;
                }
            }
        }
    }
    if runs.is_empty() {
        return 1.0;
    }
    runs.sort();
    runs[runs.len() / 2] as f32
}

/// Cut a blob of touching letters into `k` pieces where the ink is thinnest.
/// Without `forced`, every cut must go through a joint no thicker than `max_joint`.
fn cut_comp(c: &Comp, k: usize, label: &[u32], w: usize, forced: bool, max_joint: f32) -> Option<Vec<Comp>> {
    if k < 2 {
        return None;
    }
    let n = c.w();
    let avg = n as f32 / k as f32;
    let minw = ((0.35 * avg) as usize).max(2);
    if n < k * minw {
        return None;
    }
    let p = column_ink(c, label, w);
    // best[j][x]: cheapest way to place j+1 cuts with the last one at column x
    let inf = f32::INFINITY;
    let cost = |x: usize| p[x] as f32 + 0.002 * (x as f32 - avg * (x as f32 / avg).round()).abs();
    let mut best = vec![vec![inf; n]; k - 1];
    let mut from = vec![vec![0usize; n]; k - 1];
    for x in minw..n {
        best[0][x] = cost(x);
    }
    for j in 1..k - 1 {
        for x in minw * (j + 1)..n {
            for prev in minw * j..=x - minw {
                let v = best[j - 1][prev] + cost(x);
                if v < best[j][x] {
                    best[j][x] = v;
                    from[j][x] = prev;
                }
            }
        }
    }
    let last = (minw * (k - 1)..=n - minw).min_by(|a, b| best[k - 2][*a].partial_cmp(&best[k - 2][*b]).unwrap())?;
    if !best[k - 2][last].is_finite() {
        return None;
    }
    let mut cuts = vec![last];
    for j in (1..k - 1).rev() {
        let prev = from[j][*cuts.last().unwrap()];
        cuts.push(prev);
    }
    cuts.reverse();
    if !forced {
        // A thin joint that only sits along the top is the arch of an m or n, not two
        // letters touching: those are left for the person to cut by hand.
        let arch = |x: usize| {
            let lowest = (c.y0..c.y1).rev().find(|y| { let l = label[y * w + c.x0 + x]; l != 0 && c.labels.contains(&l) });
            lowest.map(|y| ((y - c.y0) as f32) < 0.3 * c.h() as f32).unwrap_or(true)
        };
        if cuts.iter().any(|x| p[*x] as f32 > max_joint || arch(*x)) {
            return None;
        }
    }
    let mut edges = vec![0];
    edges.extend(cuts);
    edges.push(n);
    let mut out = vec![];
    for e in edges.windows(2) {
        let (a, b) = (c.x0 + e[0], c.x0 + e[1]);
        let mut piece = Comp { x0: a, y0: usize::MAX, x1: b, y1: 0, area: 0, labels: c.labels.clone() };
        for y in c.y0..c.y1 {
            for x in a..b {
                let l = label[y * w + x];
                if l != 0 && c.labels.contains(&l) {
                    piece.y0 = piece.y0.min(y);
                    piece.y1 = piece.y1.max(y + 1);
                    piece.area += 1;
                }
            }
        }
        if piece.area == 0 {
            return None;
        }
        out.push(piece);
    }
    Some(out)
}

/// Find the letters in `crop` (x0, y0, x1, y1 as fractions) of a grey picture.
#[allow(dead_code)]
pub fn analyze(img: &image::GrayImage, crop: [f32; 4]) -> Sample {
    analyze_with(img, crop, &BoxEdits::default())
}

/// Like [`analyze`], with the person's own splits and joins of letter boxes.
pub fn analyze_with(img: &image::GrayImage, crop: [f32; 4], edits: &BoxEdits) -> Sample {
    let (iw, ih) = img.dimensions();
    if iw < 2 || ih < 2 {
        return Sample::default();
    }
    let cx0 = ((crop[0].clamp(0.0, 1.0) * iw as f32) as u32).min(iw - 1);
    let cy0 = ((crop[1].clamp(0.0, 1.0) * ih as f32) as u32).min(ih - 1);
    let cx1 = ((crop[2].clamp(0.0, 1.0) * iw as f32).ceil() as u32).clamp(cx0 + 1, iw);
    let cy1 = ((crop[3].clamp(0.0, 1.0) * ih as f32).ceil() as u32).clamp(cy0 + 1, ih);
    let sub = image::imageops::crop_imm(img, cx0, cy0, cx1 - cx0, cy1 - cy0).to_image();
    // Work at a comfortable size: big enough for small text, small enough to be quick.
    let (sw, sh) = sub.dimensions();
    // levels are measured on the picture as it is (scaling softens thin strokes)
    let mut hist0 = [0u32; 256];
    for v in sub.as_raw() {
        hist0[*v as usize] += 1;
    }
    let longest = sw.max(sh) as f32;
    let k = if longest > 1800.0 { 1800.0 / longest } else if longest < 700.0 { (700.0 / longest).min(4.0) } else { 1.0 };
    let work = if (k - 1.0).abs() > 0.01 {
        let nw = ((sw as f32 * k).round() as u32).max(2);
        let nh = ((sh as f32 * k).round() as u32).max(2);
        image::imageops::resize(&sub, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        sub
    };
    let (w, h) = (work.width() as usize, work.height() as usize);
    let px = work.as_raw();
    let mut hist = [0u32; 256];
    for v in px {
        hist[*v as usize] += 1;
    }
    let t = otsu(&hist, px.len() as u32);
    // the background is what most of the edge is made of
    let mut bright_edge = 0usize;
    let mut edge = 0usize;
    for x in 0..w {
        for y in [0, h - 1] {
            edge += 1;
            bright_edge += (px[y * w + x] > t) as usize;
        }
    }
    for y in 0..h {
        for x in [0, w - 1] {
            edge += 1;
            bright_edge += (px[y * w + x] > t) as usize;
        }
    }
    let dark_ink = bright_edge * 2 >= edge;
    // Otsu leans towards the (much bigger) background, which makes thin letters fat and
    // joins neighbours. Cut halfway between real ink and real background instead.
    let t = {
        let level = |hist: &[u32; 256], lo: usize, hi: usize, q: f32| -> Option<f32> {
            let n: u32 = hist[lo..=hi].iter().sum();
            if n == 0 {
                return None;
            }
            let target = (n as f32 * q) as u32;
            let mut acc = 0;
            for i in lo..=hi {
                acc += hist[i];
                if acc > target {
                    return Some(i as f32);
                }
            }
            Some(hi as f32)
        };
        // the cut that separates letters well (measured on the scaled picture)…
        let (ink_s, bg_s) = if dark_ink {
            (level(&hist, 0, t as usize, 0.1), level(&hist, t as usize + 1, 255, 0.5))
        } else {
            (level(&hist, t as usize + 1, 255, 0.9), level(&hist, 0, t as usize, 0.5))
        };
        // …and the real ink colour, for stroke weight (the darkest few pixels as they are)
        let ink_r = if dark_ink { level(&hist0, 0, t as usize, 0.03) } else { level(&hist0, t as usize + 1, 255, 0.97) };
        let bg_r = if dark_ink { level(&hist0, t as usize + 1, 255, 0.5) } else { level(&hist0, 0, t as usize, 0.5) };
        let cut = match (ink_s, bg_s) {
            (Some(a), Some(b)) => ((a + b) / 2.0).round().clamp(0.0, 254.0) as u8,
            _ => t,
        };
        match (ink_r, bg_r) {
            (Some(a), Some(b)) => (cut, a, b),
            _ => (cut, if dark_ink { 0.0 } else { 255.0 }, t as f32),
        }
    };
    let (t, ink_level, bg_level) = t;
    // how much of a pixel is covered by ink (keeps the true stroke weight of small letters)
    let span = (bg_level - ink_level).abs().max(1.0);
    let cover = |v: u8| -> f32 { ((bg_level - v as f32).abs() / span).clamp(0.0, 1.0) };
    let ink: Vec<bool> = px.iter().map(|v| if dark_ink { *v <= t } else { *v > t }).collect();

    // connected blobs (8-neighbours)
    let mut label = vec![0u32; w * h];
    let mut comps: Vec<Comp> = vec![];
    let mut stack: Vec<usize> = vec![];
    for start in 0..w * h {
        if !ink[start] || label[start] != 0 {
            continue;
        }
        let id = comps.len() as u32 + 1;
        let mut c = Comp { x0: usize::MAX, y0: usize::MAX, x1: 0, y1: 0, area: 0, labels: vec![id] };
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            c.x0 = c.x0.min(x);
            c.y0 = c.y0.min(y);
            c.x1 = c.x1.max(x + 1);
            c.y1 = c.y1.max(y + 1);
            c.area += 1;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if ink[j] && label[j] == 0 {
                        label[j] = id;
                        stack.push(j);
                    }
                }
            }
        }
        comps.push(c);
    }
    // drop specks, frames and blobs that fill the picture
    comps.retain(|c| {
        let touches = c.x0 == 0 || c.y0 == 0 || c.x1 == w || c.y1 == h;
        let huge = c.w() as f32 > 0.92 * w as f32 || c.h() as f32 > 0.92 * h as f32;
        c.area >= 4 && !(touches && huge) && !(c.w() as f32 > 0.97 * w as f32 && c.h() as f32 > 0.6 * h as f32)
    });
    if comps.is_empty() {
        return Sample { dark_ink, ..Default::default() };
    }
    let mut hs: Vec<usize> = comps.iter().filter(|c| c.area >= 12).map(|c| c.h()).collect();
    if hs.is_empty() {
        hs = comps.iter().map(|c| c.h()).collect();
    }
    hs.sort();
    let hmed = hs[hs.len() / 2].max(1) as f32;
    let (mut big, small): (Vec<Comp>, Vec<Comp>) = comps.into_iter().partition(|c| c.h() as f32 >= 0.4 * hmed);
    // lines
    big.sort_by(|a, b| (a.y0 + a.y1).cmp(&(b.y0 + b.y1)));
    let mut lines: Vec<(usize, usize, Vec<Comp>)> = vec![];
    for c in big {
        let mut best: Option<(usize, f32)> = None;
        for (li, l) in lines.iter().enumerate() {
            let ov = overlap(c.y0, c.y1, l.0, l.1) / (c.h().min(l.1 - l.0).max(1) as f32);
            if ov >= 0.5 && best.map(|b| ov > b.1).unwrap_or(true) {
                best = Some((li, ov));
            }
        }
        match best {
            Some((li, _)) => {
                let l = &mut lines[li];
                l.0 = l.0.min(c.y0);
                l.1 = l.1.max(c.y1);
                l.2.push(c);
            }
            None => lines.push((c.y0, c.y1, vec![c])),
        }
    }
    for c in small {
        let cy = (c.y0 + c.y1) as f32 / 2.0;
        let mut best: Option<(usize, f32)> = None;
        for (li, l) in lines.iter().enumerate() {
            let lh = (l.1 - l.0) as f32;
            if cy >= l.0 as f32 - 0.6 * lh && cy <= l.1 as f32 + 0.4 * lh {
                let d = (cy - (l.0 + l.1) as f32 / 2.0).abs();
                if best.map(|b| d < b.1).unwrap_or(true) {
                    best = Some((li, d));
                }
            }
        }
        if let Some((li, _)) = best {
            lines[li].2.push(c);
        }
    }
    lines.sort_by_key(|l| l.0);

    let mut sample = Sample { dark_ink, ..Default::default() };
    let to_frac = |x: usize, y: usize| -> (f32, f32) {
        (
            (cx0 as f32 + x as f32 / k) / iw as f32,
            (cy0 as f32 + y as f32 / k) / ih as f32,
        )
    };
    let mask_of = |c: &Comp, x0: usize, y0: usize, x1: usize, y1: usize| -> Vec<f32> {
        let mut m = vec![0.0f32; (x1 - x0) * (y1 - y0)];
        for y in y0..y1 {
            for x in x0..x1 {
                let l = label[y * w + x];
                let mine = |l: u32| l != 0 && c.labels.contains(&l);
                // the letter's own pixels, plus the soft edge right next to it
                let take = mine(l)
                    || (l == 0
                        && (-1i32..=1).any(|dy| {
                            (-1i32..=1).any(|dx| {
                                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                                nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && mine(label[ny as usize * w + nx as usize])
                            })
                        }));
                if take {
                    m[(y - y0) * (x1 - x0) + x - x0] = cover(px[y * w + x]);
                }
            }
        }
        m
    };
    for (_, _, mut comps) in lines {
        // merge blobs stacked above each other (dots, accents, broken letters)
        comps.sort_by_key(|c| c.x0);
        loop {
            let mut merged = false;
            'outer: for i in 0..comps.len() {
                for j in 0..comps.len() {
                    if i == j {
                        continue;
                    }
                    let (a, b) = (&comps[i], &comps[j]);
                    let xo = overlap(a.x0, a.x1, b.x0, b.x1);
                    let yo = overlap(a.y0, a.y1, b.y0, b.y1);
                    let minw = a.w().min(b.w()).max(1) as f32;
                    let minh = a.h().min(b.h()).max(1) as f32;
                    if xo >= 0.5 * minw && yo <= 0.25 * minh {
                        let b = comps.remove(j);
                        let ai = if j < i { i - 1 } else { i };
                        comps[ai].merge(b);
                        merged = true;
                        break 'outer;
                    }
                }
            }
            if !merged {
                break;
            }
        }
        if comps.is_empty() {
            continue;
        }
        let mut lh: Vec<usize> = comps.iter().map(|c| c.h()).collect();
        lh.sort();
        let line_h = lh[lh.len() - 1].max(1) as f32;
        // drop punctuation and specks: small in both directions
        comps.retain(|c| (c.h() as f32) >= 0.3 * line_h || (c.w() as f32) >= 0.3 * line_h);
        if comps.is_empty() {
            continue;
        }
        // Letters that touch (ink bleed, tight spacing, blur in photos) come out as one
        // blob two or three letters wide: cut them where they meet in a thin joint.
        let mut ws: Vec<usize> = comps.iter().filter(|c| (c.w() as f32) <= line_h).map(|c| c.w()).collect();
        if ws.len() >= 3 {
            ws.sort();
            let tw = ws[ws.len() / 2].max(1) as f32;
            // usual width for a letter's height, so capitals (H, N) aren't judged by small letters
            let mut asp: Vec<f32> = comps.iter().filter(|c| (c.w() as f32) <= line_h).map(|c| c.w() as f32 / c.h().max(1) as f32).collect();
            asp.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let ta = asp[asp.len() / 2].max(0.05);
            let sw = stroke_width(&comps, &label, w);
            let mut out = Vec::with_capacity(comps.len());
            for c in comps {
                let r = (c.w() as f32 / tw).min(c.w() as f32 / (ta * c.h().max(1) as f32));
                if (1.7..=3.6).contains(&r) {
                    let max_joint = (0.5 * sw).min(0.15 * c.h() as f32).max(1.0);
                    let k = (r.round() as usize).clamp(2, 3);
                    if let Some(parts) = cut_comp(&c, k, &label, w, false, max_joint).or_else(|| if k > 2 { cut_comp(&c, 2, &label, w, false, max_joint) } else { None }) {
                        out.extend(parts);
                        continue;
                    }
                }
                out.push(c);
            }
            comps = out;
        }
        // the person's own cuts and joins
        let to_work = |p: [f32; 2]| -> (f32, f32) { ((p[0] * iw as f32 - cx0 as f32) * k, (p[1] * ih as f32 - cy0 as f32) * k) };
        let under = |comps: &[Comp], p: [f32; 2]| -> Option<usize> {
            let (x, y) = to_work(p);
            comps.iter().position(|c| x >= c.x0 as f32 - 1.0 && x <= c.x1 as f32 + 1.0 && y >= c.y0 as f32 - 1.0 && y <= c.y1 as f32 + 1.0)
        };
        for e in &edits.0 {
            match e {
                BoxEdit::Split(pt, n) => {
                    if let Some(i) = under(&comps, *pt) {
                        if let Some(parts) = cut_comp(&comps[i], *n, &label, w, true, 0.0) {
                            comps.splice(i..=i, parts);
                        }
                    }
                }
                BoxEdit::Join(a, b) => {
                    if let (Some(i), Some(j)) = (under(&comps, *a), under(&comps, *b)) {
                        if i != j {
                            let other = comps.remove(j);
                            let i = if j < i { i - 1 } else { i };
                            comps[i].merge(other);
                        }
                    }
                }
            }
        }
        comps.sort_by_key(|c| c.x0);
        let mut bottoms: Vec<usize> = comps.iter().map(|c| c.y1).collect();
        bottoms.sort();
        let baseline = bottoms[bottoms.len() / 2] as f32;
        let asc = comps.iter().map(|c| baseline - c.y0 as f32).fold(1.0f32, f32::max);
        let li = sample.lines;
        sample.lines += 1;
        // word breaks from the gaps
        let gaps: Vec<f32> = comps.windows(2).map(|p| p[1].x0 as f32 - p[0].x1 as f32).collect();
        let mut sg = gaps.clone();
        sg.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let gmed = sg.get(sg.len() / 2).copied().unwrap_or(0.0).max(0.0);
        let brk = (0.33 * asc).max(gmed * 2.2 + 1.0);
        let mut word_start = sample.glyphs.len();
        let line_start = word_start;
        for (ci, c) in comps.iter().enumerate() {
            if sample.glyphs.len() >= MAX_GLYPHS {
                break;
            }
            if ci > 0 && gaps[ci - 1] > brk {
                push_word(&mut sample, word_start, li, &comps, &mask_of, baseline, false);
                word_start = sample.glyphs.len();
            }
            let m = mask_of(c, c.x0, c.y0, c.x1, c.y1);
            let (fx0, fy0) = to_frac(c.x0, c.y0);
            let (fx1, fy1) = to_frac(c.x1, c.y1);
            sample.glyphs.push(Glyph {
                rect: [fx0, fy0, fx1, fy1],
                line: li,
                bmp: fit(&m, c.w(), c.h(), N, N, true),
                aspect: c.w() as f32 / c.h().max(1) as f32,
                top: baseline - c.y0 as f32,
                bot: baseline - c.y1 as f32,
                wide: c.w() as f32 > 1.45 * asc,
            });
        }
        if sample.glyphs.len() > word_start {
            push_word(&mut sample, word_start, li, &comps, &mask_of, baseline, false);
        }
        if sample.glyphs.len() > line_start {
            push_word(&mut sample, line_start, li, &comps, &mask_of, baseline, true);
        }
        if sample.glyphs.len() >= MAX_GLYPHS {
            break;
        }
    }
    sample
}

/// Close the word made of glyphs `start..` (the comps of this line, in the same order).
fn push_word(
    s: &mut Sample,
    start: usize,
    line: usize,
    comps: &[Comp],
    mask_of: &dyn Fn(&Comp, usize, usize, usize, usize) -> Vec<f32>,
    baseline: f32,
    whole_line: bool,
) {
    let end = s.glyphs.len();
    if end <= start {
        return;
    }
    // index of these glyphs inside the line's comps
    let first_in_line = s.glyphs.iter().filter(|g| g.line == line).count() - (end - start);
    let cs = &comps[first_in_line..first_in_line + (end - start)];
    let x0 = cs.iter().map(|c| c.x0).min().unwrap_or(0);
    let y0 = cs.iter().map(|c| c.y0).min().unwrap_or(0);
    let x1 = cs.iter().map(|c| c.x1).max().unwrap_or(1);
    let y1 = cs.iter().map(|c| c.y1).max().unwrap_or(1);
    let (ww, wh) = (x1 - x0, y1 - y0);
    let mut m = vec![0.0f32; ww * wh];
    for c in cs {
        let cm = mask_of(c, x0, y0, x1, y1);
        for (a, b) in m.iter_mut().zip(cm) {
            *a = a.max(b);
        }
    }
    let list = if whole_line { &mut s.line_words } else { &mut s.words };
    list.push(Word {
        glyphs: start..end,
        line,
        bmp: fit(&m, ww, wh, WW, WH, false),
        aspect: ww as f32 / wh.max(1) as f32,
        top: baseline - y0 as f32,
        bot: baseline - y1 as f32,
    });
}

// ------------------------------------------------------------------ drawing fonts

#[derive(Clone)]
struct Drawn {
    bmp: Vec<f32>,
    aspect: f32,
    top: f32,
    bot: f32,
}

fn draw_outlines(font: &FontRef, items: &[(ab_glyph::GlyphId, f32)], ow: usize, oh: usize, keep_aspect: bool) -> Option<Drawn> {
    let scale = PxScale::from(RENDER_PX);
    let outlines: Vec<_> = items
        .iter()
        .filter_map(|(id, x)| font.outline_glyph(id.with_scale_and_position(scale, point(*x, 0.0))))
        .collect();
    if outlines.is_empty() {
        return None;
    }
    let mut b = outlines[0].px_bounds();
    for o in &outlines[1..] {
        let r = o.px_bounds();
        b.min.x = b.min.x.min(r.min.x);
        b.min.y = b.min.y.min(r.min.y);
        b.max.x = b.max.x.max(r.max.x);
        b.max.y = b.max.y.max(r.max.y);
    }
    let w = (b.max.x - b.min.x).ceil().max(1.0) as usize;
    let h = (b.max.y - b.min.y).ceil().max(1.0) as usize;
    if w > 4000 || h > 4000 {
        return None;
    }
    let mut buf = vec![0.0f32; w * h];
    for o in &outlines {
        let r = o.px_bounds();
        let (ox, oy) = ((r.min.x - b.min.x) as i32, (r.min.y - b.min.y) as i32);
        o.draw(|x, y, c| {
            let (xx, yy) = (x as i32 + ox, y as i32 + oy);
            if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                let v = &mut buf[yy as usize * w + xx as usize];
                *v = (*v + c).min(1.0);
            }
        });
    }
    // trim empty edges (bounds can be generous)
    let ink_cols: Vec<usize> = (0..w).filter(|x| (0..h).any(|y| buf[y * w + x] >= 0.5)).collect();
    let ink_rows: Vec<usize> = (0..h).filter(|y| (0..w).any(|x| buf[y * w + x] >= 0.5)).collect();
    let (Some(&tx0), Some(&tx1), Some(&ty0), Some(&ty1)) = (ink_cols.first(), ink_cols.last(), ink_rows.first(), ink_rows.last()) else {
        return None;
    };
    let (tw, th) = (tx1 - tx0 + 1, ty1 - ty0 + 1);
    let mut t = vec![0.0f32; tw * th];
    for y in 0..th {
        for x in 0..tw {
            t[y * tw + x] = buf[(y + ty0) * w + x + tx0];
        }
    }
    Some(Drawn {
        bmp: fit(&t, tw, th, ow, oh, keep_aspect),
        aspect: tw as f32 / th as f32,
        top: -(b.min.y + ty0 as f32),
        bot: -(b.min.y + ty1 as f32 + 1.0),
    })
}

fn draw_char(font: &FontRef, c: char) -> Option<Drawn> {
    let id = font.glyph_id(c);
    if id.0 == 0 {
        return None;
    }
    draw_outlines(font, &[(id, 0.0)], N, N, true)
}

fn draw_word(font: &FontRef, text: &str) -> Option<Drawn> {
    let sf = font.as_scaled(PxScale::from(RENDER_PX));
    let mut x = 0.0;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    let mut items = vec![];
    for c in text.chars() {
        let id = font.glyph_id(c);
        if id.0 == 0 {
            return None;
        }
        if let Some(p) = prev {
            x += sf.kern(p, id);
        }
        items.push((id, x));
        x += sf.h_advance(id);
        prev = Some(id);
    }
    draw_outlines(font, &items, WW, WH, false)
}

/// Soft Jaccard: shared ink divided by all ink.
fn overlap_score(a: &[f32], b: &[f32]) -> f32 {
    let (mut mn, mut mx) = (0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        mn += x.min(*y);
        mx += x.max(*y);
    }
    if mx <= 0.0 {
        0.0
    } else {
        mn / mx
    }
}

/// How alike a piece of the picture and a drawn letter are (0..1).
/// `rel` = (top, bottom) of both relative to their line height.
fn likeness(bmp: &[f32], aspect: f32, d: &Drawn, img_rel: (f32, f32), font_rel: (f32, f32)) -> f32 {
    let shape = overlap_score(bmp, &d.bmp);
    let asp = (aspect.max(0.01) / d.aspect.max(0.01)).ln().abs();
    let pos = (img_rel.0 - font_rel.0).abs() + (img_rel.1 - font_rel.1).abs();
    0.7 * shape + 0.15 * (-3.0 * asp).exp() + 0.15 * (-4.0 * pos).exp()
}

// ------------------------------------------------------------------ what to compare

#[derive(Clone, Debug, PartialEq)]
enum Unit {
    Glyph(usize, char),
    Word(usize, String),
    Line(usize, String),
}

/// Decide what is compared: single letters, or whole words where letters are joined.
/// `letters` holds a letter for every glyph, `skip` the glyphs left out and `typed`
/// the text the person typed (if they did).
fn make_plan(s: &Sample, letters: &[char], skip: &[bool], typed: Option<&str>) -> Vec<Unit> {
    let mut units = vec![];
    let skipped = |g: usize| skip.get(g).copied().unwrap_or(false);
    // '?' marks a letter nobody knows: it is left out
    if let Some(t) = typed {
        let words: Vec<&str> = t.split_whitespace().collect();
        // the text in the box lists the words that aren't left out entirely
        let shown: Vec<usize> = (0..s.words.len()).filter(|w| s.words[*w].glyphs.clone().any(|g| !skipped(g))).collect();
        let target: Option<Vec<usize>> = if words.len() == shown.len() {
            Some(shown)
        } else if words.len() == s.words.len() {
            Some((0..s.words.len()).collect())
        } else {
            None
        };
        if let Some(target) = target {
            for (k, wi) in target.into_iter().enumerate() {
                let w = &s.words[wi];
                let chars: Vec<char> = words[k].chars().collect();
                if chars.len() == w.glyphs.len() {
                    for (j, g) in w.glyphs.clone().enumerate() {
                        if !skipped(g) && chars[j] != '?' {
                            units.push(Unit::Glyph(g, chars[j]));
                        }
                    }
                } else if !chars.is_empty() && !chars.contains(&'?') {
                    units.push(Unit::Word(wi, words[k].to_string()));
                }
            }
            return units;
        }
        let chars: Vec<char> = t.chars().filter(|c| !c.is_whitespace()).collect();
        let active: Vec<usize> = (0..s.glyphs.len()).filter(|g| !skipped(*g)).collect();
        if chars.len() == active.len() {
            for (g, c) in active.into_iter().zip(chars) {
                if c != '?' {
                    units.push(Unit::Glyph(g, c));
                }
            }
            return units;
        }
        // Letters and shapes don't line up (joined-up script): compare whole lines,
        // sharing the typed words out over the lines by their length.
        if t.contains('?') || s.line_words.is_empty() {
            return units;
        }
        let total: usize = s.line_words.iter().map(|l| l.glyphs.len()).sum::<usize>().max(1);
        let all_chars = chars.len().max(1);
        let mut wi = 0;
        let mut taken = 0usize;
        let mut seen_shapes = 0usize;
        for (li, l) in s.line_words.iter().enumerate() {
            seen_shapes += l.glyphs.len();
            let last = li + 1 == s.line_words.len();
            let mut text = vec![];
            while wi < words.len() && (last || taken * total < seen_shapes * all_chars || text.is_empty()) {
                taken += words[wi].chars().count();
                text.push(words[wi]);
                wi += 1;
            }
            if !text.is_empty() {
                units.push(Unit::Line(li, text.join(" ")));
            }
        }
        return units;
    }
    for (g, gl) in s.glyphs.iter().enumerate() {
        if !skipped(g) && !gl.wide {
            if let Some(c) = letters.get(g).filter(|c| **c != '?') {
                units.push(Unit::Glyph(g, *c));
            }
        }
    }
    units
}

/// Compare one font with the picture. `subset` limits the letters the font is known to
/// have (Google letter sets only contain CHARSET); others count against the font.
fn score_font(s: &Sample, units: &[Unit], font: &FontRef, subset: bool) -> Option<f32> {
    let mut cache: HashMap<char, Option<Drawn>> = HashMap::new();
    let mut drawn: Vec<Option<Drawn>> = Vec::with_capacity(units.len());
    let mut usable = vec![true; units.len()];
    for (ui, u) in units.iter().enumerate() {
        match u {
            Unit::Glyph(_, c) => {
                if subset && !CHARSET.contains(*c) {
                    usable[ui] = false;
                    drawn.push(None);
                    continue;
                }
                let d = cache.entry(*c).or_insert_with(|| draw_char(font, *c)).clone();
                drawn.push(d);
            }
            Unit::Word(_, t) | Unit::Line(_, t) => {
                if subset && t.chars().any(|c| !c.is_whitespace() && !CHARSET.contains(c)) {
                    usable[ui] = false;
                    drawn.push(None);
                    continue;
                }
                drawn.push(draw_word(font, t));
            }
        }
    }
    // line heights in the picture and in the font, over the same letters
    let mut l_img: HashMap<usize, f32> = HashMap::new();
    let mut l_font: HashMap<usize, f32> = HashMap::new();
    let mut missing = 0usize;
    let mut used = 0usize;
    for (ui, u) in units.iter().enumerate() {
        if !usable[ui] {
            continue;
        }
        used += 1;
        let Some(d) = &drawn[ui] else {
            missing += 1;
            continue;
        };
        let (_, _, top, _, line) = part(s, u);
        let e = l_img.entry(line).or_insert(1.0);
        *e = e.max(top);
        let f = l_font.entry(line).or_insert(1.0);
        *f = f.max(d.top);
    }
    if used == 0 || missing * 2 > used {
        return None;
    }
    let (mut sum, mut wsum) = (0.0f32, 0.0f32);
    for (ui, u) in units.iter().enumerate() {
        if !usable[ui] {
            continue;
        }
        let weight = match u {
            Unit::Glyph(..) => 1.0,
            Unit::Word(_, t) | Unit::Line(_, t) => t.chars().filter(|c| !c.is_whitespace()).count().min(12) as f32,
        };
        wsum += weight;
        let Some(d) = &drawn[ui] else { continue };
        let (bmp, aspect, top, bot, line) = part(s, u);
        let (li, lf) = (l_img[&line], l_font[&line]);
        let sim = likeness(bmp, aspect, d, (top / li, bot / li), (d.top / lf, d.bot / lf));
        sum += sim * weight;
    }
    Some(if wsum > 0.0 { sum / wsum } else { 0.0 })
}

/// The piece of the picture a unit stands for: (bitmap, aspect, top, bottom, line).
fn part<'a>(s: &'a Sample, u: &Unit) -> (&'a [f32], f32, f32, f32, usize) {
    match u {
        Unit::Glyph(g, _) => {
            let x = &s.glyphs[*g];
            (&x.bmp, x.aspect, x.top, x.bot, x.line)
        }
        Unit::Word(w, _) => {
            let x = &s.words[*w];
            (&x.bmp, x.aspect, x.top, x.bot, x.line)
        }
        Unit::Line(l, _) => {
            let x = &s.line_words[*l];
            (&x.bmp, x.aspect, x.top, x.bot, x.line)
        }
    }
}

/// Read the letters by trying every letter of `fonts` and keeping the closest one.
/// Glyphs in `skip` (and joined-up ones) keep '?'.
fn read_letters(s: &Sample, skip: &[bool], fonts: &[FontRef]) -> Vec<char> {
    let chars: Vec<char> = CHARSET.chars().collect();
    let active: Vec<usize> = (0..s.glyphs.len()).filter(|g| !skip.get(*g).copied().unwrap_or(false) && !s.glyphs[*g].wide).collect();
    let mut best = vec![vec![0.0f32; chars.len()]; s.glyphs.len()];
    let mut l_img: HashMap<usize, f32> = HashMap::new();
    for g in &active {
        let e = l_img.entry(s.glyphs[*g].line).or_insert(1.0);
        *e = e.max(s.glyphs[*g].top);
    }
    for font in fonts {
        let drawn: Vec<Option<Drawn>> = chars.iter().map(|c| draw_char(font, *c)).collect();
        // the font's line height: its tallest ordinary letters
        let lf = "HdlbkhAT"
            .chars()
            .filter_map(|c| chars.iter().position(|x| *x == c).and_then(|i| drawn[i].as_ref()).map(|d| d.top))
            .fold(0.0f32, f32::max);
        if lf <= 0.0 {
            continue;
        }
        for g in &active {
            let gl = &s.glyphs[*g];
            let li = l_img[&gl.line];
            for (ci, d) in drawn.iter().enumerate() {
                if let Some(d) = d {
                    let sim = likeness(&gl.bmp, gl.aspect, d, (gl.top / li, gl.bot / li), (d.top / lf, d.bot / lf));
                    if sim > best[*g][ci] {
                        best[*g][ci] = sim;
                    }
                }
            }
        }
    }
    let pick = |g: usize, keep: &dyn Fn(char) -> bool| -> Option<(char, f32)> {
        best[g].iter().enumerate().filter(|(i, _)| keep(chars[*i])).fold(None, |a: Option<(char, f32)>, (i, v)| match a {
            Some((_, bv)) if bv >= *v => a,
            _ => Some((chars[i], *v)),
        })
    };
    let mut out: Vec<char> =
        (0..s.glyphs.len()).map(|g| if active.contains(&g) { pick(g, &|_| true).map(|x| x.0).unwrap_or('?') } else { '?' }).collect();
    // Words are letters or numbers: a near tie between "B" and "8" goes the word's way.
    for w in &s.words {
        let gs: Vec<usize> = w.glyphs.clone().filter(|g| active.contains(g)).collect();
        let digits = gs.iter().filter(|g| out[**g].is_ascii_digit()).count();
        let letters = gs.iter().filter(|g| out[**g].is_alphabetic()).count();
        if digits == letters {
            continue;
        }
        let want_letters = letters > digits;
        for g in gs {
            let is_digit = out[g].is_ascii_digit();
            if is_digit == want_letters {
                let best_now = pick(g, &|_| true).map(|x| x.1).unwrap_or(0.0);
                let alt = if want_letters { pick(g, &|c| c.is_alphabetic()) } else { pick(g, &|c| c.is_ascii_digit()) };
                if let Some((c, v)) = alt {
                    if best_now - v < 0.04 {
                        out[g] = c;
                    }
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------ the search job

#[derive(Clone, Debug)]
pub struct Cand {
    pub path: PathBuf,
    pub index: u32,
    pub family: String,
    pub style: String,
    /// a Google Fonts letter set (not in the library)
    pub google: bool,
    /// id for drawing previews (the face id for library fonts)
    pub id: u64,
}

pub struct JobInput {
    pub sample: Arc<Sample>,
    /// letters per glyph; None = read them first
    pub letters: Option<Vec<char>>,
    pub skip: Vec<bool>,
    pub typed: Option<String>,
    pub local: Vec<Cand>,
    /// fonts (indices into `local`) used to read the letters
    pub readers: Vec<usize>,
    /// Google families whose letter sets are already on disk
    pub google_ready: Vec<Cand>,
    /// Google families to fetch first: (family name, request spec)
    pub google_missing: Vec<(String, String)>,
}

pub enum JobMsg {
    Letters(Vec<char>),
    Progress { done: usize, total: usize, what: String },
    Results(Vec<(Cand, f32)>),
    Done,
}

const KEEP: usize = 150;

fn top(all: &[(Cand, f32)]) -> Vec<(Cand, f32)> {
    let mut v = all.to_vec();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    v.truncate(KEEP);
    v
}

/// Compare every candidate, on all cores. Calls `report` with progress now and then.
fn score_all(
    s: &Sample,
    units: &[Unit],
    cands: &[Cand],
    cancel: &AtomicBool,
    report: &(dyn Fn(usize) + Sync),
) -> Vec<(usize, f32)> {
    if units.is_empty() || cands.is_empty() {
        return vec![];
    }
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 12);
    let results = std::sync::Mutex::new(Vec::with_capacity(cands.len()));
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                let mut last: Option<(PathBuf, Arc<Vec<u8>>)> = None;
                let mut local = vec![];
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= cands.len() {
                        break;
                    }
                    let c = &cands[i];
                    let data = match &last {
                        Some((p, d)) if *p == c.path => Some(d.clone()),
                        _ => std::fs::read(&c.path).ok().map(Arc::new),
                    };
                    if let Some(d) = data {
                        last = Some((c.path.clone(), d.clone()));
                        if let Ok(f) = FontRef::try_from_slice_and_index(&d, c.index) {
                            // a broken font must not take the whole search down
                            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| score_font(s, units, &f, c.google)));
                            if let Ok(Some(v)) = r {
                                local.push((i, v));
                            }
                        }
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 64 == 0 {
                        report(n);
                    }
                }
                results.lock().unwrap().extend(local);
            });
        }
    });
    results.into_inner().unwrap()
}

fn load_fonts(cands: &[&Cand]) -> Vec<(Vec<u8>, u32)> {
    cands.iter().filter_map(|c| std::fs::read(&c.path).ok().map(|d| (d, c.index))).collect()
}

fn read_with(s: &Sample, skip: &[bool], data: &[(Vec<u8>, u32)]) -> Vec<char> {
    let fonts: Vec<FontRef> = data.iter().filter_map(|(d, i)| FontRef::try_from_slice_and_index(d, *i).ok()).collect();
    read_letters(s, skip, &fonts)
}

pub fn spawn(input: JobInput, cancel: Arc<AtomicBool>, ctx: egui::Context) -> Receiver<JobMsg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        run(input, &cancel, &tx, &ctx);
        let _ = tx.send(JobMsg::Done);
        ctx.request_repaint();
    });
    rx
}

fn run(input: JobInput, cancel: &AtomicBool, tx: &Sender<JobMsg>, ctx: &egui::Context) {
    let s = &*input.sample;
    let send = |m: JobMsg| {
        let _ = tx.send(m);
        ctx.request_repaint();
    };
    let auto = input.letters.is_none() && input.typed.is_none();
    let mut letters = match input.letters.clone() {
        Some(l) => l,
        None if input.typed.is_none() => {
            send(JobMsg::Progress { done: 0, total: 0, what: "Reading the letters…".into() });
            let readers: Vec<&Cand> = input.readers.iter().filter_map(|i| input.local.get(*i)).collect();
            let l = read_with(s, &input.skip, &load_fonts(&readers));
            send(JobMsg::Letters(l.clone()));
            l
        }
        None => vec![],
    };
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let mut units = make_plan(s, &letters, &input.skip, input.typed.as_deref());
    if units.is_empty() {
        send(JobMsg::Results(vec![]));
        return;
    }
    // 1. the library and the fonts on this computer
    let total_all = input.local.len() + input.google_ready.len() + input.google_missing.len();
    let what = "Comparing with your fonts…".to_string();
    send(JobMsg::Progress { done: 0, total: total_all, what: what.clone() });
    let report = |n: usize| {
        let _ = tx.send(JobMsg::Progress { done: n, total: total_all, what: what.clone() });
        ctx.request_repaint();
    };
    let mut all: Vec<(Cand, f32)> = score_all(s, &units, &input.local, cancel, &report)
        .into_iter()
        .map(|(i, v)| (input.local[i].clone(), v))
        .collect();
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    send(JobMsg::Results(top(&all)));
    // 2. read the letters again with the closest fonts, which fixes most misreadings
    if auto && !all.is_empty() {
        let best = top(&all);
        let readers: Vec<&Cand> = best.iter().take(6).map(|(c, _)| c).collect();
        let l2 = read_with(s, &input.skip, &load_fonts(&readers));
        if l2 != letters {
            letters = l2;
            send(JobMsg::Letters(letters.clone()));
            units = make_plan(s, &letters, &input.skip, None);
            let mut order = all.clone();
            order.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            order.truncate(600);
            let again: Vec<Cand> = order.into_iter().map(|(c, _)| c).collect();
            let what2 = "Checking the closest fonts again…".to_string();
            let total2 = again.len();
            let report2 = |n: usize| {
                let _ = tx.send(JobMsg::Progress { done: n, total: total2, what: what2.clone() });
                ctx.request_repaint();
            };
            all = score_all(s, &units, &again, cancel, &report2).into_iter().map(|(i, v)| (again[i].clone(), v)).collect();
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            send(JobMsg::Results(top(&all)));
        }
    }
    // 3. Google Fonts letter sets: those on disk, then the missing ones in batches
    let mut done_base = input.local.len();
    if !input.google_ready.is_empty() {
        let what = "Comparing with Google Fonts…".to_string();
        let report = |n: usize| {
            let _ = tx.send(JobMsg::Progress { done: done_base + n, total: total_all, what: what.clone() });
            ctx.request_repaint();
        };
        let r = score_all(s, &units, &input.google_ready, cancel, &report);
        all.extend(r.into_iter().map(|(i, v)| (input.google_ready[i].clone(), v)));
        done_base += input.google_ready.len();
        send(JobMsg::Results(top(&all)));
    }
    // four requests at a time; each asks for 48 families
    for group in input.google_missing.chunks(48 * 4) {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        send(JobMsg::Progress {
            done: done_base,
            total: total_all,
            what: "Getting letter shapes from Google Fonts (only the first time)…".into(),
        });
        let got: Vec<(String, PathBuf)> = std::thread::scope(|sc| {
            let hs: Vec<_> = group.chunks(48).map(|b| sc.spawn(move || crate::google::fetch_letter_sets(b))).collect();
            hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });
        let batch = group;
        let cands: Vec<Cand> = got.into_iter().map(|(name, path)| crate::google::letter_set_cand(&name, path)).collect();
        let r = score_all(s, &units, &cands, cancel, &|_| {});
        all.extend(r.into_iter().map(|(i, v)| (cands[i].clone(), v)));
        done_base += batch.len();
        send(JobMsg::Results(top(&all)));
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    fn system_font(names: &[&str]) -> Option<PathBuf> {
        let dirs = crate::library::system_font_dirs();
        for d in dirs {
            for n in names {
                let p = d.join(n);
                if p.exists() {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Draw `text` with a font into a grey picture (black on white).
    fn picture(path: &PathBuf, text: &str, px: f32) -> image::GrayImage {
        let data = std::fs::read(path).unwrap();
        let font = FontRef::try_from_slice(&data).unwrap();
        let sf = font.as_scaled(PxScale::from(px));
        let w = (text.chars().count() as f32 * px * 0.8) as u32 + 80;
        let h = (px * 1.8) as u32;
        let mut img = image::GrayImage::from_pixel(w, h, image::Luma([255]));
        let mut x = 30.0;
        let base = px * 1.2;
        for c in text.chars() {
            let id = font.glyph_id(c);
            if let Some(o) = font.outline_glyph(id.with_scale_and_position(PxScale::from(px), point(x, base))) {
                let b = o.px_bounds();
                o.draw(|gx, gy, cov| {
                    let (xx, yy) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                    if xx >= 0 && yy >= 0 && (xx as u32) < w && (yy as u32) < h {
                        let v = (255.0 * (1.0 - cov)) as u8;
                        let p = img.get_pixel_mut(xx as u32, yy as u32);
                        p.0[0] = p.0[0].min(v);
                    }
                });
            }
            x += sf.h_advance(id);
        }
        img
    }

    #[test]
    fn finds_letters_and_the_right_font() {
        let Some(arial) = system_font(&["arial.ttf", "Arial.ttf", "DejaVuSans.ttf", "LiberationSans-Regular.ttf"]) else { return };
        let Some(times) = system_font(&["times.ttf", "Times New Roman.ttf", "DejaVuSerif.ttf", "LiberationSerif-Regular.ttf"]) else { return };
        let Some(georgia) = system_font(&["georgia.ttf", "Georgia.ttf", "verdana.ttf", "DejaVuSansMono.ttf"]) else { return };
        let img = picture(&times, "Hamburg Quiz", 64.0);
        let s = analyze(&img, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(s.glyphs.len(), 11, "letters found: {}", s.glyphs.len());
        assert_eq!(s.words.len(), 2);
        assert!(s.dark_ink);
        let d: Vec<(Vec<u8>, u32)> = [&arial, &times, &georgia].iter().map(|p| (std::fs::read(p).unwrap(), 0)).collect();
        let read = read_with(&s, &[], &d);
        let txt: String = read.iter().collect();
        assert_eq!(txt, "HamburgQuiz");
        let cands: Vec<Cand> = [&arial, &times, &georgia]
            .iter()
            .map(|p| Cand { path: (*p).clone(), index: 0, family: p.display().to_string(), style: String::new(), google: false, id: 0 })
            .collect();
        let units = make_plan(&s, &read, &[], None);
        let mut r = score_all(&s, &units, &cands, &AtomicBool::new(false), &|_| {});
        r.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        assert_eq!(cands[r[0].0].path, times, "{r:?}");
        assert!(r[0].1 > 0.85, "{r:?}");
        // light text on a dark background, typed text with joined-up word handling
        let mut inv = img.clone();
        for p in inv.pixels_mut() {
            p.0[0] = 255 - p.0[0];
        }
        let s2 = analyze(&inv, [0.0, 0.0, 1.0, 1.0]);
        assert!(!s2.dark_ink);
        let units = make_plan(&s2, &[], &[], Some("Hamburg Quiz"));
        assert_eq!(units.len(), 11);
        let units = make_plan(&s2, &[], &[], Some("Hamburg Quizz"));
        assert!(units.iter().any(|u| matches!(u, Unit::Word(_, t) if t == "Quizz")));
        // more letters than shapes in a different word count: the whole line is compared
        let units = make_plan(&s2, &[], &[], Some("Hamburger Quiz Show"));
        assert_eq!(units, vec![Unit::Line(0, "Hamburger Quiz Show".into())]);
    }

    /// Join neighbouring letters in `img` with a thin bar at half their height.
    fn glue(img: &mut image::GrayImage, a: &Glyph, b: &Glyph, thick: u32) {
        let (w, h) = (img.width() as f32, img.height() as f32);
        // across most of both letters, so both are surely touched
        let x0 = ((0.8 * a.rect[0] + 0.2 * a.rect[2]) * w) as u32;
        let x1 = ((0.2 * b.rect[0] + 0.8 * b.rect[2]) * w) as u32;
        let y = ((a.rect[1] + a.rect[3]).max(b.rect[1] + b.rect[3]) / 2.0 * h) as u32;
        for x in x0..=x1 {
            for t in 0..thick {
                img.put_pixel(x, y + t, image::Luma([0]));
            }
        }
    }

    #[test]
    fn cuts_touching_letters_apart() {
        let Some(times) = system_font(&["times.ttf", "Times New Roman.ttf", "DejaVuSerif.ttf", "LiberationSerif-Regular.ttf"]) else { return };
        let Some(arial) = system_font(&["arial.ttf", "Arial.ttf", "DejaVuSans.ttf", "LiberationSans-Regular.ttf"]) else { return };
        // (font, text, neighbours glued together that must be cut apart again)
        let cases: [(&PathBuf, &str, &[usize]); 4] =
            [(&times, "Hamburg Quiz", &[0, 4]), (&arial, "Hamburg Quiz", &[0, 4, 7]), (&times, "summer swimming", &[0, 4]), (&arial, "minimum MW", &[])];
        for (font, text, pairs) in cases {
            let clean = picture(font, text, 64.0);
            let s0 = analyze(&clean, [0.0, 0.0, 1.0, 1.0]);
            let n = text.chars().filter(|c| !c.is_whitespace()).count();
            assert_eq!(s0.glyphs.len(), n, "{text}: wide letters like m and W must stay whole");
            let mut img = clean.clone();
            for g in pairs {
                glue(&mut img, &s0.glyphs[*g], &s0.glyphs[*g + 1], 2);
            }
            let glued = analyze_with(&img, [0.0, 0.0, 1.0, 1.0], &BoxEdits::default());
            assert_eq!(glued.glyphs.len(), n, "{text}: touching letters are cut apart");
            // a whole word glued into one blob: too long to cut by itself, but the person can
            let mut img = clean.clone();
            let w0 = s0.words[0].glyphs.clone();
            for g in w0.start..w0.end - 1 {
                glue(&mut img, &s0.glyphs[g], &s0.glyphs[g + 1], 2);
            }
            let s1 = analyze(&img, [0.0, 0.0, 1.0, 1.0]);
            let first = &s1.glyphs[0];
            assert!(first.wide, "{text}: a glued word is one wide box");
            let r = first.rect;
            let mid = |r: [f32; 4]| [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
            let edits = BoxEdits(vec![BoxEdit::Split(mid(r), w0.len())]);
            let s2 = analyze_with(&img, [0.0, 0.0, 1.0, 1.0], &edits);
            assert_eq!(s2.glyphs.len(), n, "{text}: split by hand");
            // and joined back
            let mut edits = edits.clone();
            edits.0.push(BoxEdit::Join(mid(s2.glyphs[0].rect), mid(s2.glyphs[1].rect)));
            let s3 = analyze_with(&img, [0.0, 0.0, 1.0, 1.0], &edits);
            assert_eq!(s3.glyphs.len(), n - 1, "{text}: joined by hand");
            // a joined box can be cut again
            edits.0.push(BoxEdit::Split(mid(s3.glyphs[0].rect), 2));
            let s4 = analyze_with(&img, [0.0, 0.0, 1.0, 1.0], &edits);
            assert_eq!(s4.glyphs.len(), n, "{text}: cut again after joining");
        }
    }

    /// The whole search over every font on this computer, without hints.
    /// Run with: cargo test --release whole_search -- --ignored --nocapture
    #[test]
    #[ignore]
    fn whole_search() {
        let mut local = vec![];
        for d in crate::library::system_font_dirs() {
            for e in walkdir::WalkDir::new(d).into_iter().flatten() {
                if crate::library::is_font_file(e.path()) {
                    for m in crate::library::parse_file(e.path()) {
                        local.push(Cand { path: m.path.clone(), index: m.index, family: m.family.clone(), style: m.style.clone(), google: false, id: 0 });
                    }
                }
            }
        }
        let readers: Vec<usize> = ["Arial", "Times New Roman", "Georgia", "Verdana", "Calibri", "Segoe UI", "Courier New", "Garamond", "Century Gothic", "Rockwell"]
            .iter()
            .filter_map(|n| local.iter().position(|c| c.family == *n && (c.style == "Regular" || c.style == "Normal")))
            .collect();
        println!("{} faces, {} readers", local.len(), readers.len());
        let cases = std::env::var("FC_CASES").unwrap_or_else(|_| "georgia.ttf|Grumpy wizards|56;bahnschrift.ttf|Quick Jump 2024|48;COOPBL.TTF|Fresh Bakery|60;consola.ttf|let mut x = 42|40;BKANT.TTF|Gentle Harvest|52".into());
        for case in cases.split(';') {
            let parts: Vec<&str> = case.split('|').collect();
            let given = PathBuf::from(parts[0]);
            let Some(path) = (if given.is_absolute() { Some(given) } else { system_font(&[parts[0]]) }) else {
                println!("skip {}", parts[0]);
                continue;
            };
            let img = picture(&path, parts[1], parts[2].parse().unwrap());
            let mut local = local.clone();
            if !local.iter().any(|c| c.path == path) {
                local.push(Cand { path: path.clone(), index: 0, family: "(the font itself)".into(), style: String::new(), google: true, id: 0 });
            }
            let s = Arc::new(analyze(&img, [0.0, 0.0, 1.0, 1.0]));
            let t = std::time::Instant::now();
            let input = JobInput {
                sample: s.clone(),
                letters: None,
                skip: s.glyphs.iter().map(|g| g.wide).collect(),
                typed: std::env::var("FC_TYPED").ok().map(|_| parts[1].to_string()),
                local: local.clone(),
                readers: readers.clone(),
                google_ready: vec![],
                google_missing: vec![],
            };
            let (tx, rx) = channel();
            run(input, &AtomicBool::new(false), &tx, &egui::Context::default());
            drop(tx);
            let mut res = vec![];
            let mut letters = String::new();
            for m in rx {
                match m {
                    JobMsg::Results(r) => res = r,
                    JobMsg::Letters(l) => letters = l.iter().collect(),
                    _ => {}
                }
            }
            let rank = res.iter().position(|(c, _)| c.path == path).map(|r| r + 1);
            println!("\n{} \"{}\": read \"{}\" in {:?}, true font at rank {:?}", parts[0], parts[1], letters, t.elapsed(), rank);
            for (c, v) in res.iter().take(5) {
                println!("   {:.3}  {} {}", v, c.family, c.style);
            }
        }
    }
}