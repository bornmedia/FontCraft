//! Playground: try a font (and a partner font) on a sample web page,
//! tune sizes and spacing, and export the CSS.

use super::*;
use egui::{vec2, FontFamily, FontId, RichText, Ui};

#[derive(Clone)]
pub struct Playground {
    /// Family used for body text; None = the selected font (Font A) everywhere.
    pub font_b: Option<String>,
    pub b_query: String,
    pub size: f32,
    pub line: f32,
    pub max_width: f32,
    pub pad_top: f32,
    pub pad_bottom: f32,
    /// (size in em, line height) for h1, h2, p
    pub el: [(f32, f32); 3],
}

impl Default for Playground {
    fn default() -> Self {
        Self {
            font_b: None,
            b_query: String::new(),
            size: 16.0,
            line: 1.75,
            max_width: 700.0,
            pad_top: 2.0,
            pad_bottom: 3.0,
            el: [(2.0, 1.2), (1.5, 1.2), (1.0, 1.5)],
        }
    }
}

const SAMPLE: [(usize, &str); 6] = [
    (0, "A quiet harbour at dawn"),
    (2, "The fishing boats come back before the town wakes up. Gulls circle the masts, the café on the corner lifts its shutters, and the first bread of the day goes into the oven. Nobody hurries; the tide sets the pace here."),
    (1, "Mornings by the water"),
    (2, "Walk along the pier and you'll pass nets drying in the sun, crates of ice and a chalkboard listing the day's catch. By nine o'clock the market is busy, and by noon it is gone again, as if it had never been there."),
    (1, "Getting there"),
    (2, "A small ferry leaves the mainland twice a day. The crossing takes forty minutes, long enough for a coffee and a look at the islands drifting past."),
];

impl FontCraftApp {
    pub(super) fn tab_playground(&mut self, ui: &mut Ui, fi: usize) {
        let p = self.p();
        let fa = self.face_picker(ui, fi, false);
        // Font B: another family for the body text
        let b_face = self
            .pg
            .font_b
            .as_ref()
            .and_then(|n| self.families.iter().find(|f| &f.name == n))
            .map(|f| f.primary);
        let mut pick: Option<Option<String>> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Headings").color(p.weak).size(12.0));
            ui.label(RichText::new(&self.families[fi].name).strong());
            ui.add_space(8.0);
            ui.label(RichText::new("Text").color(p.weak).size(12.0));
            let cur = self.pg.font_b.clone().unwrap_or_else(|| "Same font".into());
            egui::ComboBox::from_id_salt("pg-font-b").selected_text(cur).width(170.0).height(320.0).show_ui(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.pg.b_query).hint_text("Search…").desired_width(150.0));
                if ui.selectable_label(self.pg.font_b.is_none(), "Same font").clicked() {
                    pick = Some(None);
                }
                let q = self.pg.b_query.to_lowercase();
                let mut shown = 0;
                for f in &self.families {
                    if shown > 200 {
                        break;
                    }
                    if !q.is_empty() && !f.lower.contains(&q) {
                        continue;
                    }
                    shown += 1;
                    if ui.selectable_label(self.pg.font_b.as_deref() == Some(&f.name), &f.name).clicked() {
                        pick = Some(Some(f.name.clone()));
                    }
                }
            });
        });
        if let Some(b) = pick {
            self.pg.font_b = b;
        }
        egui::CollapsingHeader::new(RichText::new("Page settings").size(12.5)).default_open(false).show(ui, |ui| {
            egui::Grid::new("pg-grid").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                ui.label("Font size");
                ui.add(egui::Slider::new(&mut self.pg.size, 10.0..=28.0).suffix(" px").fixed_decimals(0));
                ui.end_row();
                ui.label("Line height");
                ui.add(egui::Slider::new(&mut self.pg.line, 1.0..=2.5).suffix(" em").fixed_decimals(2));
                ui.end_row();
                ui.label("Max width");
                ui.add(egui::Slider::new(&mut self.pg.max_width, 280.0..=1200.0).suffix(" px").fixed_decimals(0));
                ui.end_row();
                ui.label("Padding top / bottom");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.pg.pad_top).range(0.0..=8.0).speed(0.05).suffix(" em"));
                    ui.add(egui::DragValue::new(&mut self.pg.pad_bottom).range(0.0..=8.0).speed(0.05).suffix(" em"));
                });
                ui.end_row();
                for (k, name) in ["h1", "h2", "p"].iter().enumerate() {
                    ui.label(*name);
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.pg.el[k].0).range(0.5..=5.0).speed(0.02).suffix(" em"));
                        ui.label(RichText::new("/").color(p.weak));
                        ui.add(egui::DragValue::new(&mut self.pg.el[k].1).range(0.8..=2.5).speed(0.01));
                    });
                    ui.end_row();
                }
            });
            if ui.small_button("Reset").clicked() {
                let b = self.pg.font_b.clone();
                self.pg = Playground { font_b: b, ..Default::default() };
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Copy CSS").on_hover_text("@font-face rules plus styles for body, h1, h2 and p").clicked() {
                let css = self.playground_css(fa, b_face, true);
                ui.ctx().copy_text(css);
                self.set_status("Playground CSS copied to the clipboard");
            }
            if ui.button("Save CSS and fonts…").clicked() {
                if let Some(path) = rfd::FileDialog::new().set_title("Save CSS").set_file_name("fonts.css").save_file() {
                    let css = self.playground_css(fa, b_face, false);
                    let mut files = vec![self.faces[fa].meta.path.clone()];
                    if let Some(b) = b_face {
                        files.push(self.faces[b].meta.path.clone());
                    }
                    if let Some(dir) = path.parent() {
                        extras::copy_files(&files, dir);
                    }
                    match std::fs::write(&path, css) {
                        Ok(()) => self.set_status(format!("Saved {} with the font files next to it", path.display())),
                        Err(e) => self.set_status(format!("Could not save: {e}")),
                    }
                }
            }
        });
        ui.add_space(6.0);
        // the sample page
        let face_a = &self.faces[fa];
        let Some(fam_a) = self.loader.family(face_a.id, &face_a.meta.path, face_a.meta.index) else {
            ui.spinner();
            return;
        };
        let fam_b: FontFamily = match b_face {
            Some(b) => {
                let fb = &self.faces[b];
                self.loader.family(fb.id, &fb.meta.path, fb.meta.index).unwrap_or(FontFamily::Proportional)
            }
            None => fam_a.clone(),
        };
        let pg = self.pg.clone();
        // shrink everything if the panel is narrower than the chosen page width
        let avail = ui.available_width() - 24.0;
        let scale = (avail / pg.max_width).min(1.0).max(0.3);
        egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(12, 0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("pg-page").auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width((pg.max_width * scale).min(avail));
                    let base = pg.size * scale;
                    ui.add_space(pg.pad_top * base);
                    for (kind, text) in SAMPLE {
                        let (em, lh) = pg.el[kind];
                        let size = base * em;
                        let family = if kind == 2 { fam_b.clone() } else { fam_a.clone() };
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            text,
                            0.0,
                            egui::TextFormat {
                                font_id: FontId::new(size, family),
                                color: Color32::from_rgb(0x1b, 0x1b, 0x1b),
                                line_height: Some(size * lh),
                                ..Default::default()
                            },
                        );
                        job.wrap.max_width = ui.available_width();
                        ui.add_space(if kind == 2 { size * 0.5 } else { size * 0.6 });
                        ui.label(job);
                        ui.add_space(if kind == 2 { size * 0.5 } else { size * 0.2 });
                    }
                    ui.add_space(pg.pad_bottom * base);
                    let _ = vec2(0.0, 0.0);
                });
            });
    }

    fn playground_css(&self, fa: usize, fb: Option<usize>, local_urls: bool) -> String {
        let mut metas = vec![&self.faces[fa].meta];
        if let Some(b) = fb {
            metas.push(&self.faces[b].meta);
        }
        let mut css = extras::css_for(&metas, local_urls);
        let a = &self.faces[fa].meta.family;
        let b = fb.map(|b| self.faces[b].meta.family.clone()).unwrap_or_else(|| a.clone());
        let pg = &self.pg;
        css.push_str(&format!(
            "main {{\n  font-family: \"{b}\", sans-serif;\n  font-size: {:.0}px;\n  line-height: {:.2}em;\n  max-width: {:.0}px;\n  padding: {:.2}em 0 {:.2}em;\n  margin: 0 auto;\n}}\n\n",
            pg.size, pg.line, pg.max_width, pg.pad_top, pg.pad_bottom
        ));
        for (k, el) in ["h1", "h2", "p"].iter().enumerate() {
            let fam = if k == 2 { &b } else { a };
            css.push_str(&format!(
                "main {el} {{\n  font-family: \"{fam}\", sans-serif;\n  font-size: {:.2}em;\n  line-height: {:.2};\n}}\n\n",
                pg.el[k].0, pg.el[k].1
            ));
        }
        css
    }
}
