//! Previews in right-to-left and other complex scripts: drawn by `shape.rs` (joined letters,
//! right-to-left order) and kept as pictures, so scrolling stays smooth.

use super::*;
use crate::shape::{self, HAlign};
use egui::{pos2, vec2, Rect};
use std::path::Path;
use std::sync::Arc;

/// A drawn preview: its picture, size in points and where its first baseline is.
struct Drawn {
    tex: egui::TextureHandle,
    size: egui::Vec2,
    baseline: f32,
}

pub struct ShapedText {
    ctx: egui::Context,
    cache: HashMap<u64, Drawn>,
    fonts: HashMap<PathBuf, Arc<Vec<u8>>>,
    /// whether a font file has every letter of a text: (file, index, text) → yes/no
    covered: HashMap<(PathBuf, u32, u64), bool>,
    /// pictures that may still be drawn in this frame (the rest follow in the next ones)
    budget: usize,
}

/// For right-to-left text "left" means where the line starts: on the right.
pub fn mirror(a: config::TextAlign) -> config::TextAlign {
    use config::TextAlign::*;
    match a {
        Left => Right,
        Right => Left,
        JustifyLeft => JustifyRight,
        JustifyRight => JustifyLeft,
        x => x,
    }
}

/// The right-to-left script a family can be shown in: the one it was detected as made for, else
/// Arabic or Hebrew when it has those letters (`scripts`: its `LANGUAGES` bits).
pub fn rtl_script(rtl: &str, scripts: u32) -> Option<&'static str> {
    if let Some((n, _, _)) = shape::RTL_SCRIPTS.iter().find(|(n, _, _)| *n == rtl) {
        return Some(n);
    }
    ["Arabic", "Hebrew"].into_iter().find(|name| {
        crate::library::LANGUAGES.iter().position(|(l, _)| l == name).map(|k| scripts & (1 << k) != 0).unwrap_or(false)
    })
}

/// What a font's preview shows and how it is aligned.
///
/// * Text the person typed in a right-to-left script is always shown as typed (from the right).
/// * Right-to-left switch off: fonts show the sample text with the usual alignment. Only a font
///   that can't show the sample text (no Latin letters) shows its own right-to-left script.
/// * Right-to-left switch on: every font that has a right-to-left script shows it first, from the
///   right; the others show the sample text as it is, but aligned to the right.
#[allow(clippy::too_many_arguments)]
pub fn preview_text(
    shaped: &mut ShapedText,
    rtl: &str,
    scripts: u32,
    file: Option<(&Path, u32)>,
    base: &str,
    align: config::TextAlign,
    rtl_mode: bool,
) -> (String, config::TextAlign) {
    if base.chars().any(shape::is_rtl_char) {
        return (base.to_string(), align);
    }
    let script = rtl_script(rtl, scripts);
    if rtl_mode {
        return match script.and_then(shape::rtl_sample) {
            Some(s) => (s.to_string(), align),
            // the sample text stays left to right, only its place on the line changes
            None => (base.to_string(), mirror(align)),
        };
    }
    if let Some(s) = script.and_then(shape::rtl_sample) {
        let shows_base = file.map(|(p, i)| shaped.covers(p, i, base)).unwrap_or(false);
        if !shows_base {
            return (s.to_string(), align);
        }
    }
    (base.to_string(), align)
}

impl ShapedText {
    pub fn new(ctx: egui::Context) -> Self {
        Self { ctx, cache: HashMap::new(), fonts: HashMap::new(), covered: HashMap::new(), budget: 0 }
    }

    pub fn begin_frame(&mut self) {
        self.budget = 16;
        if self.cache.len() > 900 {
            self.cache.clear();
        }
        if self.fonts.len() > 48 {
            self.fonts.clear();
        }
    }

    fn font(&mut self, path: &Path) -> Option<Arc<Vec<u8>>> {
        if let Some(d) = self.fonts.get(path) {
            return Some(d.clone());
        }
        let d = Arc::new(std::fs::read(path).ok()?);
        self.fonts.insert(path.to_path_buf(), d.clone());
        Some(d)
    }

    /// Does the font file have every letter of the text (spaces and marks aside)? Remembered.
    pub fn covers(&mut self, path: &Path, index: u32, text: &str) -> bool {
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut hs);
            (path.to_path_buf(), index, hs.finish())
        };
        if let Some(v) = self.covered.get(&key) {
            return *v;
        }
        if self.covered.len() > 4000 {
            self.covered.clear();
        }
        let v = self
            .font(path)
            .and_then(|data| ttf_parser::Face::parse(&data, index).ok().map(|face| text.chars().filter(|c| c.is_alphabetic()).all(|c| face.glyph_index(c).is_some())))
            .unwrap_or(false);
        self.covered.insert(key, v);
        v
    }

    /// Draw `text` (which needs shaping) in the font file `path` into `area`, starting at the same
    /// baseline as the other previews. `wrap`: the width to wrap at (tiles). Returns false while the
    /// picture isn't ready yet (the caller shows a placeholder).
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        area: Rect,
        path: &Path,
        index: u32,
        text: &str,
        size: f32,
        color: Color32,
        align: config::TextAlign,
        wrap: Option<f32>,
    ) -> bool {
        let align = if shape::base_rtl(text) { mirror(align) } else { align };
        let h = match align.halign() {
            egui::Align::Min => HAlign::Left,
            egui::Align::Center => HAlign::Center,
            egui::Align::Max => HAlign::Right,
        };
        let ppp = self.ctx.pixels_per_point();
        let max_lines = match wrap {
            Some(_) => ((area.height() - size * 0.2) / (size * 1.3)).floor().max(1.0) as usize,
            None => 1,
        };
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            (path, index, text, size.to_bits(), color, wrap.map(|w| w.to_bits()), h, ppp.to_bits(), max_lines).hash(&mut hs);
            hs.finish()
        };
        if !self.cache.contains_key(&key) {
            if self.budget == 0 {
                self.ctx.request_repaint();
                return false;
            }
            self.budget -= 1;
            let Some(data) = self.font(path) else { return false };
            let Some(pic) = shape::render(&data, index, text, size * ppp, wrap.map(|w| w * ppp), h, max_lines) else { return false };
            let mut rgba = Vec::with_capacity(pic.w * pic.h * 4);
            for a in &pic.alpha {
                let a = ((*a as u32 * color.a() as u32) / 255) as u8;
                rgba.extend_from_slice(&[color.r(), color.g(), color.b(), a]);
            }
            let img = egui::ColorImage::from_rgba_unmultiplied([pic.w, pic.h], &rgba);
            let tex = self.ctx.load_texture(format!("shaped-{key}"), img, egui::TextureOptions::LINEAR);
            self.cache.insert(key, Drawn { tex, size: vec2(pic.w as f32 / ppp, pic.h as f32 / ppp), baseline: pic.baseline / ppp });
        }
        let d = &self.cache[&key];
        let x = match (wrap, h) {
            (Some(_), _) | (None, HAlign::Left) => area.left() - 2.0 / ppp,
            (None, HAlign::Center) => area.center().x - d.size.x / 2.0,
            (None, HAlign::Right) => area.right() - d.size.x + 2.0 / ppp,
        };
        let y = area.top() + size * 0.98 - d.baseline;
        let rect = Rect::from_min_size(pos2(x, y), d.size);
        painter.image(d.tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::TextAlign;

    /// A font with Latin and Arabic letters shows the sample text normally; with the switch on it
    /// shows Arabic first. A Latin-only font with the switch on keeps its text, set from the right.
    #[test]
    fn rtl_switch() {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let tahoma = win.join("Fonts").join("tahoma.ttf");
        if !tahoma.exists() {
            return;
        }
        let mut sh = ShapedText::new(egui::Context::default());
        let base = "Pack my box with five dozen liquor jugs.";
        let file = Some((tahoma.as_path(), 0));
        // a font made for Arabic that also has Latin letters: normal, from the left
        let (t, a) = preview_text(&mut sh, "Arabic", 0, file, base, TextAlign::Left, false);
        assert_eq!((t.as_str(), a), (base, TextAlign::Left));
        // switch on: Arabic first (drawn from the right, as right-to-left text)
        let (t, a) = preview_text(&mut sh, "Arabic", 0, file, base, TextAlign::Left, true);
        assert!(shape::base_rtl(&t));
        assert_eq!(a, TextAlign::Left);
        // a font without a right-to-left script, switch on: same text, aligned right
        let (t, a) = preview_text(&mut sh, "", 0, file, base, TextAlign::Left, true);
        assert_eq!((t.as_str(), a), (base, TextAlign::Right));
        // a font that can't show the sample text shows its own script, also with the switch off
        let (t, _) = preview_text(&mut sh, "Arabic", 0, Some((tahoma.as_path(), 0)), "日本語", TextAlign::Left, false);
        assert!(shape::base_rtl(&t));
        // text typed in Arabic stays as typed
        let (t, _) = preview_text(&mut sh, "", 0, file, "مرحبا", TextAlign::Left, false);
        assert_eq!(t, "مرحبا");
    }
}