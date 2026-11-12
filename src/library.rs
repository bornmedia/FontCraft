//! Reading font files: names, styles, metadata, glyphs. Folder scanning with a cache.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use ttf_parser::{name_id, Face, Language};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Axis {
    pub tag: String,
    pub name: String,
    pub min: f32,
    pub def: f32,
    pub max: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FaceMeta {
    pub path: PathBuf,
    pub index: u32,
    pub family: String,
    pub style: String,
    pub full_name: String,
    pub postscript: String,
    pub version: String,
    pub designer: String,
    pub manufacturer: String,
    pub description: String,
    pub license: String,
    pub copyright: String,
    pub url: String,
    pub weight: u16,
    pub width: u16,
    pub italic: bool,
    pub monospace: bool,
    pub glyph_count: u16,
    pub axes: Vec<Axis>,
    pub features: Vec<String>,
    pub format: String,
    pub file_size: u64,
    /// Measured shape, for Super search (None for symbol fonts without an "o").
    #[serde(default)]
    pub shape: Option<Shape>,
    /// serif / sans / script / display / symbol (from the font's PANOSE class), or empty.
    #[serde(default)]
    pub class: String,
}

/// Shape measurements, each 0..1.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Shape {
    /// 0 = even strokes everywhere, 1 = very thin hairlines next to thick stems
    pub contrast: f32,
    /// 0 = very narrow letters, 1 = very wide
    pub proportion: f32,
    /// 0 = tiny lowercase, 1 = lowercase almost as tall as capitals
    pub xheight: f32,
    /// 0 = hairline, 1 = black
    pub weight: f32,
}

impl Shape {
    pub fn get(&self, k: usize) -> f32 {
        [self.contrast, self.proportion, self.xheight, self.weight][k]
    }
}

pub const SHAPE_NAMES: [&str; 4] = ["Contrast", "Proportion", "x-height", "Weight"];

/// Rasterise "o" and measure its strokes; read x-height and letter widths.
fn measure_shape(data: &[u8], index: u32, face: &Face) -> Option<Shape> {
    use ab_glyph::{Font, FontRef, PxScale};
    let font = FontRef::try_from_slice_and_index(data, index).ok()?;
    let gid = font.glyph_id('o');
    if gid.0 == 0 {
        return None;
    }
    let scale = PxScale::from(600.0);
    let glyph = gid.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
    let og = font.outline_glyph(glyph)?;
    let b = og.px_bounds();
    let (w, h) = (b.width().ceil() as usize + 1, b.height().ceil() as usize + 1);
    if w < 8 || h < 8 || w > 2000 || h > 2000 {
        return None;
    }
    let mut ink = vec![false; w * h];
    og.draw(|x, y, c| {
        if c > 0.5 && (x as usize) < w && (y as usize) < h {
            ink[y as usize * w + x as usize] = true;
        }
    });
    let run = |cells: &mut dyn Iterator<Item = bool>| -> usize {
        let mut started = false;
        let mut n = 0;
        for c in cells {
            if c {
                started = true;
                n += 1;
            } else if started {
                break;
            }
        }
        n
    };
    let cy = h / 2;
    let cx = w / 2;
    let thick = run(&mut (0..w).map(|x| ink[cy * w + x])).max(run(&mut (0..w).rev().map(|x| ink[cy * w + x]))) as f32;
    let thin = run(&mut (0..h).map(|y| ink[y * w + cx])).min(run(&mut (0..h).rev().map(|y| ink[y * w + cx]))) as f32;
    if thick < 1.0 {
        return None;
    }
    let contrast = (1.0 - thin / thick).clamp(0.0, 1.0);
    let weight = ((thick / h as f32 - 0.04) / 0.32).clamp(0.0, 1.0);
    let upem = face.units_per_em() as f32;
    let xh = face
        .x_height()
        .filter(|v| *v > 0)
        .map(|v| v as f32)
        .or_else(|| face.glyph_index('x').and_then(|g| face.glyph_bounding_box(g)).map(|r| r.y_max as f32))?;
    let xheight = ((xh / upem - 0.3) / 0.35).clamp(0.0, 1.0);
    // letter widths in font units (same units as the x-height)
    let mut adv = 0.0;
    let mut n = 0.0;
    for c in "abcdefghijklmnopqrstuvwxyz".chars() {
        if let Some(a) = face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)) {
            adv += a as f32;
            n += 1.0;
        }
    }
    let ratio = if n > 0.0 && xh > 0.0 { adv / n / xh } else { 1.0 };
    let proportion = ((ratio - 0.6) / 1.0).clamp(0.0, 1.0);
    Some(Shape { contrast, proportion, xheight, weight })
}

/// PANOSE classification from the OS/2 table.
fn panose_class(face: &Face) -> String {
    let Some(os2) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"OS/2")) else { return String::new() };
    if os2.len() < 42 {
        return String::new();
    }
    let (family, serif) = (os2[32], os2[33]);
    match family {
        2 => match serif {
            2..=10 => "serif",
            11..=15 => "sans",
            _ => "",
        },
        3 => "script",
        4 => "display",
        5 => "symbol",
        _ => "",
    }
    .to_string()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    System,
    Folder(PathBuf),
}

pub const FONT_EXTS: &[&str] = &["ttf", "otf", "ttc", "otc"];

pub fn is_font_file(p: &Path) -> bool {
    p.extension()
        .map(|e| FONT_EXTS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn name_of(face: &Face, id: u16) -> Option<String> {
    let mut best: Option<String> = None;
    for n in face.names() {
        if n.name_id != id {
            continue;
        }
        if let Some(s) = n.to_string() {
            let s = s.trim().to_string();
            if s.is_empty() {
                continue;
            }
            if n.language() == Language::English_UnitedStates {
                return Some(s);
            }
            if best.is_none() {
                best = Some(s);
            }
        }
    }
    best
}

fn tag_str(t: ttf_parser::Tag) -> String {
    String::from_utf8_lossy(&t.to_bytes()).trim().to_string()
}

pub fn parse_file(path: &Path) -> Vec<FaceMeta> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    let is_collection = ttf_parser::fonts_in_collection(&data).is_some();
    let mut out = Vec::new();
    for index in 0..count.min(64) {
        let Ok(face) = Face::parse(&data, index) else { continue };
        // Make sure the renderer can read it too.
        if ab_glyph::FontRef::try_from_slice_and_index(&data, index).is_err() {
            continue;
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let family = name_of(&face, name_id::TYPOGRAPHIC_FAMILY)
            .or_else(|| name_of(&face, name_id::FAMILY))
            .unwrap_or_else(|| stem.clone());
        let style = name_of(&face, name_id::TYPOGRAPHIC_SUBFAMILY)
            .or_else(|| name_of(&face, name_id::SUBFAMILY))
            .unwrap_or_else(|| "Regular".into());
        let mut features = BTreeSet::new();
        if let Some(t) = face.tables().gsub {
            for f in t.features {
                features.insert(tag_str(f.tag));
            }
        }
        if let Some(t) = face.tables().gpos {
            for f in t.features {
                features.insert(tag_str(f.tag));
            }
        }
        let axes = face
            .variation_axes()
            .into_iter()
            .filter(|a| !a.hidden)
            .map(|a| Axis {
                tag: tag_str(a.tag),
                name: name_of(&face, a.name_id).unwrap_or_else(|| tag_str(a.tag)),
                min: a.min_value,
                def: a.def_value,
                max: a.max_value,
            })
            .collect::<Vec<_>>();
        let cff = face.tables().cff.is_some() || face.tables().cff2.is_some();
        let mut format = if cff { "OpenType (PostScript outlines)".to_string() } else { "TrueType".to_string() };
        if is_collection {
            format.push_str(" collection");
        }
        if !axes.is_empty() {
            format.push_str(", variable");
        }
        out.push(FaceMeta {
            path: path.to_path_buf(),
            index,
            full_name: name_of(&face, name_id::FULL_NAME).unwrap_or_else(|| format!("{family} {style}")),
            postscript: name_of(&face, name_id::POST_SCRIPT_NAME).unwrap_or_default(),
            version: name_of(&face, name_id::VERSION).unwrap_or_default(),
            designer: name_of(&face, name_id::DESIGNER).unwrap_or_default(),
            manufacturer: name_of(&face, name_id::MANUFACTURER).unwrap_or_default(),
            description: name_of(&face, name_id::DESCRIPTION).unwrap_or_default(),
            license: name_of(&face, name_id::LICENSE).unwrap_or_default(),
            copyright: name_of(&face, name_id::COPYRIGHT_NOTICE).unwrap_or_default(),
            url: name_of(&face, name_id::DESIGNER_URL)
                .or_else(|| name_of(&face, name_id::VENDOR_URL))
                .unwrap_or_default(),
            weight: face.weight().to_number(),
            width: face.width().to_number(),
            italic: face.is_italic() || face.is_oblique(),
            monospace: face.is_monospaced(),
            glyph_count: face.number_of_glyphs(),
            axes,
            features: features.into_iter().collect(),
            format,
            file_size: data.len() as u64,
            shape: measure_shape(&data, index, &face),
            class: panose_class(&face),
            family,
            style,
        });
    }
    out
}

// ---------------------------------------------------------------- scan cache

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CacheEntry {
    pub mtime: u64,
    pub size: u64,
    pub faces: Vec<FaceMeta>,
    /// Entries from older FontCraft versions lack newer measurements and are read again.
    #[serde(default)]
    pub version: u32,
}

const CACHE_VERSION: u32 = 3;

pub type ScanCache = HashMap<String, CacheEntry>;

fn file_stamp(p: &Path) -> Option<(u64, u64)> {
    let m = std::fs::metadata(p).ok()?;
    let t = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some((t, m.len()))
}

pub enum ScanMsg {
    Progress { done: usize, total: usize },
    /// `offline`: library folders that could not be reached (a network share that is away)
    /// `network`: library folders that live on another computer
    Done { faces: Vec<(FaceMeta, Origin)>, cache: ScanCache, offline: Vec<PathBuf>, network: Vec<PathBuf> },
}

pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        v.push(win.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        v.push(PathBuf::from("/System/Library/Fonts"));
        v.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join("Library/Fonts"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        v.push(PathBuf::from("/usr/share/fonts"));
        v.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(&h).join(".local/share/fonts"));
            v.push(PathBuf::from(&h).join(".fonts"));
        }
    }
    v
}

/// Scan system font folders plus the user's library folders in the background.
pub fn spawn_scan(folders: Vec<PathBuf>, cache: ScanCache, ctx: egui::Context) -> Receiver<ScanMsg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut files: Vec<(PathBuf, Origin)> = Vec::new();
        let mut seen = BTreeSet::new();
        for dir in system_font_dirs() {
            for e in walkdir::WalkDir::new(&dir).follow_links(false).into_iter().filter_map(|e| e.ok()) {
                let p = e.path();
                let name = e.file_name().to_string_lossy();
                if name.starts_with(crate::activation::LINK_PREFIX) || p.components().any(|c| c.as_os_str() == "FontCraft") {
                    continue;
                }
                if e.file_type().is_file() && is_font_file(p) && seen.insert(p.to_path_buf()) {
                    files.push((p.to_path_buf(), Origin::System));
                }
            }
        }
        let mut offline = vec![];
        let mut network = vec![];
        for dir in &folders {
            let net = crate::netfs::is_network_path(dir);
            if net {
                network.push(dir.clone());
            }
            let ok = if net {
                crate::netfs::reachable(dir, std::time::Duration::from_secs(4))
            } else {
                dir.is_dir()
            };
            if !ok {
                offline.push(dir.clone());
                continue;
            }
            for e in walkdir::WalkDir::new(dir).follow_links(true).into_iter().filter_map(|e| e.ok()) {
                let p = e.path();
                if e.file_type().is_file() && is_font_file(p) && seen.insert(p.to_path_buf()) {
                    files.push((p.to_path_buf(), Origin::Folder(dir.clone())));
                }
            }
        }
        let total = files.len();
        let mut new_cache = ScanCache::new();
        let mut faces = Vec::new();
        for (i, (p, origin)) in files.into_iter().enumerate() {
            let key = p.to_string_lossy().to_string();
            let stamp = file_stamp(&p).unwrap_or((0, 0));
            let entry = match cache.get(&key) {
                Some(c) if c.mtime == stamp.0 && c.size == stamp.1 && c.version == CACHE_VERSION => c.clone(),
                _ => CacheEntry { mtime: stamp.0, size: stamp.1, faces: parse_file(&p), version: CACHE_VERSION },
            };
            for f in &entry.faces {
                faces.push((f.clone(), origin.clone()));
            }
            new_cache.insert(key, entry);
            if i % 25 == 0 {
                let _ = tx.send(ScanMsg::Progress { done: i, total });
                ctx.request_repaint();
            }
        }
        // keep what is known about files in folders that are away, so they are quick when they're back
        for (k, v) in cache {
            if offline.iter().any(|o| Path::new(&k).starts_with(o)) {
                new_cache.entry(k).or_insert(v);
            }
        }
        let _ = tx.send(ScanMsg::Done { faces, cache: new_cache, offline, network });
        ctx.request_repaint();
    });
    rx
}

// ---------------------------------------------------------------- glyphs

pub struct GlyphInfo {
    pub ch: char,
    pub name: Option<String>,
}

pub fn glyph_list(path: &Path, index: u32) -> Vec<GlyphInfo> {
    let Ok(data) = std::fs::read(path) else { return vec![] };
    let Ok(face) = Face::parse(&data, index) else { return vec![] };
    let mut set = BTreeSet::new();
    if let Some(cmap) = face.tables().cmap {
        for st in cmap.subtables {
            if st.is_unicode() {
                st.codepoints(|cp| {
                    set.insert(cp);
                });
            }
        }
        if set.is_empty() {
            for st in cmap.subtables {
                st.codepoints(|cp| {
                    set.insert(cp);
                });
            }
        }
    }
    set.into_iter()
        .filter_map(char::from_u32)
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .map(|ch| GlyphInfo {
            ch,
            name: face.glyph_index(ch).and_then(|g| face.glyph_name(g)).map(|s| s.to_string()),
        })
        .collect()
}

// ---------------------------------------------------------------- variable-font rendering

/// Render one line of text with chosen variable axis values into an image.
pub fn render_line(
    path: &Path,
    index: u32,
    text: &str,
    px: f32,
    variations: &[(String, f32)],
    color: egui::Color32,
) -> Option<egui::ColorImage> {
    use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont, VariableFont};
    let data = std::fs::read(path).ok()?;
    let mut font = FontVec::try_from_vec_and_index(data, index).ok()?;
    for (tag, v) in variations {
        let b = tag.as_bytes();
        if b.len() == 4 {
            font.set_variation(&[b[0], b[1], b[2], b[3]], *v);
        }
    }
    let scale = PxScale::from(px);
    let sf = font.as_scaled(scale);
    let ascent = sf.ascent();
    let height = (sf.ascent() - sf.descent()).ceil().max(1.0);
    let mut x = 2.0f32;
    let mut glyphs = Vec::new();
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
    let w = (x.ceil() as usize + 4).clamp(1, 4000);
    let h = (height as usize + 4).clamp(1, 1000);
    let mut img = egui::ColorImage::new([w, h], egui::Color32::TRANSPARENT);
    for g in glyphs {
        if let Some(og) = font.outline_glyph(g) {
            let b = og.px_bounds();
            og.draw(|gx, gy, cov| {
                let px_ = b.min.x as i32 + gx as i32;
                let py_ = b.min.y as i32 + gy as i32;
                if px_ >= 0 && py_ >= 0 && (px_ as usize) < w && (py_ as usize) < h {
                    let a = (cov.clamp(0.0, 1.0) * 255.0) as u8;
                    let i = py_ as usize * w + px_ as usize;
                    let old = img.pixels[i].a();
                    let a = a.max(old);
                    img.pixels[i] = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a);
                }
            });
        }
    }
    Some(img)
}

/// Friendly names for common OpenType feature tags.
pub fn feature_name(tag: &str) -> &'static str {
    match tag {
        "aalt" => "Access All Alternates",
        "c2sc" => "Small Capitals From Capitals",
        "calt" => "Contextual Alternates",
        "case" => "Case-Sensitive Forms",
        "ccmp" => "Glyph Composition",
        "cpsp" => "Capital Spacing",
        "cv01" | "cv02" | "cv03" | "cv04" | "cv05" | "cv06" | "cv07" | "cv08" | "cv09" => "Character Variant",
        "dlig" => "Discretionary Ligatures",
        "dnom" => "Denominators",
        "frac" => "Fractions",
        "hist" => "Historical Forms",
        "kern" => "Kerning",
        "liga" => "Standard Ligatures",
        "lnum" => "Lining Figures",
        "locl" => "Localized Forms",
        "mark" => "Mark Positioning",
        "mkmk" => "Mark to Mark Positioning",
        "numr" => "Numerators",
        "onum" => "Oldstyle Figures",
        "ordn" => "Ordinals",
        "pnum" => "Proportional Figures",
        "salt" => "Stylistic Alternates",
        "sinf" => "Scientific Inferiors",
        "smcp" => "Small Capitals",
        "subs" => "Subscript",
        "sups" => "Superscript",
        "swsh" => "Swash",
        "titl" => "Titling",
        "tnum" => "Tabular Figures",
        "zero" => "Slashed Zero",
        t if t.starts_with("ss") => "Stylistic Set",
        _ => "",
    }
}

// ---------------------------------------------------------------- Unicode blocks

const BLOCKS: &[(u32, u32, &str)] = &[
    (0x0000, 0x007F, "Basic Latin"),
    (0x0080, 0x00FF, "Latin-1 Supplement"),
    (0x0100, 0x017F, "Latin Extended-A"),
    (0x0180, 0x024F, "Latin Extended-B"),
    (0x0250, 0x02AF, "IPA Extensions"),
    (0x02B0, 0x02FF, "Spacing Modifier Letters"),
    (0x0300, 0x036F, "Combining Diacritical Marks"),
    (0x0370, 0x03FF, "Greek and Coptic"),
    (0x0400, 0x04FF, "Cyrillic"),
    (0x0500, 0x052F, "Cyrillic Supplement"),
    (0x0530, 0x058F, "Armenian"),
    (0x0590, 0x05FF, "Hebrew"),
    (0x0600, 0x06FF, "Arabic"),
    (0x0700, 0x074F, "Syriac"),
    (0x0750, 0x077F, "Arabic Supplement"),
    (0x0780, 0x07BF, "Thaana"),
    (0x07C0, 0x07FF, "NKo"),
    (0x0800, 0x08FF, "Samaritan, Mandaic and Arabic Extended"),
    (0x0900, 0x097F, "Devanagari"),
    (0x0980, 0x09FF, "Bengali"),
    (0x0A00, 0x0A7F, "Gurmukhi"),
    (0x0A80, 0x0AFF, "Gujarati"),
    (0x0B00, 0x0B7F, "Oriya"),
    (0x0B80, 0x0BFF, "Tamil"),
    (0x0C00, 0x0C7F, "Telugu"),
    (0x0C80, 0x0CFF, "Kannada"),
    (0x0D00, 0x0D7F, "Malayalam"),
    (0x0D80, 0x0DFF, "Sinhala"),
    (0x0E00, 0x0E7F, "Thai"),
    (0x0E80, 0x0EFF, "Lao"),
    (0x0F00, 0x0FFF, "Tibetan"),
    (0x1000, 0x109F, "Myanmar"),
    (0x10A0, 0x10FF, "Georgian"),
    (0x1100, 0x11FF, "Hangul Jamo"),
    (0x1200, 0x139F, "Ethiopic"),
    (0x13A0, 0x13FF, "Cherokee"),
    (0x1400, 0x167F, "Unified Canadian Aboriginal Syllabics"),
    (0x1680, 0x16FF, "Ogham and Runic"),
    (0x1700, 0x17FF, "Philippine scripts and Khmer"),
    (0x1800, 0x18AF, "Mongolian"),
    (0x1AB0, 0x1AFF, "Combining Diacritical Marks Extended"),
    (0x1B00, 0x1BFF, "Balinese, Sundanese and Batak"),
    (0x1C80, 0x1CFF, "Cyrillic Extended-C and Vedic"),
    (0x1D00, 0x1DBF, "Phonetic Extensions"),
    (0x1DC0, 0x1DFF, "Combining Diacritical Marks Supplement"),
    (0x1E00, 0x1EFF, "Latin Extended Additional"),
    (0x1F00, 0x1FFF, "Greek Extended"),
    (0x2000, 0x206F, "General Punctuation"),
    (0x2070, 0x209F, "Superscripts and Subscripts"),
    (0x20A0, 0x20CF, "Currency Symbols"),
    (0x20D0, 0x20FF, "Combining Marks for Symbols"),
    (0x2100, 0x214F, "Letterlike Symbols"),
    (0x2150, 0x218F, "Number Forms"),
    (0x2190, 0x21FF, "Arrows"),
    (0x2200, 0x22FF, "Mathematical Operators"),
    (0x2300, 0x23FF, "Miscellaneous Technical"),
    (0x2400, 0x245F, "Control Pictures and OCR"),
    (0x2460, 0x24FF, "Enclosed Alphanumerics"),
    (0x2500, 0x257F, "Box Drawing"),
    (0x2580, 0x259F, "Block Elements"),
    (0x25A0, 0x25FF, "Geometric Shapes"),
    (0x2600, 0x26FF, "Miscellaneous Symbols"),
    (0x2700, 0x27BF, "Dingbats"),
    (0x27C0, 0x27FF, "Mathematical Symbols and Arrows"),
    (0x2800, 0x28FF, "Braille Patterns"),
    (0x2900, 0x2BFF, "Arrows and Symbols"),
    (0x2C00, 0x2C7F, "Glagolitic and Latin Extended-C"),
    (0x2C80, 0x2CFF, "Coptic"),
    (0x2D00, 0x2DFF, "Georgian Supplement, Tifinagh and Ethiopic Extended"),
    (0x2E00, 0x2E7F, "Supplemental Punctuation"),
    (0x2E80, 0x2FFF, "CJK Radicals"),
    (0x3000, 0x303F, "CJK Symbols and Punctuation"),
    (0x3040, 0x309F, "Hiragana"),
    (0x30A0, 0x30FF, "Katakana"),
    (0x3100, 0x31FF, "Bopomofo and Kana Extensions"),
    (0x3200, 0x33FF, "Enclosed CJK Letters and Compatibility"),
    (0x3400, 0x4DBF, "CJK Unified Ideographs Extension A"),
    (0x4DC0, 0x4DFF, "Yijing Hexagram Symbols"),
    (0x4E00, 0x9FFF, "CJK Unified Ideographs"),
    (0xA000, 0xA4CF, "Yi"),
    (0xA4D0, 0xA63F, "Lisu and Vai"),
    (0xA640, 0xA69F, "Cyrillic Extended-B"),
    (0xA700, 0xA71F, "Modifier Tone Letters"),
    (0xA720, 0xA7FF, "Latin Extended-D"),
    (0xA800, 0xAB2F, "Indic and Southeast Asian scripts"),
    (0xAB30, 0xAB6F, "Latin Extended-E"),
    (0xAC00, 0xD7AF, "Hangul Syllables"),
    (0xE000, 0xF8FF, "Private Use Area"),
    (0xF900, 0xFAFF, "CJK Compatibility Ideographs"),
    (0xFB00, 0xFB4F, "Alphabetic Presentation Forms"),
    (0xFB50, 0xFDFF, "Arabic Presentation Forms-A"),
    (0xFE00, 0xFE0F, "Variation Selectors"),
    (0xFE10, 0xFE6F, "Vertical and Small Form Variants"),
    (0xFE70, 0xFEFF, "Arabic Presentation Forms-B"),
    (0xFF00, 0xFFEF, "Halfwidth and Fullwidth Forms"),
    (0xFFF0, 0xFFFF, "Specials"),
    (0x1D400, 0x1D7FF, "Mathematical Alphanumeric Symbols"),
    (0x1F000, 0x1F0FF, "Game Symbols"),
    (0x1F100, 0x1F2FF, "Enclosed Alphanumeric Supplement"),
    (0x1F300, 0x1F5FF, "Miscellaneous Symbols and Pictographs"),
    (0x1F600, 0x1F64F, "Emoticons"),
    (0x1F650, 0x1F6FF, "Transport and Map Symbols"),
    (0x1F700, 0x1FAFF, "Symbols and Pictographs Extended"),
];

/// The Unicode block a character belongs to.
pub fn unicode_block(c: char) -> &'static str {
    let u = c as u32;
    BLOCKS.iter().find(|(a, b, _)| u >= *a && u <= *b).map(|(_, _, n)| *n).unwrap_or("Other")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_shapes_of_installed_fonts() {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let f = |n: &str| win.join("Fonts").join(n);
        let (Some(reg), Some(bold)) = (parse_file(&f("arial.ttf")).pop(), parse_file(&f("arialbd.ttf")).pop()) else { return };
        let (r, b) = (reg.shape.unwrap(), bold.shape.unwrap());
        assert!(b.weight > r.weight, "bold {b:?} regular {r:?}");
        if let Some(times) = parse_file(&f("times.ttf")).pop() {
            assert!(times.shape.unwrap().contrast > r.contrast, "times {:?} arial {:?}", times.shape, r);
            assert_eq!(times.class, "serif");
        }
        assert_eq!(unicode_block('A'), "Basic Latin");
        assert_eq!(unicode_block('€'), "Currency Symbols");
    }
}
