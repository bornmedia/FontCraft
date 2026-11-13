//! The Font finder page: give it a picture with text and it lists the fonts from the
//! library, this computer and Google Fonts that look most like it.

use super::list::paint_on_baseline;
use super::ui::paint_dot;
use super::*;
use crate::fontmatch::{self, BoxEdit, BoxEdits, Cand, JobMsg, Sample};
use egui::{pos2, vec2, Align2, FontFamily, FontId, Id, Rect, RichText, Sense, StrokeKind, Ui};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];

pub fn is_image(p: &std::path::Path) -> bool {
    p.extension().map(|e| IMAGE_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str())).unwrap_or(false)
}

/// Fonts that read letters well: common, plain and very different from each other.
const READERS: &[&str] = &[
    "Arial", "Helvetica", "Helvetica Neue", "Times New Roman", "Times", "Georgia", "Verdana", "Calibri", "Cambria", "Segoe UI",
    "Garamond", "Palatino Linotype", "Palatino", "Century Gothic", "Futura", "Gill Sans MT", "Gill Sans", "Trebuchet MS",
    "Courier New", "Rockwell", "Bodoni MT", "Franklin Gothic Medium", "Baskerville", "Avenir", "Roboto", "Open Sans", "Lato",
    "Montserrat", "DejaVu Sans", "DejaVu Serif", "Liberation Sans", "Liberation Serif", "Noto Sans", "Noto Serif", "Impact",
];
const MAX_READERS: usize = 32;
const SHOW: usize = 30;

pub struct Pic {
    gray: image::GrayImage,
    tex: egui::TextureHandle,
    size: [usize; 2],
    name: String,
}

pub struct Finder {
    pic: Option<Pic>,
    crop: [f32; 4],
    drag: Option<egui::Pos2>,
    sample: Option<Arc<Sample>>,
    letters: Vec<char>,
    skip: Vec<bool>,
    typed: Option<String>,
    text: String,
    pub job: Option<(Receiver<JobMsg>, Arc<AtomicBool>)>,
    progress: Option<(usize, usize, String)>,
    results: Vec<(Cand, f32)>,
    searched: bool,
    skip_google: bool,
    error: Option<String>,
    /// a search waiting for the library (or the Google list) to load
    pub waiting: Option<Option<Vec<char>>>,
    /// smaller text was left out at first
    small_left_out: bool,
    /// the person's own cuts and joins of letter boxes (for this picture)
    edits: BoxEdits,
    /// the letter box the right-click menu is open for
    menu_glyph: Option<usize>,
}

impl Default for Finder {
    fn default() -> Self {
        Self {
            pic: None,
            crop: [0.0, 0.0, 1.0, 1.0],
            drag: None,
            sample: None,
            letters: vec![],
            skip: vec![],
            typed: None,
            text: String::new(),
            job: None,
            progress: None,
            results: vec![],
            searched: false,
            skip_google: false,
            error: None,
            waiting: None,
            small_left_out: false,
            edits: BoxEdits::default(),
            menu_glyph: None,
        }
    }
}

/// The letters as text, one word per word in the picture (words left out entirely are hidden).
fn letters_text(s: &Sample, letters: &[char], skip: &[bool]) -> String {
    s.words
        .iter()
        .filter(|w| w.glyphs.clone().any(|g| !skip.get(g).copied().unwrap_or(false)))
        .map(|w| w.glyphs.clone().map(|g| letters.get(g).copied().unwrap_or('?')).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

impl FontCraftApp {
    // -------------------------------------------------------------- loading pictures

    pub(super) fn finder_open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .set_title("Choose a picture with text")
            .add_filter("Pictures", IMAGE_EXTS)
            .pick_file()
        {
            self.finder_load_path(p);
        }
    }

    pub(super) fn finder_load_path(&mut self, p: PathBuf) {
        self.view = View::Finder;
        match image::open(&p) {
            Ok(img) => {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.finder_set_image(img, name);
            }
            Err(e) => self.finder.error = Some(format!("This picture could not be opened: {e}")),
        }
    }

    pub(super) fn finder_paste(&mut self) {
        let got = arboard::Clipboard::new().ok().and_then(|mut c| c.get_image().ok());
        match got {
            Some(d) => {
                let img = image::RgbaImage::from_raw(d.width as u32, d.height as u32, d.bytes.into_owned());
                match img {
                    Some(img) => {
                        self.view = View::Finder;
                        self.finder_set_image(image::DynamicImage::ImageRgba8(img), "Pasted picture".into());
                    }
                    None => self.finder.error = Some("The picture on the clipboard could not be read.".into()),
                }
            }
            None => self.set_status("There is no picture on the clipboard – copy a screenshot or an image first"),
        }
    }

    fn finder_set_image(&mut self, img: image::DynamicImage, name: String) {
        // transparent pictures (logos) are judged on white
        let mut rgba = img.to_rgba8();
        for p in rgba.pixels_mut() {
            let a = p.0[3] as f32 / 255.0;
            for c in 0..3 {
                p.0[c] = (p.0[c] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
            }
            p.0[3] = 255;
        }
        let (w, h) = rgba.dimensions();
        let longest = w.max(h) as f32;
        let analysis = if longest > 2600.0 {
            let k = 2600.0 / longest;
            image::imageops::resize(&rgba, (w as f32 * k) as u32, (h as f32 * k) as u32, image::imageops::FilterType::Triangle)
        } else {
            rgba.clone()
        };
        let gray = image::DynamicImage::ImageRgba8(analysis).to_luma8();
        let shown = if longest > 1600.0 {
            let k = 1600.0 / longest;
            image::imageops::resize(&rgba, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1), image::imageops::FilterType::Triangle)
        } else {
            rgba
        };
        let size = [shown.width() as usize, shown.height() as usize];
        let tex = self.ctx.load_texture("finder-picture", egui::ColorImage::from_rgba_unmultiplied(size, shown.as_raw()), egui::TextureOptions::LINEAR);
        self.finder.pic = Some(Pic { gray, tex, size, name });
        self.finder.error = None;
        self.finder.crop = [0.0, 0.0, 1.0, 1.0];
        self.finder.edits = BoxEdits::default();
        self.finder_analyze();
    }

    /// Find the letter boxes again after the person cut or joined one. What they left
    /// out and what they typed stays.
    fn finder_edit_boxes(&mut self, e: Option<BoxEdit>) {
        match e {
            Some(e) => self.finder.edits.0.push(e),
            None => self.finder.edits = BoxEdits::default(),
        }
        let (Some(pic), Some(old)) = (&self.finder.pic, self.finder.sample.clone()) else { return };
        let s = fontmatch::analyze_with(&pic.gray, self.finder.crop, &self.finder.edits);
        let skip: Vec<bool> = s
            .glyphs
            .iter()
            .map(|g| {
                let (cx, cy) = ((g.rect[0] + g.rect[2]) / 2.0, (g.rect[1] + g.rect[3]) / 2.0);
                match old.glyphs.iter().position(|o| cx >= o.rect[0] && cx <= o.rect[2] && cy >= o.rect[1] && cy <= o.rect[3]) {
                    // a joined-up box that was left out by itself: its pieces are wanted now
                    Some(i) if old.glyphs[i].wide && !g.wide => false,
                    Some(i) => self.finder.skip.get(i).copied().unwrap_or(g.wide),
                    None => g.wide,
                }
            })
            .collect();
        self.finder.skip = skip;
        self.finder.letters = vec!['?'; s.glyphs.len()];
        if self.finder.typed.is_none() {
            self.finder.text = letters_text(&s, &self.finder.letters, &self.finder.skip);
        }
        self.finder.results.clear();
        self.finder.sample = Some(Arc::new(s));
        self.finder_start(None);
    }

    fn finder_analyze(&mut self) {
        let Some(pic) = &self.finder.pic else { return };
        let s = fontmatch::analyze_with(&pic.gray, self.finder.crop, &self.finder.edits);
        // Text of very different sizes is usually different fonts (a headline and its
        // small print): start with the biggest text, the rest can be clicked back in.
        let mut line_h: HashMap<usize, f32> = HashMap::new();
        for g in &s.glyphs {
            let e = line_h.entry(g.line).or_insert(0.0);
            *e = e.max(g.rect[3] - g.rect[1]);
        }
        let biggest = line_h.values().copied().fold(0.0f32, f32::max);
        self.finder.skip = s.glyphs.iter().map(|g| g.wide || line_h[&g.line] < 0.65 * biggest).collect();
        self.finder.small_left_out = s.glyphs.iter().any(|g| line_h[&g.line] < 0.65 * biggest);
        self.finder.letters = vec!['?'; s.glyphs.len()];
        self.finder.text = letters_text(&s, &self.finder.letters, &self.finder.skip);
        self.finder.typed = None;
        self.finder.sample = Some(Arc::new(s));
        self.finder.results.clear();
        self.finder_start(None);
    }

    // -------------------------------------------------------------- searching

    fn finder_cancel(&mut self) {
        if let Some((_, c)) = self.finder.job.take() {
            c.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.finder.progress = None;
    }

    fn finder_start(&mut self, letters: Option<Vec<char>>) {
        self.finder_cancel();
        if (self.scan_rx.is_some() && self.faces.is_empty()) || self.gstates[0] == GState::Loading {
            self.finder.waiting = Some(letters);
            self.finder.progress = Some((0, 0, "Waiting for your fonts to load…".into()));
            return;
        }
        self.finder.waiting = None;
        let Some(sample) = self.finder.sample.clone() else { return };
        if sample.glyphs.is_empty() {
            self.finder.results.clear();
            self.finder.searched = true;
            return;
        }
        // every face once, sorted by file so collections of fonts are read once
        let mut seen = HashSet::new();
        let mut local: Vec<Cand> = self
            .faces
            .iter()
            .enumerate()
            .filter(|(_, f)| seen.insert((f.path_key.clone(), f.meta.index)))
            .map(|(i, f)| Cand {
                path: f.meta.path.clone(),
                index: f.meta.index,
                family: self.families[self.face_fam[i]].name.clone(),
                style: f.meta.style.clone(),
                google: false,
                id: f.id,
            })
            .collect();
        local.sort_by(|a, b| a.path.cmp(&b.path).then(a.index.cmp(&b.index)));
        // fonts that read the letters
        let mut readers = vec![];
        for name in READERS {
            if readers.len() >= MAX_READERS {
                break;
            }
            let lname = name.to_lowercase();
            if let Some(fam) = self.families.iter().find(|f| f.lower == lname) {
                // the regular style and, for heavy lettering, the boldest upright one
                let bold = fam.faces.iter().copied().filter(|f| !self.faces[*f].meta.italic && self.faces[*f].meta.weight >= 600).max_by_key(|f| self.faces[*f].meta.weight);
                for fi in std::iter::once(fam.primary).chain(bold) {
                    let pf = &self.faces[fi];
                    if let Some(i) = local.iter().position(|c| c.path == pf.meta.path && c.index == pf.meta.index) {
                        readers.push(i);
                    }
                }
            }
        }
        if readers.len() < 6 {
            // a computer without the usual fonts: take a spread of regular styles
            let step = (self.families.len() / 12).max(1);
            for fam in self.families.iter().step_by(step) {
                let pf = &self.faces[fam.primary];
                if let Some(i) = local.iter().position(|c| c.path == pf.meta.path && c.index == pf.meta.index) {
                    readers.push(i);
                }
                if readers.len() >= 12 {
                    break;
                }
            }
        }
        // Google Fonts letter sets
        let (mut gready, mut gmissing) = (vec![], vec![]);
        if !self.finder.skip_google && self.gstates[0] == GState::Ready {
            let online = self.shot.is_none();
            for g in self.gcat.iter().filter(|g| g.provider == Provider::Google) {
                // emoji and icon families have no letters to compare (and are huge)
                if !self.gfaces(&g.name).is_empty() || self.lib_names.contains(&g.name) || g.name.contains("Emoji") || g.name.contains("Icons") || g.name.contains("Symbols") {
                    continue;
                }
                match google::letter_set_state(&g.name) {
                    Some(true) => gready.push(google::letter_set_cand(&g.name, google::letter_set_path(&g.name))),
                    Some(false) => {}
                    None if online => gmissing.push((g.name.clone(), google::letter_spec(g))),
                    None => {}
                }
            }
            // the most used families first, so good matches show up early
            let pop: HashMap<&str, u32> = self.gcat.iter().filter(|g| g.provider == Provider::Google).map(|g| (g.name.as_str(), g.popularity)).collect();
            gmissing.sort_by_key(|(n, _)| pop.get(n.as_str()).copied().unwrap_or(u32::MAX));
        }
        let input = fontmatch::JobInput {
            sample,
            letters,
            skip: self.finder.skip.clone(),
            typed: self.finder.typed.clone(),
            local,
            readers,
            google_ready: gready,
            google_missing: gmissing,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let rx = fontmatch::spawn(input, cancel.clone(), self.ctx.clone());
        self.finder.job = Some((rx, cancel));
        self.finder.progress = Some((0, 0, "Starting…".into()));
        self.finder.searched = true;
    }

    /// Search again with what is known now (typed text, letters, left-out letters).
    fn finder_search_again(&mut self) {
        let Some(s) = self.finder.sample.clone() else { return };
        let auto_text = letters_text(&s, &self.finder.letters, &self.finder.skip);
        let t = self.finder.text.trim().to_string();
        if t.is_empty() {
            self.finder.typed = None;
            self.finder_start(None);
        } else if t == auto_text {
            // nothing typed: use the letters read so far, or read again where some are unknown
            self.finder.typed = None;
            let known = s.glyphs.iter().enumerate().all(|(g, gl)| self.finder.skip[g] || gl.wide || self.finder.letters.get(g).map(|c| *c != '?').unwrap_or(false));
            if known {
                let l = self.finder.letters.clone();
                self.finder_start(Some(l));
            } else {
                self.finder_start(None);
            }
        } else {
            self.finder.typed = Some(t);
            self.finder_start(None);
        }
    }

    pub(super) fn poll_finder(&mut self) {
        if self.finder.waiting.is_some() && !(self.scan_rx.is_some() && self.faces.is_empty()) && self.gstates[0] != GState::Loading {
            let l = self.finder.waiting.take().flatten();
            self.finder_start(l);
        }
        let mut done = false;
        if let Some((rx, _)) = &self.finder.job {
            while let Ok(m) = rx.try_recv() {
                match m {
                    JobMsg::Letters(l) => {
                        if let Some(s) = &self.finder.sample {
                            if self.finder.typed.is_none() {
                                self.finder.text = letters_text(s, &l, &self.finder.skip);
                            }
                        }
                        self.finder.letters = l;
                    }
                    JobMsg::Progress { done, total, what } => self.finder.progress = Some((done, total, what)),
                    JobMsg::Results(r) => self.finder.results = r,
                    JobMsg::Done => done = true,
                }
            }
        }
        if done {
            self.finder.job = None;
            self.finder.progress = None;
        }
    }

    // -------------------------------------------------------------- drawing

    pub(super) fn finder_central(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let p = self.p();
        // Ctrl+V pastes a picture when no text field is being edited
        let typing = ctx.memory(|m| m.focused().is_some());
        let paste = ctx.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                || (i.modifiers.command && i.key_pressed(egui::Key::V))
        });
        if paste && !typing {
            self.finder_paste();
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.content).inner_margin(egui::Margin { left: 22, right: 18, top: 14, bottom: 0 }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Font finder").size(21.0).strong().color(p.text));
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Find the font in a picture – compared with your own fonts and Google Fonts, right on this computer.")
                            .size(12.5)
                            .color(p.weak),
                    );
                });
                ui.add_space(10.0);
                if let Some(e) = self.finder.error.clone() {
                    ui.label(RichText::new(e).color(p.heart));
                }
                if self.finder.pic.is_none() {
                    self.finder_dropzone(ui, &p);
                    return;
                }
                egui::ScrollArea::vertical().id_salt("finderscroll").auto_shrink([false, false]).show(ui, |ui| {
                    let avail = ui.available_width();
                    ui.horizontal_top(|ui| {
                        let img_w = (avail * 0.56).clamp(260.0, 760.0);
                        self.finder_picture(ui, &p, img_w);
                        ui.add_space(16.0);
                        ui.vertical(|ui| {
                            ui.set_width((avail - img_w - 24.0).max(220.0));
                            self.finder_controls(ui, &p);
                        });
                    });
                    ui.add_space(14.0);
                    self.finder_results(ui, &p, acts);
                    ui.add_space(20.0);
                });
            });
    }

    fn finder_dropzone(&mut self, ui: &mut Ui, p: &Pal) {
        let w = ui.available_width().min(760.0);
        let mut open = false;
        let mut paste = false;
        let inner = egui::Frame::new().fill(p.card).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::symmetric(20, 26)).show(ui, |ui| {
            ui.set_width(w - 40.0);
            ui.vertical_centered(|ui| {
                // a little picture-with-text symbol
                let (ic, _) = ui.allocate_exact_size(vec2(64.0, 46.0), Sense::hover());
                ui.painter().rect_stroke(ic, CornerRadius::same(6), Stroke::new(1.6_f32, p.accent), StrokeKind::Middle);
                ui.painter().text(ic.center(), Align2::CENTER_CENTER, "Aa", FontId::new(22.0, FontFamily::Proportional), p.accent);
                ui.add_space(12.0);
                ui.label(RichText::new("Drop a picture with text here").size(18.0).color(p.text));
                ui.label(RichText::new("a screenshot, a photo of a poster, a logo… (PNG, JPG, WebP, GIF, BMP, TIFF)").size(12.5).color(p.weak));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 300.0).max(0.0) / 2.0);
                    let b = egui::Button::new(RichText::new("Open picture…").color(Color32::BLACK)).fill(p.accent).corner_radius(CornerRadius::same(14));
                    if ui.add(b).clicked() {
                        open = true;
                    }
                    if ui.button("Paste picture (Ctrl+V)").clicked() {
                        paste = true;
                    }
                });
            });
        });
        // dashed frame around it
        let rect = inner.response.rect;
        let painter = ui.painter();
        let r = rect.shrink(6.0);
        let dash = |a: egui::Pos2, b: egui::Pos2| {
            let n = (a.distance(b) / 12.0) as usize;
            for k in 0..n {
                let t0 = k as f32 / n as f32;
                let t1 = (k as f32 + 0.55) / n as f32;
                painter.line_segment([a.lerp(b, t0), a.lerp(b, t1)], Stroke::new(1.3_f32, p.weak.gamma_multiply(0.6)));
            }
        };
        dash(r.left_top(), r.right_top());
        dash(r.right_top(), r.right_bottom());
        dash(r.right_bottom(), r.left_bottom());
        dash(r.left_bottom(), r.left_top());
        if open {
            self.finder_open_dialog();
        }
        if paste {
            self.finder_paste();
        }
        ui.add_space(14.0);
        ui.label(RichText::new("How it works").strong().color(p.text));
        for line in [
            "1.  FontCraft finds the letters in the picture and reads them.",
            "2.  Drag a box around the text you care about; click a letter to leave it out, right-click it to cut touching letters apart.",
            "3.  Fix the letters if they were misread – the more letters you confirm, the better the match.",
            "4.  Every font in your library, on this computer and on Google Fonts is compared, closest first.",
        ] {
            ui.label(RichText::new(line).size(12.5).color(p.weak));
        }
    }

    fn finder_picture(&mut self, ui: &mut Ui, p: &Pal, max_w: f32) {
        let Some((tex_id, [w, h])) = self.finder.pic.as_ref().map(|p| (p.tex.id(), p.size)) else { return };
        let k = (max_w / w as f32).min(320.0 / h as f32).min(3.0);
        let size = vec2(w as f32 * k, h as f32 * k);
        ui.vertical(|ui| {
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let painter = ui.painter_at(rect.expand(2.0));
            painter.rect_filled(rect, CornerRadius::ZERO, Color32::WHITE);
            painter.image(tex_id, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            let to_screen = |x: f32, y: f32| pos2(rect.left() + x * rect.width(), rect.top() + y * rect.height());
            let to_norm = |pt: egui::Pos2| pos2(((pt.x - rect.left()) / rect.width()).clamp(0.0, 1.0), ((pt.y - rect.top()) / rect.height()).clamp(0.0, 1.0));
            // the chosen part, with the rest dimmed
            let c = self.finder.crop;
            let cr = Rect::from_min_max(to_screen(c[0], c[1]), to_screen(c[2], c[3]));
            let shade = Color32::from_black_alpha(130);
            if c != [0.0, 0.0, 1.0, 1.0] {
                painter.rect_filled(Rect::from_min_max(rect.min, pos2(rect.max.x, cr.min.y)), CornerRadius::ZERO, shade);
                painter.rect_filled(Rect::from_min_max(pos2(rect.min.x, cr.max.y), rect.max), CornerRadius::ZERO, shade);
                painter.rect_filled(Rect::from_min_max(pos2(rect.min.x, cr.min.y), pos2(cr.min.x, cr.max.y)), CornerRadius::ZERO, shade);
                painter.rect_filled(Rect::from_min_max(pos2(cr.max.x, cr.min.y), pos2(rect.max.x, cr.max.y)), CornerRadius::ZERO, shade);
                painter.rect_stroke(cr, CornerRadius::ZERO, Stroke::new(1.5_f32, p.accent), StrokeKind::Outside);
            }
            // letters
            let mut clicked_glyph = None;
            let mut hovered_glyph = None;
            let hover = resp.hover_pos();
            if let Some(s) = &self.finder.sample {
                for (gi, g) in s.glyphs.iter().enumerate() {
                    let gr = Rect::from_min_max(to_screen(g.rect[0], g.rect[1]), to_screen(g.rect[2], g.rect[3])).expand(1.5);
                    let off = self.finder.skip.get(gi).copied().unwrap_or(false);
                    let hov = hover.map(|h| gr.contains(h)).unwrap_or(false) && self.finder.drag.is_none();
                    let col = if off { Color32::from_gray(150) } else { p.accent };
                    if hov {
                        painter.rect_filled(gr, CornerRadius::same(2), col.gamma_multiply(0.25));
                    }
                    painter.rect_stroke(gr, CornerRadius::same(2), Stroke::new(if hov { 2.0 } else { 1.2_f32 }, col), StrokeKind::Outside);
                    if off {
                        painter.line_segment([gr.left_top(), gr.right_bottom()], Stroke::new(1.0_f32, col));
                    }
                    let ch = self.finder.letters.get(gi).copied().unwrap_or('?');
                    if !off && ch != '?' {
                        let lr = Rect::from_min_size(pos2(gr.left(), gr.bottom() + 2.0), vec2(14.0, 14.0));
                        painter.rect_filled(lr, CornerRadius::same(3), p.accent);
                        painter.text(lr.center(), Align2::CENTER_CENTER, ch.to_string(), FontId::proportional(10.5), Color32::BLACK);
                    }
                    if hov {
                        hovered_glyph = Some(gi);
                    }
                    if hov && resp.clicked() {
                        clicked_glyph = Some(gi);
                    }
                    if self.finder.menu_glyph == Some(gi) && ui.ctx().is_context_menu_open() {
                        painter.rect_stroke(gr.expand(2.0), CornerRadius::same(3), Stroke::new(2.0_f32, p.text), StrokeKind::Outside);
                    }
                }
            }
            // drag a box to choose the text
            if resp.drag_started() {
                self.finder.drag = resp.interact_pointer_pos().map(to_norm);
            }
            if let (Some(a), Some(b)) = (self.finder.drag, resp.interact_pointer_pos().map(to_norm)) {
                let r = Rect::from_two_pos(to_screen(a.x, a.y), to_screen(b.x, b.y));
                painter.rect_filled(r, CornerRadius::ZERO, p.accent.gamma_multiply(0.15));
                painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.5_f32, p.accent), StrokeKind::Middle);
                if resp.drag_stopped() {
                    self.finder.drag = None;
                    let n = Rect::from_two_pos(a, b);
                    if n.width() > 0.01 && n.height() > 0.01 {
                        self.finder.crop = [n.min.x, n.min.y, n.max.x, n.max.y];
                        self.finder_analyze();
                    }
                }
            } else if resp.drag_stopped() {
                self.finder.drag = None;
            }
            // right-click a letter box: cut it into letters, join it with the next one
            if resp.secondary_clicked() {
                self.finder.menu_glyph = hovered_glyph;
            }
            let mut menu_act: Option<Option<BoxEdit>> = None;
            let mut toggle = None;
            let info = self.finder.sample.as_ref().zip(self.finder.menu_glyph).and_then(|(s, gi)| {
                let g = s.glyphs.get(gi)?;
                let mid = |r: [f32; 4]| [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0];
                let next = s.glyphs.get(gi + 1).filter(|n| n.line == g.line).map(|n| mid(n.rect));
                Some((gi, mid(g.rect), next, self.finder.skip.get(gi).copied().unwrap_or(false)))
            });
            let has_edits = !self.finder.edits.0.is_empty();
            resp.context_menu(|ui| {
                ui.set_min_width(210.0);
                match info {
                    Some((gi, at, next, off)) => {
                        ui.label(RichText::new("Several letters in this box?").size(11.5).color(p.weak));
                        for n in 2..=4 {
                            if ui.button(format!("Cut into {n} letters")).clicked() {
                                menu_act = Some(Some(BoxEdit::Split(at, n)));
                                ui.close_menu();
                            }
                        }
                        ui.menu_button("Cut into more letters", |ui| {
                            for n in 5..=12 {
                                if ui.button(format!("{n} letters")).clicked() {
                                    menu_act = Some(Some(BoxEdit::Split(at, n)));
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.separator();
                        if let Some(nx) = next {
                            if ui.button("Join with the letter on the right").on_hover_text("For a letter that was cut in two").clicked() {
                                menu_act = Some(Some(BoxEdit::Join(at, nx)));
                                ui.close_menu();
                            }
                        }
                        if ui.button(if off { "Take this letter back in" } else { "Leave this letter out" }).clicked() {
                            toggle = Some(gi);
                            ui.close_menu();
                        }
                    }
                    None => {
                        ui.label(RichText::new("Right-click a letter box to cut it into letters or join it with the next one.").size(12.0));
                    }
                }
                if has_edits {
                    ui.separator();
                    if ui.button("Undo all my cuts and joins").clicked() {
                        menu_act = Some(None);
                        ui.close_menu();
                    }
                }
            });
            if let Some(gi) = toggle {
                clicked_glyph = Some(gi);
            }
            if let Some(gi) = clicked_glyph {
                if let Some(v) = self.finder.skip.get_mut(gi) {
                    *v = !*v;
                }
                if self.finder.typed.is_none() {
                    if let Some(s) = &self.finder.sample {
                        self.finder.text = letters_text(s, &self.finder.letters, &self.finder.skip);
                    }
                }
                self.finder_search_again();
            }
            if let Some(e) = menu_act {
                self.finder_edit_boxes(e);
            }
            let resp = resp.on_hover_text("Drag a box around the text to compare. Click a letter to leave it out (or take it back in). Right-click a letter to cut it apart or join it.");
            let _ = resp;
            let name = self.finder.pic.as_ref().map(|p| p.name.clone()).unwrap_or_default();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).size(11.5).color(p.weak));
                if self.finder.crop != [0.0, 0.0, 1.0, 1.0] && ui.small_button("Use the whole picture").clicked() {
                    self.finder.crop = [0.0, 0.0, 1.0, 1.0];
                    self.finder_analyze();
                }
            });
        });
    }

    fn finder_controls(&mut self, ui: &mut Ui, p: &Pal) {
        let Some(s) = self.finder.sample.clone() else { return };
        ui.label(RichText::new("LETTERS IN THE PICTURE").size(11.0).strong().color(p.weak));
        if s.glyphs.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new("No letters were found here.").color(p.text));
            ui.label(
                RichText::new("Drag a box tightly around the text, or try a sharper picture with more contrast.").size(12.0).color(p.weak),
            );
        } else {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.finder.text)
                    .font(FontId::proportional(18.0))
                    .desired_width(ui.available_width())
                    .hint_text("Type the text you see"),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let n = s.glyphs.len();
            let typed_n = self.finder.text.chars().filter(|c| !c.is_whitespace()).count();
            let left_out = self.finder.skip.iter().filter(|x| **x).count();
            let mut info = format!("{n} letter{} in {} word{}", if n == 1 { "" } else { "s" }, s.words.len(), if s.words.len() == 1 { "" } else { "s" });
            if left_out > 0 {
                info.push_str(&format!(" · {left_out} left out"));
            }
            ui.label(RichText::new(info).size(11.5).color(p.weak));
            let joined = s.glyphs.iter().filter(|g| g.wide).count();
            if self.finder.typed.is_none() && joined * 10 >= n * 3 {
                ui.label(
                    RichText::new("The letters are joined up (script). Type the text exactly as it appears and press Enter – then whole words are compared and the match gets far better.")
                        .size(12.0)
                        .color(p.accent),
                );
            } else if joined > 0 {
                ui.label(
                    RichText::new("Some boxes hold several letters and are left out. Right-click one to cut it into single letters.")
                        .size(11.5)
                        .color(p.weak),
                );
            }
            if self.finder.small_left_out {
                ui.label(
                    RichText::new("Smaller text is left out at first, as it is often another font. Click its letters to add them, or drag a box around it to compare it on its own.")
                        .size(11.5)
                        .color(p.weak),
                );
            }
            let n_active = n - left_out;
            if self.finder.typed.is_some() && typed_n != n_active && !self.finder.text.trim().is_empty() {
                ui.label(
                    RichText::new(format!(
                        "You typed {typed_n} letters for {n_active} shapes: words that don't line up are compared as whole words (good for joined-up script)."
                    ))
                    .size(11.5)
                    .color(p.weak),
                );
            } else {
                ui.label(
                    RichText::new("Misread? Type the letters you see and press Enter – checked letters give much better matches.")
                        .size(11.5)
                        .color(p.weak),
                );
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let b = egui::Button::new(RichText::new("Find matching fonts").color(Color32::BLACK)).fill(p.accent).corner_radius(CornerRadius::same(12));
                if ui.add(b).clicked() || enter {
                    self.finder_search_again();
                }
                if self.finder.typed.is_some() && ui.button("Read the letters again").clicked() {
                    self.finder.text.clear();
                    self.finder_search_again();
                }
            });
        }
        ui.add_space(10.0);
        let gtxt = match &self.gstates[0] {
            GState::Ready => format!("Also compare Google Fonts ({} families)", self.gcat.iter().filter(|g| g.provider == Provider::Google).count()),
            GState::Loading => "Also compare Google Fonts (list is loading…)".to_string(),
            GState::Failed(_) => "Also compare Google Fonts (not reachable right now)".to_string(),
        };
        let mut inc = !self.finder.skip_google;
        if ui
            .checkbox(&mut inc, RichText::new(gtxt).size(12.5))
            .on_hover_text("The first time, FontCraft fetches just the letters of each Google family (a few KB each). Your picture is never uploaded.")
            .changed()
        {
            self.finder.skip_google = !inc;
            self.finder_search_again();
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open another picture…").clicked() {
                self.finder_open_dialog();
            }
            if ui.button("Paste").on_hover_text("Paste a picture from the clipboard (Ctrl+V)").clicked() {
                self.finder_paste();
            }
            if ui.button("Close picture").clicked() {
                self.finder_cancel();
                self.finder = Default::default();
            }
        });
        if let Some((done, total, what)) = self.finder.progress.clone() {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(what).size(12.0).color(p.text));
            });
            if total > 0 {
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_width(ui.available_width()).text(format!("{done} / {total}")));
            }
            if ui.small_button("Stop").clicked() {
                self.finder_cancel();
            }
        }
    }

    fn finder_results(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        let Some(s) = self.finder.sample.clone() else { return };
        if s.glyphs.is_empty() {
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("BEST MATCHES").size(11.0).strong().color(p.weak));
            if !self.finder.results.is_empty() {
                ui.label(RichText::new("closest first · click one to see all its styles").size(11.0).color(p.weak));
            }
        });
        ui.add_space(4.0);
        let preview_text = {
            let t = match &self.finder.typed {
                Some(t) => t.clone(),
                None => letters_text(&s, &self.finder.letters, &self.finder.skip).replace('?', ""),
            };
            if t.trim().is_empty() {
                "Handgloves".to_string()
            } else {
                t
            }
        };
        let size = 34.0f32;
        // the picture itself, for comparing by eye
        if let Some(pic) = &self.finder.pic {
            let c = self.finder.crop;
            // tighten to the letters
            let (mut x0, mut y0, mut x1, mut y1) = (1.0f32, 1.0f32, 0.0f32, 0.0f32);
            for g in &s.glyphs {
                x0 = x0.min(g.rect[0]);
                y0 = y0.min(g.rect[1]);
                x1 = x1.max(g.rect[2]);
                y1 = y1.max(g.rect[3]);
            }
            let uv = if x1 > x0 && y1 > y0 {
                Rect::from_min_max(pos2(x0, y0), pos2(x1, y1)).expand(0.004)
            } else {
                Rect::from_min_max(pos2(c[0], c[1]), pos2(c[2], c[3]))
            };
            let [w, h] = pic.size;
            let ar = (uv.width() * w as f32) / (uv.height() * h as f32).max(1.0);
            let hh = 46.0f32.min(ui.available_width() / ar.max(0.01));
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), hh + 26.0), Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, CornerRadius::same(8), p.card);
            painter.text(pos2(rect.left() + 14.0, rect.top() + 12.0), Align2::LEFT_CENTER, "Your picture", FontId::proportional(11.5), p.weak);
            let ir = Rect::from_min_size(pos2(rect.left() + 14.0, rect.top() + 22.0), vec2(hh * ar, hh - 2.0));
            painter.image(pic.tex.id(), ir, uv, Color32::WHITE);
        }
        ui.add_space(6.0);
        // best style per family
        let mut seen = HashSet::new();
        let rows: Vec<(Cand, f32)> =
            self.finder.results.iter().filter(|(c, _)| seen.insert((c.google, c.family.clone()))).take(SHOW).cloned().collect();
        if rows.is_empty() {
            if self.finder.job.is_none() && self.finder.searched {
                ui.label(RichText::new("No fonts to compare yet – add fonts to your library or include Google Fonts.").color(p.weak));
            }
            return;
        }
        for (rank, (c, score)) in rows.iter().enumerate() {
            let row_h = 30.0 + size * 1.3;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
            let fi = if c.google { None } else { self.face_by_id.get(&c.id).copied().map(|f| self.face_fam[f]) };
            let gi = if c.google { self.gcat.iter().position(|g| g.provider == Provider::Google && g.name == c.family) } else { None };
            let selected = fi.map(|f| self.primary == Some(f)).unwrap_or(false) || (gi.is_some() && self.gsel == gi && self.primary.is_none());
            let painter = ui.painter_at(rect);
            if selected {
                painter.rect_filled(rect, CornerRadius::same(8), p.sel);
            } else if resp.hovered() {
                painter.rect_filled(rect, CornerRadius::same(8), p.hover);
            }
            let hy = rect.top() + 14.0;
            let x0 = rect.left() + 10.0;
            // rank and match
            painter.text(pos2(x0, hy), Align2::LEFT_CENTER, format!("{}", rank + 1), FontId::proportional(11.0), p.weak);
            let pct = (score * 100.0).round().clamp(0.0, 100.0);
            let pr = painter.text(pos2(x0 + 22.0, hy), Align2::LEFT_CENTER, format!("{pct:.0}%"), FontId::proportional(13.0), p.accent);
            let bar = Rect::from_min_size(pos2(pr.right() + 6.0, hy - 2.0), vec2(40.0, 4.0));
            painter.rect_filled(bar, CornerRadius::same(2), p.toggle_off);
            let fillw = 40.0 * ((score - 0.4) / 0.6).clamp(0.02, 1.0);
            painter.rect_filled(Rect::from_min_size(bar.min, vec2(fillw, 4.0)), CornerRadius::same(2), p.accent);
            // name, style, where it comes from
            let mut nx = bar.right() + 12.0;
            if let Some(f) = fi {
                let state = self.fam_state(f);
                let system = self.families[f].system;
                let dc = pos2(nx + 4.0, hy);
                if system {
                    paint_dot(&painter, dc, (1, 1), p, 4.0);
                } else {
                    let dresp = ui.interact(Rect::from_center_size(dc, vec2(16.0, 16.0)), Id::new(("fdot", c.id)), Sense::click());
                    paint_dot(&painter, dc, state, p, if dresp.hovered() { 4.6 } else { 4.0 });
                    let on = state.0 == state.1 && state.1 > 0;
                    if dresp.on_hover_text(if on { "Active in all apps – click to switch off" } else { "Click to activate (Word, Photoshop, Affinity…)" }).clicked() {
                        acts.push(Action::SetFamilies(vec![f], !on));
                    }
                }
                nx += 15.0;
            }
            let nr = painter.text(pos2(nx, hy), Align2::LEFT_CENTER, &c.family, FontId::proportional(12.5), p.text);
            let mut after = nr.right() + 8.0;
            if !c.style.is_empty() {
                let sr = painter.text(pos2(after, hy), Align2::LEFT_CENTER, &c.style, FontId::proportional(11.5), p.weak);
                after = sr.right() + 8.0;
            }
            let src = match fi {
                Some(f) if self.families[f].system => "installed on this computer".to_string(),
                Some(f) => {
                    let (a, n) = self.fam_state(f);
                    if a == n && n > 0 { "in your library · active".into() } else { "in your library".into() }
                }
                None if c.google => "Google Fonts · free".into(),
                None => String::new(),
            };
            painter.text(pos2(after, hy), Align2::LEFT_CENTER, src, FontId::proportional(11.0), p.weak.gamma_multiply(0.85));
            // preview in the font
            let prev = Rect::from_min_max(pos2(x0 + 20.0, rect.top() + 26.0), pos2(rect.right() - 150.0, rect.bottom()));
            let pp = ui.painter_at(prev);
            match self.loader.family(c.id, &c.path, c.index) {
                Some(ff) => paint_on_baseline(&pp, prev, &preview_text, FontId::new(size, ff), p.text, config::TextAlign::Left, None),
                None => {
                    let w = (prev.width() * 0.5).min(size * preview_text.chars().count() as f32 * 0.45);
                    pp.rect_filled(Rect::from_min_size(prev.min + vec2(2.0, size * 0.2), vec2(w, size * 0.75)), CornerRadius::same(4), p.skeleton);
                }
            }
            // what you can do with it
            let br = Rect::from_min_max(pos2(rect.right() - 146.0, rect.top() + 4.0), pos2(rect.right() - 8.0, rect.bottom() - 4.0));
            let bw = if c.google { 86.0 } else { 64.0 };
            let brect = Rect::from_center_size(pos2(br.right() - bw / 2.0, br.center().y), vec2(bw, 26.0));
            // a child area, so the buttons don't move the list's layout
            let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(brect));
            if c.google {
                if self.gjobs.contains(&c.family) {
                    bui.put(brect, egui::Spinner::new());
                } else if bui
                    .put(brect, egui::Button::new(RichText::new("Get font").color(Color32::BLACK)).fill(p.accent).corner_radius(CornerRadius::same(10)))
                    .on_hover_text("Download the whole family into your library and activate it")
                    .clicked()
                {
                    if let Some(gi) = gi {
                        self.google_download(gi, true);
                    }
                }
            } else if let Some(f) = fi {
                if bui.put(brect, egui::Button::new("Show")).on_hover_text("Show this font in your library").clicked() {
                    acts.push(Action::Click(f, false, false));
                }
            }
            if resp.clicked() {
                if let Some(f) = fi {
                    acts.push(Action::Click(f, false, false));
                    self.gsel = None;
                } else if let Some(g) = gi {
                    self.primary = None;
                    self.selected.clear();
                    self.gsel = Some(g);
                }
            }
            if let Some(f) = fi {
                resp.context_menu(|ui| self.family_menu(ui, f, acts));
            }
        }
        if self.finder.results.len() > SHOW {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Showing the {SHOW} closest families.")).size(11.5).color(p.weak));
        }
    }
}
