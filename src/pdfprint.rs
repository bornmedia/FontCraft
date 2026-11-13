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

/// Lines of `text` set in `font` at `px` pixels, wrapped to `max_w` pixels.
fn wrap(font: &ab_glyph::FontArc, px: f32, text: &str, max_w: f32) -> Vec<String> {
    use ab_glyph::{Font, ScaleFont};
    let sf = font.as_scaled(px);
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
    let sf = font.as_scaled(px);
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
        glyphs.push(id.with_scale_and_position(px, point(x, ascent)));
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
    write_pdf(&pages, spec, dest)?;
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
        match load(&item.path, item.index) {
            Some(font) => {
                for line in wrap(&font, px, &spec.text, content_w * PX_PER_PT) {
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

fn write_pdf(pages: &[Vec<Placed>], spec: &Spec, dest: &Path) -> Result<(), String> {
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
        if spec.bg != [255, 255, 255] {
            let c = |v: u8| v as f32 / 255.0;
            content.push_str(&format!(
                "{:.4} {:.4} {:.4} rg 0 0 {PAGE_W:.2} {PAGE_H:.2} re f\n",
                c(spec.bg[0]),
                c(spec.bg[1]),
                c(spec.bg[2])
            ));
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
    objs[2] = format!("<< /Title {} /Creator (FontCraft) /Producer (FontCraft) >>", pdf_string(&spec.title)).into_bytes();
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
            .map(|i| Item { name: format!("Font {i}"), info: "TTF · 1 style".into(), path: fonts[i % fonts.len()].clone(), index: 0 })
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
            let pages = layout(&spec);
            let s = 100.0 / 72.0; // 100 dpi
            let (w, h) = ((PAGE_W * s) as u32, (PAGE_H * s) as u32);
            let mut img = image::RgbImage::from_pixel(w, h, image::Rgb(spec.bg));
            for pl in &pages[0] {
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
            img.save(PathBuf::from(png)).unwrap();
        }
    }
}
