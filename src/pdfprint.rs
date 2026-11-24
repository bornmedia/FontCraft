//! Printing a collection (or a tag, or a smart filter) as a PDF: one entry per font family with
//! its name and the preview text set in the font, at the preview size, in the preview colours
//! and alignment the person is using.
//!
//! The previews are drawn by FontCraft itself (300 dpi) and placed in the PDF as pictures, so
//! the PDF looks the same everywhere, needs no fonts installed and never contains font files.

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Item {
    pub name: String,
    /// small text after the name, e.g. "OTF · 4 styles"
    pub info: String,
    pub path: PathBuf,
    pub index: u32,
    /// the right-to-left script the font is made for ("Arabic"…), or empty
    pub rtl: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

pub struct Spec {
    pub title: String,
    pub items: Vec<Item>,
    pub text: String,
    /// the preview size in screen pixels
    pub size_px: f32,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub align: HAlign,
    /// the interface font, for names and page numbers
    pub label_font: Option<PathBuf>,
}

const PAGE_W: f32 = 595.28; // A4 in points
const PAGE_H: f32 = 841.89;
const MARGIN: f32 = 42.0;
const DPI: f32 = 300.0;
const PX_PER_PT: f32 = DPI / 72.0;

/// A picture placed on a page (position and size in points, from the top left).
struct Placed {
    img: Rgb,
    x: f32,
    y: f32,
}

struct Rgb {
    w: usize,
    h: usize,
    data: Vec<u8>,
}

impl Rgb {
    fn w_pt(&self) -> f32 {
        self.w as f32 / PX_PER_PT
    }
    fn h_pt(&self) -> f32 {
        self.h as f32 / PX_PER_PT
    }
}

fn mix(bg: [u8; 3], fg: [u8; 3], a: f32) -> [u8; 3] {
    let m = |b: u8, f: u8| (b as f32 + (f as f32 - b as f32) * a).round().clamp(0.0, 255.0) as u8;
    [m(bg[0], fg[0]), m(bg[1], fg[1]), m(bg[2], fg[2])]
}

/// The scale at which one em of the font is `px` pixels: type sizes (12 pt…) measure the em, not
/// the height from the highest to the lowest point, so printed sizes are the real sizes.
fn em_scale(font: &ab_glyph::FontArc, px: f32) -> ab_glyph::PxScale {
    use ab_glyph::Font;
    match font.units_per_em() {
        Some(upem) if upem > 0.0 => ab_glyph::PxScale::from(px * font.height_unscaled() / upem),
        _ => ab_glyph::PxScale::from(px),
    }
}

/// Lines of `text` set in `font` at `px` pixels (to the em), wrapped to `max_w` pixels.
fn wrap(font: &ab_glyph::FontArc, px: f32, text: &str, max_w: f32) -> Vec<String> {
    use ab_glyph::{Font, ScaleFont};
    let sf = font.as_scaled(em_scale(font, px));
    let width = |s: &str| {
        let mut w = 0.0;
        let mut prev = None;
        for c in s.chars() {
            let id = sf.glyph_id(c);
            if let Some(p) = prev {
                w += sf.kern(p, id);
            }
            w += sf.h_advance(id);
            prev = Some(id);
        }
        w
    };
    let mut lines = vec![];
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if width(&candidate) <= max_w || line.is_empty() {
                // a single word wider than the line is cut into pieces
                if line.is_empty() && width(word) > max_w {
                    let mut piece = String::new();
                    for c in word.chars() {
                        let next = format!("{piece}{c}");
                        if width(&next) > max_w && !piece.is_empty() {
                            lines.push(std::mem::take(&mut piece));
                        }
                        piece.push(c);
                    }
                    line = piece;
                } else {
                    line = candidate;
                }
            } else {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            }
        }
        lines.push(line);
    }
    lines
}

/// One line of text drawn at `px` pixels on the background colour.
fn draw_line(font: &ab_glyph::FontArc, px: f32, text: &str, fg: [u8; 3], bg: [u8; 3]) -> Option<Rgb> {
    use ab_glyph::{point, Font, ScaleFont};
    let scale = em_scale(font, px);
    let sf = font.as_scaled(scale);
    let ascent = sf.ascent();
    let height = (sf.ascent() - sf.descent() + sf.line_gap().max(0.0) * 0.5).ceil().max(1.0);
    let mut x = 0.0f32;
    let mut glyphs = vec![];
    let mut prev = None;
    for c in text.chars() {
        let id = sf.glyph_id(c);
        if let Some(p) = prev {
            x += sf.kern(p, id);
        }
        glyphs.push(id.with_scale_and_position(scale, point(x, ascent)));
        x += sf.h_advance(id);
        prev = Some(id);
    }
    let w = (x.ceil() as usize + 2).clamp(1, 12000);
    let h = (height as usize + 2).clamp(1, 6000);
    let mut cov = vec![0f32; w * h];
    for g in glyphs {
        if let Some(og) = font.outline_glyph(g) {
            let b = og.px_bounds();
            og.draw(|gx, gy, a| {
                let px_ = b.min.x as i32 + gx as i32;
                let py_ = b.min.y as i32 + gy as i32;
                if px_ >= 0 && py_ >= 0 && (px_ as usize) < w && (py_ as usize) < h {
                    let i = py_ as usize * w + px_ as usize;
                    cov[i] = (cov[i] + a).min(1.0);
                }
            });
        }
    }
    let mut data = Vec::with_capacity(w * h * 3);
    for a in cov {
        data.extend_from_slice(&mix(bg, fg, a));
    }
    Some(Rgb { w, h, data })
}

fn load(path: &Path, index: u32) -> Option<ab_glyph::FontArc> {
    let data = std::fs::read(path).ok()?;
    ab_glyph::FontVec::try_from_vec_and_index(data, index).ok().map(ab_glyph::FontArc::new)
}

/// Make the PDF. Returns how many pages it has.
pub fn make_pdf(spec: &Spec, dest: &Path) -> Result<usize, String> {
    let pages = layout(spec);
    write_pdf(&pages, &spec.title, spec.bg, dest)?;
    Ok(pages.len())
}

/// Everything drawn and placed on pages.
fn layout(spec: &Spec) -> Vec<Vec<Placed>> {
    let content_w = PAGE_W - 2.0 * MARGIN;
    let label = spec.label_font.as_deref().and_then(|p| load(p, 0));
    let weak = mix(spec.bg, spec.fg, 0.55);
    let label_px = 8.5 * PX_PER_PT;
    // the preview size: screen pixels (96 per inch) to points, then to print pixels
    let pt = spec.size_px * 0.75;
    let px = pt * PX_PER_PT;
    let mut pages: Vec<Vec<Placed>> = vec![vec![]];
    let mut y = MARGIN;
    let bottom = PAGE_H - MARGIN - 18.0;
    let place = |pages: &mut Vec<Vec<Placed>>, y: &mut f32, img: Rgb, align: HAlign, gap_after: f32, keep: f32| {
        let h = img.h_pt();
        if *y + h + keep > bottom && !pages.last().map(|p| p.is_empty()).unwrap_or(true) {
            pages.push(vec![]);
            *y = MARGIN;
        }
        let x = match align {
            HAlign::Left => MARGIN,
            HAlign::Center => MARGIN + (content_w - img.w_pt()).max(0.0) / 2.0,
            HAlign::Right => MARGIN + (content_w - img.w_pt()).max(0.0),
        };
        let yy = *y;
        pages.last_mut().unwrap().push(Placed { img, x, y: yy });
        *y += h + gap_after;
    };
    // title
    if let Some(l) = &label {
        if let Some(img) = draw_line(l, 16.0 * PX_PER_PT, &spec.title, spec.fg, spec.bg) {
            place(&mut pages, &mut y, img, HAlign::Left, 2.0, 0.0);
        }
        let sub = format!("{} font{} · FontCraft", spec.items.len(), if spec.items.len() == 1 { "" } else { "s" });
        if let Some(img) = draw_line(l, 8.5 * PX_PER_PT, &sub, weak, spec.bg) {
            place(&mut pages, &mut y, img, HAlign::Left, 18.0, 0.0);
        }
    }
    for item in &spec.items {
        // the name, then the preview lines
        let mut block: Vec<(Rgb, HAlign, f32)> = vec![];
        if let Some(l) = &label {
            let head = if item.info.is_empty() { item.name.clone() } else { format!("{}     {}", item.name, item.info) };
            if let Some(img) = draw_line(l, label_px, &head, weak, spec.bg) {
                block.push((img, HAlign::Left, 3.0));
            }
        }
        // a right-to-left font gets a sample in its own script (unless the text is right to left already)
        let text = if !item.rtl.is_empty() && !spec.text.chars().any(crate::shape::is_rtl_char) {
            crate::shape::rtl_sample(&item.rtl).unwrap_or(&spec.text).to_string()
        } else {
            spec.text.clone()
        };
        match load(&item.path, item.index) {
            // right-to-left and joined scripts are shaped; their lines start on the right
            Some(_) if crate::shape::needs_shaping(&text) => {
                let data = std::fs::read(&item.path).unwrap_or_default();
                let align = if crate::shape::base_rtl(&text) {
                    match spec.align {
                        HAlign::Left => HAlign::Right,
                        HAlign::Right => HAlign::Left,
                        c => c,
                    }
                } else {
                    spec.align
                };
                for line in crate::shape::wrap_lines(&data, item.index, &text, px, content_w * PX_PER_PT) {
                    if let Some(pic) = crate::shape::render(&data, item.index, &line, px, None, crate::shape::HAlign::Left, 1) {
                        let mut rgb = Vec::with_capacity(pic.w * pic.h * 3);
                        for a in &pic.alpha {
                            rgb.extend_from_slice(&mix(spec.bg, spec.fg, *a as f32 / 255.0));
                        }
                        block.push((Rgb { w: pic.w, h: pic.h, data: rgb }, align, 0.0));
                    }
                }
            }
            Some(font) => {
                for line in wrap(&font, px, &text, content_w * PX_PER_PT) {
                    if let Some(img) = draw_line(&font, px, &line, spec.fg, spec.bg) {
                        block.push((img, spec.align, 0.0));
                    }
                }
            }
            None => {
                if let Some(img) = label.as_ref().and_then(|l| draw_line(l, label_px, "(this font could not be read)", weak, spec.bg)) {
                    block.push((img, HAlign::Left, 0.0));
                }
            }
        }
        // a font's block stays on one page when it fits on one; a very long one continues on the next
        let total: f32 = block.iter().map(|(i, _, g)| i.h_pt() + g).sum();
        if y + total > bottom && total <= bottom - MARGIN && !pages.last().map(|p| p.is_empty()).unwrap_or(true) {
            pages.push(vec![]);
            y = MARGIN;
        }
        for (img, align, gap) in block {
            place(&mut pages, &mut y, img, align, gap, 0.0);
        }
        y += 16.0;
    }
    // page numbers
    let n = pages.len();
    if let Some(l) = &label {
        for (i, page) in pages.iter_mut().enumerate() {
            let t = format!("{}  ·  {} / {}", spec.title, i + 1, n);
            if let Some(img) = draw_line(l, 7.5 * PX_PER_PT, &t, weak, spec.bg) {
                let x = PAGE_W - MARGIN - img.w_pt();
                page.push(Placed { img, x, y: PAGE_H - MARGIN + 4.0 });
            }
        }
    }
    pages
}

/// A one-page specimen of one font: its name large, a waterfall of sizes, the sample text, the
/// character set and running text in two columns. Black on white; every size exactly in points
/// (one em = the size). Whatever doesn't fit on the page is left out (and named in the result).
pub struct Sheet {
    /// "Tahoma" or "Tahoma Bold": the big name, the waterfall lines and the footer
    pub name: String,
    /// small line under the name: "Bold · 4 styles · OTF · Designer · Foundry · Version 2.0"
    pub info: String,
    pub path: PathBuf,
    pub index: u32,
    /// the right-to-left script the font is made for ("Arabic"…), or empty
    pub rtl: String,
    /// sizes of the waterfall in points; empty: no waterfall
    pub sizes: Vec<f32>,
    /// the waterfall's lines: empty = the font's name and the size ("Tahoma 12pt"), else this text
    pub waterfall_text: String,
    /// the sample text and its size in points
    pub sample: Option<(String, f32)>,
    /// A–Z, a–z, 0–9 and punctuation
    pub charset: bool,
    /// how many paragraphs of lorem ipsum, and their size in points
    pub lorem: Option<(usize, f32)>,
    /// the date printed at the top
    pub date: String,
    pub label_font: Option<PathBuf>,
}

/// Classic placeholder paragraphs (the traditional "lorem ipsum" text).
pub const LOREM: [&str; 5] = [
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.",
    "Curabitur pretium tincidunt lacus. Nulla gravida orci a odio. Nullam varius, turpis et commodo pharetra, est eros bibendum elit, nec luctus magna felis sollicitudin mauris. Integer in mauris eu nibh euismod gravida. Duis ac tellus et risus vulputate vehicula. Donec lobortis risus a elit. Etiam tempor. Ut ullamcorper, ligula eu tempor congue, eros est euismod turpis, id tincidunt sapien risus a quam.",
    "Maecenas fermentum consequat mi. Donec fermentum. Pellentesque malesuada nulla a mi. Duis sapien sem, aliquet nec, commodo eget, consequat quis, neque. Aliquam faucibus, elit ut dictum aliquet, felis nisl adipiscing sapien, sed malesuada diam lacus eget erat. Cras mollis scelerisque nunc. Nullam arcu. Aliquam consequat. Curabitur augue lorem, dapibus quis, laoreet et, pretium ac, nisi.",
    "Aenean magna nisl, mollis quis, molestie eu, feugiat in, orci. In hac habitasse platea dictumst. Fusce convallis, mauris imperdiet gravida bibendum, nisl turpis suscipit mauris, sed placerat ipsum urna sed risus. In convallis tellus a mauris. Curabitur non elit ut libero tristique sodales. Mauris a lacus. Donec mattis semper leo. In hac habitasse platea dictumst.",
    "Vivamus facilisis diam at odio. Mauris dictum, nisi eget consequat elementum, lacus ligula molestie metus, non feugiat orci magna ac sem. Donec turpis. Donec vitae metus. Morbi tristique neque eu mauris. Quisque gravida ipsum non sapien. Proin turpis lacus, scelerisque vitae, elementum at, lobortis ac, quam. Aliquam dictum eleifend risus. In hac habitasse platea dictumst.",
];

/// The character set line of the specimen.
pub const CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789 &?!@#%(.,:;)";

/// What the sheet left out because the page was full.
pub struct SheetResult {
    pub left_out: Vec<String>,
}

/// Make the one-page PDF of a specimen sheet.
pub fn make_sheet_pdf(sheet: &Sheet, dest: &Path) -> Result<SheetResult, String> {
    let (page, left_out) = sheet_layout(sheet);
    write_pdf(&[page], &sheet.name, [255, 255, 255], dest)?;
    Ok(SheetResult { left_out })
}

/// Lines of text in the font at `pt` points, wrapped to `max_w` points (shaped when the script
/// needs it). The flag says the line is right to left.
#[allow(clippy::too_many_arguments)]
fn font_lines(font: &ab_glyph::FontArc, data: &[u8], index: u32, text: &str, pt: f32, max_w: f32, fg: [u8; 3], bg: [u8; 3]) -> Vec<(Rgb, bool)> {
    let px = pt * PX_PER_PT;
    let mut out = vec![];
    if crate::shape::needs_shaping(text) {
        let rtl = crate::shape::base_rtl(text);
        for line in crate::shape::wrap_lines(data, index, text, px, max_w * PX_PER_PT) {
            if let Some(pic) = crate::shape::render(data, index, &line, px, None, crate::shape::HAlign::Left, 1) {
                let mut rgb = Vec::with_capacity(pic.w * pic.h * 3);
                for a in &pic.alpha {
                    rgb.extend_from_slice(&mix(bg, fg, *a as f32 / 255.0));
                }
                out.push((Rgb { w: pic.w, h: pic.h, data: rgb }, rtl));
            }
        }
    } else {
        for line in wrap(font, px, text, max_w * PX_PER_PT) {
            if let Some(img) = draw_line(font, px, &line, fg, bg) {
                out.push((img, false));
            }
        }
    }
    out
}

/// One line in the font, cut off at `max_w` points (a waterfall line never wraps).
fn one_line(font: &ab_glyph::FontArc, data: &[u8], index: u32, text: &str, pt: f32, max_w: f32, fg: [u8; 3], bg: [u8; 3]) -> Option<(Rgb, bool)> {
    let px = pt * PX_PER_PT;
    let (img, rtl) = if crate::shape::needs_shaping(text) {
        let pic = crate::shape::render(data, index, text, px, None, crate::shape::HAlign::Left, 1)?;
        let mut rgb = Vec::with_capacity(pic.w * pic.h * 3);
        for a in &pic.alpha {
            rgb.extend_from_slice(&mix(bg, fg, *a as f32 / 255.0));
        }
        (Rgb { w: pic.w, h: pic.h, data: rgb }, crate::shape::base_rtl(text))
    } else {
        // a text too wide for the line ends after the last word that fits
        let mut t = text.to_string();
        let mut img = draw_line(font, px, &t, fg, bg)?;
        while img.w_pt() > max_w {
            match t.trim_end().rfind(' ') {
                Some(k) if k > 0 => {
                    t.truncate(k);
                    let trimmed = t.trim_end_matches([',', ';', ':', ' ']).to_string();
                    img = draw_line(font, px, &trimmed, fg, bg)?;
                }
                _ => break,
            }
        }
        (img, false)
    };
    Some((crop(img, (max_w * PX_PER_PT) as usize, rtl), rtl))
}

/// A picture no wider than `w` pixels (right-to-left lines keep their right end).
fn crop(img: Rgb, w: usize, keep_right: bool) -> Rgb {
    if img.w <= w || w == 0 {
        return img;
    }
    let start = if keep_right { img.w - w } else { 0 };
    let mut data = Vec::with_capacity(w * img.h * 3);
    for y in 0..img.h {
        let row = (y * img.w + start) * 3;
        data.extend_from_slice(&img.data[row..row + w * 3]);
    }
    Rgb { w, h: img.h, data }
}

/// A filled bar (a rule), `w` × `h` points.
fn bar(w: f32, h: f32, c: [u8; 3]) -> Rgb {
    let (w, h) = (((w * PX_PER_PT) as usize).max(1), ((h * PX_PER_PT).round() as usize).max(1));
    Rgb { w, h, data: c.iter().copied().cycle().take(w * h * 3).collect() }
}

fn fmt_pt(pt: f32) -> String {
    if (pt - pt.round()).abs() < 0.05 {
        format!("{:.0}", pt)
    } else {
        format!("{:.1}", pt)
    }
}

fn sheet_layout(sheet: &Sheet) -> (Vec<Placed>, Vec<String>) {
    let (ink, paper) = ([18u8, 18, 18], [255u8, 255, 255]);
    let weak = [120u8, 116, 110];
    let hair = [214u8, 210, 204];
    let accent = [166u8, 124, 66]; // FontCraft's sand
    let cw = PAGE_W - 2.0 * MARGIN;
    let foot_y = PAGE_H - MARGIN + 2.0;
    let bottom = foot_y - 14.0;
    let label = sheet.label_font.as_deref().and_then(|p| load(p, 0));
    let lab = |t: &str, pt: f32, c: [u8; 3]| label.as_ref().and_then(|l| draw_line(l, pt * PX_PER_PT, t, c, paper));
    let mut page: Vec<Placed> = vec![];
    let mut left_out: Vec<String> = vec![];
    let mut y = MARGIN;

    // ── top line: "FONT SPECIMEN" on the left, the date on the right
    if let Some(img) = lab("FONT SPECIMEN", 7.0, accent) {
        page.push(Placed { img, x: MARGIN, y });
    }
    if let Some(img) = lab(&sheet.date, 7.0, weak) {
        let x = PAGE_W - MARGIN - img.w_pt();
        page.push(Placed { img, x, y });
    }
    y += 14.0;

    let data = std::fs::read(&sheet.path).unwrap_or_default();
    let Some(font) = load(&sheet.path, sheet.index) else {
        if let Some(img) = lab(&format!("{} — this font could not be read", sheet.name), 12.0, ink) {
            page.push(Placed { img, x: MARGIN, y });
        }
        return (page, left_out);
    };
    // a font without Latin letters (Arabic, Hebrew…) shows its own script
    let latin = {
        use ab_glyph::Font;
        font.glyph_id('a').0 != 0 || font.glyph_id('A').0 != 0
    };
    let own_script = crate::shape::rtl_sample(&sheet.rtl).map(|s| s.to_string());
    let place = |page: &mut Vec<Placed>, img: Rgb, rtl: bool, x0: f32, w: f32, y: f32| {
        let x = if rtl { x0 + (w - img.w_pt()).max(0.0) } else { x0 };
        page.push(Placed { img, x: MARGIN + x, y });
    };

    // ── the name, large, in the font itself (smaller when a long name wouldn't fit)
    let title = if latin { sheet.name.clone() } else { own_script.clone().unwrap_or_else(|| sheet.name.clone()) };
    let mut tpt = 46.0;
    let mut timg = one_line(&font, &data, sheet.index, &title, tpt, f32::MAX / 4.0, ink, paper);
    if let Some((img, _)) = &timg {
        if img.w_pt() > cw {
            tpt = (tpt * cw / img.w_pt()).max(14.0);
            timg = one_line(&font, &data, sheet.index, &title, tpt, cw, ink, paper);
        }
    }
    if let Some((img, rtl)) = timg {
        let h = img.h_pt();
        place(&mut page, img, rtl, 0.0, cw, y);
        y += h + 2.0;
    }
    if !sheet.info.is_empty() {
        if let Some(img) = lab(&sheet.info, 8.5, weak) {
            let h = img.h_pt();
            page.push(Placed { img, x: MARGIN, y });
            y += h + 8.0;
        }
    }
    page.push(Placed { img: bar(cw, 1.2, ink), x: MARGIN, y });
    y += 12.0;

    // a section heading: small sand capitals, with a hairline across the rest of the width
    let section = |page: &mut Vec<Placed>, y: &mut f32, t: &str| {
        if let Some(img) = lab(t, 6.5, accent) {
            let (w, h) = (img.w_pt(), img.h_pt());
            page.push(Placed { img, x: MARGIN, y: *y });
            if cw - w - 8.0 > 10.0 {
                page.push(Placed { img: bar(cw - w - 8.0, 0.4, hair), x: MARGIN + w + 8.0, y: *y + h * 0.55 });
            }
            *y += h + 6.0;
        }
    };
    let section_h = 6.5 * 1.25 + 6.0;

    // The page below the name, like a magazine page:
    //   small waterfall sizes on the left │ characters and running text on the right
    //   the large waterfall sizes across the whole width
    //   the sample text across the whole width
    let col_gap = 22.0;
    let left_w = (cw - col_gap) * 0.5;
    let right_x = left_w + col_gap;
    let right_w = cw - right_x;
    let custom = !sheet.waterfall_text.trim().is_empty();
    let size_gutter = 26.0;
    let gap = |pt: f32| (pt * 0.12).clamp(1.5, 7.0);

    // the waterfall's lines: (size, picture, right to left, the size goes in the margin)
    let mut lines: Vec<(f32, Rgb, bool, bool)> = vec![];
    for &pt in &sheet.sizes {
        let (t, with_gutter) = if custom {
            (sheet.waterfall_text.clone(), true)
        } else if latin {
            (format!("{} {}pt", sheet.name, fmt_pt(pt)), false)
        } else {
            (own_script.clone().unwrap_or_else(|| sheet.name.clone()), true)
        };
        let w = if with_gutter { cw - size_gutter } else { cw };
        if let Some((img, rtl)) = one_line(&font, &data, sheet.index, &t, pt, w, ink, paper) {
            lines.push((pt, img, rtl, with_gutter));
        }
    }
    let line_h = |l: &(f32, Rgb, bool, bool)| l.1.h_pt() + gap(l.0);
    let in_left = |l: &(f32, Rgb, bool, bool)| l.1.w_pt() + if l.3 { size_gutter } else { 0.0 } <= left_w;

    // the sample text (full width) and the characters (right column)
    let sample_lines: Vec<(Rgb, bool)> = match &sheet.sample {
        Some((t, pt)) => {
            let t = if !latin && !t.chars().any(crate::shape::is_rtl_char) { own_script.clone().unwrap_or_else(|| t.clone()) } else { t.clone() };
            font_lines(&font, &data, sheet.index, &t, *pt, cw, ink, paper).into_iter().take(3).collect()
        }
        None => vec![],
    };
    let sample_pt = sheet.sample.as_ref().map(|s| s.1).unwrap_or(0.0);
    let sample_h = |n: usize| if n == 0 { 0.0 } else { 12.0 + section_h + sample_lines.iter().take(n).map(|(i, _)| i.h_pt() + sample_pt * 0.08).sum::<f32>() };
    let chars_pt = 12.0;
    let chars: Vec<(Rgb, bool)> = if sheet.charset && latin { font_lines(&font, &data, sheet.index, CHARSET, chars_pt, right_w, ink, paper) } else { vec![] };
    let chars_h = if chars.is_empty() { 0.0 } else { section_h + chars.iter().map(|(i, _)| i.h_pt() + 1.0).sum::<f32>() + 10.0 };
    // the running text: lines for the right column
    let (lorem_pt, lorem_lines): (f32, Vec<Option<Rgb>>) = match sheet.lorem {
        Some((n, pt)) => {
            let mut v = vec![];
            for (k, para) in LOREM.iter().cycle().take(n.max(1)).enumerate() {
                if k > 0 {
                    v.push(None);
                }
                for (img, _) in font_lines(&font, &data, sheet.index, para, pt, right_w, ink, paper) {
                    v.push(Some(img));
                }
            }
            (pt, v)
        }
        None => (0.0, vec![]),
    };
    let lorem_lh = lorem_lines.iter().flatten().map(|i| i.h_pt()).fold(0.0, f32::max) + lorem_pt * 0.12;
    let lorem_min = if lorem_lines.is_empty() { 0.0 } else { section_h + lorem_lh * 4.0 };

    // what fits: the largest waterfall sizes go first, then sample text lines, when the page is full
    let room = bottom - y;
    let mut n_sample = sample_lines.len();
    let height = |lines: &[(f32, Rgb, bool, bool)], n_sample: usize| -> f32 {
        let left: f32 = lines.iter().filter(|l| in_left(l)).map(line_h).sum::<f32>();
        let left = if left > 0.0 { left + section_h } else { 0.0 };
        let right = chars_h + lorem_min;
        let full: f32 = lines.iter().filter(|l| !in_left(l)).map(line_h).sum::<f32>();
        left.max(right) + if full > 0.0 { 8.0 + full } else { 0.0 } + sample_h(n_sample)
    };
    while height(&lines, n_sample) > room {
        if n_sample > 1 {
            n_sample -= 1;
        } else if let Some(k) = lines.iter().enumerate().max_by(|a, b| a.1 .0.total_cmp(&b.1 .0)).map(|(k, _)| k) {
            let (pt, _, _, _) = lines.remove(k);
            left_out.push(format!("{} pt", fmt_pt(pt)));
        } else {
            break;
        }
    }
    let draw_wf = |page: &mut Vec<Placed>, y: &mut f32, l: (f32, Rgb, bool, bool), w: f32| {
        let (pt, img, rtl, with_gutter) = l;
        let h = img.h_pt();
        if with_gutter {
            if let Some(lb) = lab(&fmt_pt(pt), 6.5, weak) {
                let ly = *y + (h * 0.62 - lb.h_pt() * 0.6).max(0.0);
                page.push(Placed { img: lb, x: MARGIN, y: ly });
            }
            place(page, img, rtl, size_gutter, w - size_gutter, *y);
        } else {
            place(page, img, rtl, 0.0, w, *y);
        }
        *y += h + gap(pt);
    };
    let (small, large): (Vec<_>, Vec<_>) = lines.into_iter().partition(|l| in_left(l));
    let large_h: f32 = large.iter().map(line_h).sum();
    // the band: how tall the two columns are (the right one fills up to the left one)
    let left_h = if small.is_empty() { 0.0 } else { section_h + small.iter().map(line_h).sum::<f32>() };
    let band_h = left_h.max(chars_h + lorem_min).min(bottom - y - if large_h > 0.0 { 8.0 + large_h } else { 0.0 } - sample_h(n_sample));
    let top = y;
    let any_small = !small.is_empty();
    // left: the small sizes
    if any_small {
        section(&mut page, &mut y, "WATERFALL");
        for l in small {
            draw_wf(&mut page, &mut y, l, left_w);
        }
    }
    // right: the characters, then as much running text as the band holds
    let mut ry = top;
    let rsection = |page: &mut Vec<Placed>, y: &mut f32, t: &str| {
        if let Some(img) = lab(t, 6.5, accent) {
            let (w, h) = (img.w_pt(), img.h_pt());
            page.push(Placed { img, x: MARGIN + right_x, y: *y });
            if right_w - w - 8.0 > 10.0 {
                page.push(Placed { img: bar(right_w - w - 8.0, 0.4, hair), x: MARGIN + right_x + w + 8.0, y: *y + h * 0.55 });
            }
            *y += h + 6.0;
        }
    };
    if !chars.is_empty() {
        rsection(&mut page, &mut ry, &format!("CHARACTERS  ·  {} PT", fmt_pt(chars_pt)));
        for (img, rtl) in chars {
            let h = img.h_pt() + 1.0;
            place(&mut page, img, rtl, right_x, right_w, ry);
            ry += h;
        }
        ry += 10.0;
    }
    if !lorem_lines.is_empty() {
        let fit = ((top + band_h - ry - section_h) / lorem_lh.max(1.0)).floor().max(0.0) as usize;
        if fit < 2 {
            left_out.push("the running text".into());
        } else {
            rsection(&mut page, &mut ry, &format!("TEXT  ·  {} PT", fmt_pt(lorem_pt)));
            let mut shown = lorem_lines.into_iter().take(fit).collect::<Vec<_>>();
            // don't end the column on an empty line between paragraphs
            while matches!(shown.last(), Some(None)) {
                shown.pop();
            }
            for line in shown {
                if let Some(img) = line {
                    page.push(Placed { img, x: MARGIN + right_x, y: ry });
                }
                ry += lorem_lh;
            }
        }
    }
    // a fine line between the two columns
    let band_bottom = y.max(ry);
    if band_bottom - top > 20.0 {
        page.push(Placed { img: bar(0.6, band_bottom - top - 4.0, hair), x: MARGIN + left_w + col_gap / 2.0, y: top + 2.0 });
    }
    y = band_bottom;
    // the large sizes across the page
    if !large.is_empty() {
        y += 8.0;
        if !any_small {
            section(&mut page, &mut y, "WATERFALL");
        }
        for l in large {
            draw_wf(&mut page, &mut y, l, cw);
        }
    }
    // the sample text
    if n_sample > 0 {
        y += 12.0;
        section(&mut page, &mut y, &format!("SAMPLE TEXT  ·  {} PT", fmt_pt(sample_pt)));
        for (img, rtl) in sample_lines.into_iter().take(n_sample) {
            let h = img.h_pt() + sample_pt * 0.08;
            place(&mut page, img, rtl, 0.0, cw, y);
            y += h;
        }
    } else if sheet.sample.is_some() {
        left_out.push("the sample text".into());
    }


    // ── footer
    page.push(Placed { img: bar(cw, 0.4, hair), x: MARGIN, y: foot_y - 6.0 });
    if let Some(img) = lab(&sheet.name, 7.0, weak) {
        page.push(Placed { img, x: MARGIN, y: foot_y });
    }
    if let Some(img) = lab("FontCraft", 7.0, weak) {
        let x = PAGE_W - MARGIN - img.w_pt();
        page.push(Placed { img, x, y: foot_y });
    }
    (page, left_out)
}

/// Every glyph of one font style, one size, in a grid grouped by Unicode block, on as many pages
/// as it takes. Each glyph has its code (U+…) under it.
pub struct GlyphSheet {
    pub name: String,
    pub info: String,
    pub path: PathBuf,
    pub index: u32,
    /// the size of the glyphs in points
    pub pt: f32,
    pub date: String,
    pub label_font: Option<PathBuf>,
}

/// Make the glyph overview PDF. Returns (pages, glyphs).
pub fn make_glyph_pdf(sheet: &GlyphSheet, dest: &Path) -> Result<(usize, usize), String> {
    let (pages, n) = glyph_layout(sheet)?;
    write_pdf(&pages, &format!("{} – glyphs", sheet.name), [255, 255, 255], dest)?;
    Ok((pages.len(), n))
}

/// Copy `src` into `dst` at pixel (x, y), keeping the darker of the two (black ink on white).
fn blit(dst: &mut Rgb, src: &Rgb, x: i64, y: i64) {
    for sy in 0..src.h {
        let dy = y + sy as i64;
        if dy < 0 || dy >= dst.h as i64 {
            continue;
        }
        for sx in 0..src.w {
            let dx = x + sx as i64;
            if dx < 0 || dx >= dst.w as i64 {
                continue;
            }
            let si = (sy * src.w + sx) * 3;
            let di = (dy as usize * dst.w + dx as usize) * 3;
            for c in 0..3 {
                dst.data[di + c] = dst.data[di + c].min(src.data[si + c]);
            }
        }
    }
}

fn fill(dst: &mut Rgb, x: usize, y: usize, w: usize, h: usize, c: [u8; 3]) {
    for yy in y..(y + h).min(dst.h) {
        for xx in x..(x + w).min(dst.w) {
            let i = (yy * dst.w + xx) * 3;
            dst.data[i..i + 3].copy_from_slice(&c);
        }
    }
}

fn glyph_layout(sheet: &GlyphSheet) -> Result<(Vec<Vec<Placed>>, usize), String> {
    let (ink, paper) = ([18u8, 18, 18], [255u8, 255, 255]);
    let weak = [120u8, 116, 110];
    let hair = [222u8, 218, 212];
    let accent = [166u8, 124, 66];
    let cw = PAGE_W - 2.0 * MARGIN;
    let foot_y = PAGE_H - MARGIN + 2.0;
    let bottom = foot_y - 14.0;
    let font = load(&sheet.path, sheet.index).ok_or("this font could not be read")?;
    let label = sheet.label_font.as_deref().and_then(|p| load(p, 0));
    let lab = |t: &str, pt: f32, c: [u8; 3]| label.as_ref().and_then(|l| draw_line(l, pt * PX_PER_PT, t, c, paper));
    let glyphs: Vec<char> = crate::library::glyph_list(&sheet.path, sheet.index).into_iter().map(|g| g.ch).filter(|c| !c.is_control()).collect();
    // group by Unicode block, in code order
    let mut blocks: Vec<(&'static str, Vec<char>)> = vec![];
    for c in &glyphs {
        let b = crate::library::unicode_block(*c);
        match blocks.last_mut() {
            Some((n, v)) if *n == b => v.push(*c),
            _ => blocks.push((b, vec![*c])),
        }
    }
    let pt = sheet.pt.clamp(4.0, 72.0);
    let cols = ((cw / (pt * 2.1).max(26.0)).floor() as usize).max(1);
    let cell_w = cw / cols as f32;
    let code_pt = 4.8;
    let cell_h = pt * 1.45 + code_pt * 1.6 + 6.0;
    let mut pages: Vec<Vec<Placed>> = vec![vec![]];
    let mut y = MARGIN;

    // the first page's head: what it is, the name in the font, the facts
    if let Some(img) = lab("GLYPH OVERVIEW", 7.0, accent) {
        pages[0].push(Placed { img, x: MARGIN, y });
    }
    if let Some(img) = lab(&sheet.date, 7.0, weak) {
        let x = PAGE_W - MARGIN - img.w_pt();
        pages[0].push(Placed { img, x, y });
    }
    y += 14.0;
    let data = std::fs::read(&sheet.path).unwrap_or_default();
    let latin = {
        use ab_glyph::Font;
        font.glyph_id('a').0 != 0 || font.glyph_id('A').0 != 0
    };
    if let Some((img, _)) = latin.then(|| one_line(&font, &data, sheet.index, &sheet.name, 32.0, cw, ink, paper)).flatten().or_else(|| lab(&sheet.name, 24.0, ink).map(|i| (i, false))) {
        let h = img.h_pt();
        pages[0].push(Placed { img, x: MARGIN, y });
        y += h + 2.0;
    }
    let info = format!("{}{}{} glyphs  ·  {} pt", sheet.info, if sheet.info.is_empty() { "" } else { "  ·  " }, glyphs.len(), fmt_pt(pt));
    if let Some(img) = lab(&info, 8.5, weak) {
        let h = img.h_pt();
        pages[0].push(Placed { img, x: MARGIN, y });
        y += h + 8.0;
    }
    pages[0].push(Placed { img: bar(cw, 1.2, ink), x: MARGIN, y });
    y += 12.0;

    let px = |v: f32| (v * PX_PER_PT).round() as usize;
    let new_page = |pages: &mut Vec<Vec<Placed>>, y: &mut f32| {
        pages.push(vec![]);
        *y = MARGIN;
    };
    for (block, chars) in &blocks {
        // the block's name, kept together with its first row
        let head_h = 7.0 * 1.25 + 5.0;
        if y + head_h + cell_h > bottom {
            new_page(&mut pages, &mut y);
        }
        if let Some(img) = lab(&format!("{}  ·  {}", block.to_uppercase(), chars.len()), 6.5, accent) {
            let (w, h) = (img.w_pt(), img.h_pt());
            pages.last_mut().unwrap().push(Placed { img, x: MARGIN, y });
            if cw - w - 8.0 > 10.0 {
                pages.last_mut().unwrap().push(Placed { img: bar(cw - w - 8.0, 0.4, hair), x: MARGIN + w + 8.0, y: y + h * 0.55 });
            }
            y += head_h;
        }
        // one picture per row of cells: light boxes, the glyph in the middle, its code below
        for row in chars.chunks(cols) {
            if y + cell_h > bottom {
                new_page(&mut pages, &mut y);
            }
            let (rw, rh) = (px(cw), px(cell_h));
            let mut img = Rgb { w: rw, h: rh, data: paper.iter().copied().cycle().take(rw * rh * 3).collect() };
            let line = 2usize;
            fill(&mut img, 0, 0, rw, line, hair);
            fill(&mut img, 0, rh - line, rw, line, hair);
            for k in 0..=cols {
                let x = (px(cell_w * k as f32)).min(rw - line);
                fill(&mut img, x, 0, line, rh, hair);
            }
            for (k, ch) in row.iter().enumerate() {
                let x0 = cell_w * k as f32;
                if let Some(g) = draw_line(&font, pt * PX_PER_PT, &ch.to_string(), ink, paper) {
                    let gx = px(x0) as i64 + (px(cell_w) as i64 - g.w as i64) / 2;
                    blit(&mut img, &g, gx, px(3.0) as i64);
                }
                if let Some(c) = lab(&format!("{:04X}", *ch as u32), code_pt, weak) {
                    let cx = px(x0) as i64 + (px(cell_w) as i64 - c.w as i64) / 2;
                    blit(&mut img, &c, cx, (rh - c.h - px(2.0)) as i64);
                }
            }
            pages.last_mut().unwrap().push(Placed { img, x: MARGIN, y });
            y += cell_h - 0.5;
        }
        y += 10.0;
    }
    // footer on every page: the font, "glyphs", page number
    let n = pages.len();
    for (i, page) in pages.iter_mut().enumerate() {
        page.push(Placed { img: bar(cw, 0.4, hair), x: MARGIN, y: foot_y - 6.0 });
        if let Some(img) = lab(&format!("{}  ·  glyphs", sheet.name), 7.0, weak) {
            page.push(Placed { img, x: MARGIN, y: foot_y });
        }
        if let Some(img) = lab(&format!("{} / {}", i + 1, n), 7.0, weak) {
            let x = PAGE_W - MARGIN - img.w_pt();
            page.push(Placed { img, x, y: foot_y });
        }
    }
    Ok((pages, glyphs.len()))
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// Text for the PDF's information (plain ASCII, so any viewer shows it).
fn pdf_string(s: &str) -> String {
    let clean: String = s.chars().map(|c| if c.is_ascii() && !c.is_ascii_control() { c } else { '?' }).collect();
    format!("({})", clean.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)"))
}

fn write_pdf(pages: &[Vec<Placed>], title: &str, bg: [u8; 3], dest: &Path) -> Result<(), String> {
    // object numbers: 1 catalog, 2 page tree, 3 info, then per page: page, content, pictures
    let mut objs: Vec<Vec<u8>> = vec![vec![], vec![], vec![]];
    let mut page_ids = vec![];
    for page in pages {
        let page_id = objs.len() + 1;
        let content_id = page_id + 1;
        objs.push(vec![]);
        objs.push(vec![]);
        let mut xobjects = String::new();
        let mut content = String::new();
        if bg != [255, 255, 255] {
            let c = |v: u8| v as f32 / 255.0;
            content.push_str(&format!("{:.4} {:.4} {:.4} rg 0 0 {PAGE_W:.2} {PAGE_H:.2} re f\n", c(bg[0]), c(bg[1]), c(bg[2])));
        }
        for (k, pl) in page.iter().enumerate() {
            let img_id = objs.len() + 1;
            let data = deflate(&pl.img.data);
            let mut obj = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                pl.img.w,
                pl.img.h,
                data.len()
            )
            .into_bytes();
            obj.extend_from_slice(&data);
            obj.extend_from_slice(b"\nendstream");
            objs.push(obj);
            xobjects.push_str(&format!("/Im{k} {img_id} 0 R "));
            // PDF measures from the bottom left
            let (w, h) = (pl.img.w_pt(), pl.img.h_pt());
            content.push_str(&format!("q {w:.3} 0 0 {h:.3} {:.3} {:.3} cm /Im{k} Do Q\n", pl.x, PAGE_H - pl.y - h));
        }
        let cdata = deflate(content.as_bytes());
        let mut cobj = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", cdata.len()).into_bytes();
        cobj.extend_from_slice(&cdata);
        cobj.extend_from_slice(b"\nendstream");
        objs[content_id - 1] = cobj;
        objs[page_id - 1] = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.2} {PAGE_H:.2}] /Resources << /XObject << {xobjects}>> >> /Contents {content_id} 0 R >>"
        )
        .into_bytes();
        page_ids.push(page_id);
    }
    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    let kids: Vec<String> = page_ids.iter().map(|i| format!("{i} 0 R")).collect();
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), page_ids.len()).into_bytes();
    objs[2] = format!("<< /Title {} /Creator (FontCraft) /Producer (FontCraft) >>", pdf_string(title)).into_bytes();
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![];
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes(),
    );
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(dest, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PDF with a few installed fonts: it has pages, the right structure and an end marker.
    #[test]
    fn prints_a_collection() {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let fonts: Vec<PathBuf> =
            ["arial.ttf", "times.ttf", "consola.ttf", "segoeui.ttf"].iter().map(|f| win.join("Fonts").join(f)).filter(|p| p.exists()).collect();
        if fonts.is_empty() {
            return;
        }
        let items: Vec<Item> = (0..30)
            .map(|i| Item {
                name: format!("Font {i}"),
                info: "TTF · 1 style".into(),
                path: fonts[i % fonts.len()].clone(),
                index: 0,
                // every fifth one as an Arabic font: printed with the Arabic sample, shaped, from the right
                rtl: if i % 5 == 4 { "Arabic".into() } else { String::new() },
            })
            .collect();
        let spec = Spec {
            title: "Test collection".into(),
            items,
            text: "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.".into(),
            size_px: 42.0,
            fg: [240, 235, 225],
            bg: [30, 28, 25],
            align: HAlign::Center,
            label_font: Some(fonts[0].clone()),
        };
        let dest = std::env::temp_dir().join("fontcraft-print-test.pdf");
        let n = make_pdf(&spec, &dest).unwrap();
        assert!(n >= 2, "30 fonts with two lines each should need more than one page, got {n}");
        let data = std::fs::read(&dest).unwrap();
        assert!(data.starts_with(b"%PDF-1.4"));
        assert!(data.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&data);
        assert_eq!(text.matches("/Type /Page ").count(), n);
        let _ = std::fs::remove_file(&dest);
        // a picture of the first page, to look at (set FONTCRAFT_PRINT_PNG to a file)
        if let Some(png) = std::env::var_os("FONTCRAFT_PRINT_PNG") {
            page_png(&layout(&spec)[0], spec.bg, &PathBuf::from(png), 100.0);
        }
    }

    /// A picture of a page at `dpi`.
    fn page_png(page: &[Placed], bg: [u8; 3], dest: &Path, dpi: f32) {
        let s = dpi / 72.0;
        let (w, h) = ((PAGE_W * s) as u32, (PAGE_H * s) as u32);
        let mut img = image::RgbImage::from_pixel(w, h, image::Rgb(bg));
        for pl in page {
            let k = PX_PER_PT / s;
            for y in 0..(pl.img.h as f32 / k) as u32 {
                for x in 0..(pl.img.w as f32 / k) as u32 {
                    let (sx, sy) = ((x as f32 * k) as usize, (y as f32 * k) as usize);
                    let i = (sy * pl.img.w + sx) * 3;
                    let (px, py) = ((pl.x * s) as u32 + x, (pl.y * s) as u32 + y);
                    if px < w && py < h && i + 2 < pl.img.data.len() {
                        img.put_pixel(px, py, image::Rgb([pl.img.data[i], pl.img.data[i + 1], pl.img.data[i + 2]]));
                    }
                }
            }
        }
        img.save(dest).unwrap();
    }

    /// Every glyph is printed, on as many pages as needed, and nothing leaves the page.
    #[test]
    fn prints_all_glyphs() {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let font = win.join("Fonts").join("tahoma.ttf");
        if !font.exists() {
            return;
        }
        let sheet = GlyphSheet {
            name: "Tahoma".into(),
            info: "Regular  ·  TTF".into(),
            path: font.clone(),
            index: 0,
            pt: 12.0,
            date: "2026-11-24".into(),
            label_font: Some(font.clone()),
        };
        let (pages, n) = glyph_layout(&sheet).unwrap();
        let all = crate::library::glyph_list(&font, 0).into_iter().filter(|g| !g.ch.is_control()).count();
        assert_eq!(n, all);
        assert!(pages.len() > 1, "Tahoma has {n} glyphs: more than one page");
        for page in &pages {
            for pl in page {
                assert!(pl.y + pl.img.h_pt() <= PAGE_H + 0.5);
            }
        }
        let dest = std::env::temp_dir().join("fontcraft-glyphs-test.pdf");
        let (np, _) = make_glyph_pdf(&sheet, &dest).unwrap();
        assert_eq!(np, pages.len());
        let _ = std::fs::remove_file(&dest);
        if let Some(png) = std::env::var_os("FONTCRAFT_GLYPH_PNG") {
            let png = PathBuf::from(png);
            page_png(&pages[0], [255; 3], &png, 110.0);
            page_png(&pages[pages.len() / 2], [255; 3], &png.with_extension("2.png"), 110.0);
            eprintln!("pages: {}, glyphs: {n}", pages.len());
        }
    }

    /// The print sheet is one page, sizes are real (a 72 pt line is about 72 pt high), and what
    /// doesn't fit is named.
    #[test]
    fn prints_a_sheet() {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let font = win.join("Fonts").join("tahoma.ttf");
        if !font.exists() {
            return;
        }
        let sheet = Sheet {
            name: "Tahoma".into(),
            info: "Regular  ·  2 styles  ·  TTF  ·  Matthew Carter  ·  Version 7.0".into(),
            path: font.clone(),
            index: 0,
            rtl: String::new(),
            sizes: crate::config::PrintSheet::default().size_list(),
            waterfall_text: String::new(),
            sample: Some(("The quick brown fox jumps over the lazy dog.".into(), 18.0)),
            charset: true,
            lorem: Some((3, 9.0)),
            date: "2026-11-24".into(),
            label_font: Some(font.clone()),
        };
        let (page, left_out) = sheet_layout(&sheet);
        assert!(!page.is_empty());
        for pl in &page {
            assert!(pl.y + pl.img.h_pt() <= PAGE_H + 0.5, "everything stays on the page");
        }
        // em size: Tahoma's line (ascent to descent) is about 1.2 em
        let f = load(&font, 0).unwrap();
        let line = draw_line(&f, 72.0 * PX_PER_PT, "Tahoma", [0; 3], [255; 3]).unwrap();
        assert!((line.h_pt() / 72.0 - 1.2).abs() < 0.12, "72 pt line is {} pt high", line.h_pt());
        let dest = std::env::temp_dir().join("fontcraft-sheet-test.pdf");
        make_sheet_pdf(&sheet, &dest).unwrap();
        let data = std::fs::read(&dest).unwrap();
        assert_eq!(String::from_utf8_lossy(&data).matches("/Type /Page ").count(), 1);
        let _ = std::fs::remove_file(&dest);
        if let Some(png) = std::env::var_os("FONTCRAFT_SHEET_PNG") {
            page_png(&page, [255; 3], &PathBuf::from(png), 110.0);
            eprintln!("left out: {left_out:?}");
        }
        // with the sample text in the waterfall the size goes in the margin
        let s2 = Sheet { waterfall_text: "Sphinx of black quartz, judge my vow.".into(), lorem: Some((5, 8.0)), ..sheet };
        let (page2, _) = sheet_layout(&s2);
        if let Some(png) = std::env::var_os("FONTCRAFT_SHEET_PNG2") {
            page_png(&page2, [255; 3], &PathBuf::from(png), 110.0);
        }
    }
}
