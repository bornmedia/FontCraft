//! The online fonts view (Google Fonts, Fontshare, Font Squirrel): the catalogue list
//! and the details card of a family that is not downloaded yet.

use super::list::paint_on_baseline;
use super::ui::paint_dot;
use super::*;
use egui::{pos2, vec2, Align, Align2, FontFamily, FontId, Id, Layout, Rect, RichText, Sense, Ui};

const CATEGORIES: [(&str, Option<&str>); 6] = [
    ("All", None),
    ("Sans Serif", Some("Sans Serif")),
    ("Serif", Some("Serif")),
    ("Display", Some("Display")),
    ("Handwriting", Some("Handwriting")),
    ("Monospace", Some("Monospace")),
];

impl FontCraftApp {
    /// The font to preview a family with; when there is none, why not (None = still loading).
    fn gpreview_font(&mut self, gi: usize) -> (Option<FontFamily>, Option<&'static str>) {
        // A family in the library previews with its own regular style.
        if let Some(fi) = self.gfam_faces(gi).first().map(|f| self.face_fam[*f]) {
            let face = &self.faces[self.families[fi].primary];
            return (self.loader.family(face.id, &face.meta.path, face.meta.index), None);
        }
        let g = self.gcat[gi].clone();
        let key = g.key();
        let online = self.shot.is_none();
        if g.provider == Provider::FontSquirrel {
            return (None, Some("(Font Squirrel shows previews only on its website – get the font to see it here)"));
        }
        match self.google.preview(&g, online) {
            Some(path) => (self.loader.family(activation::fnv(&format!("google:{key}")), &path, 0), None),
            None if !online => (None, Some("(preview not available offline)")),
            None if self.google.failed.contains(&key) => (None, Some("(no preview available)")),
            None => (None, None),
        }
    }

    fn gprovider(&self) -> Provider {
        match self.view {
            View::Online(p) => p,
            _ => Provider::Google,
        }
    }

    pub(super) fn google_central(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let p = self.p();
        let prov = self.gprovider();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.content).inner_margin(egui::Margin { left: 0, right: 0, top: 6, bottom: 0 }))
            .show(ctx, |ui| {
                self.toolbar(ui);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    for (label, cat) in CATEGORIES {
                        let sel = self.gcategory == cat;
                        let txt = RichText::new(label).size(12.0).color(if sel { Color32::BLACK } else { p.text });
                        let b = egui::Button::new(txt).fill(if sel { p.accent } else { p.input }).corner_radius(CornerRadius::same(11));
                        if ui.add(b).clicked() {
                            self.gcategory = cat;
                        }
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("{} families · {}", self.gvisible.len(), prov.terms())).size(12.0).color(p.weak));
                });
                ui.add_space(6.0);
                match self.gstate(prov).clone() {
                    GState::Loading => {
                        ui.add_space(80.0);
                        ui.vertical_centered(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(format!("Loading the {} list…", prov.label())).color(p.weak));
                        });
                    }
                    GState::Failed(e) => {
                        ui.add_space(80.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(format!("{} is not available right now", prov.label())).size(17.0).strong().color(p.text));
                            ui.label(RichText::new(e).color(p.weak));
                            ui.add_space(8.0);
                            if ui.button("Try again").clicked() {
                                self.gstates[prov.idx()] = GState::Loading;
                                self.google.load_catalog(prov, self.shot.is_none());
                            }
                        });
                    }
                    GState::Ready => {
                        if self.gvisible.is_empty() {
                            ui.add_space(80.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(format!("No {} font matches", prov.short())).size(17.0).strong().color(p.text));
                                ui.label(RichText::new("Try another search or category.").color(p.weak));
                            });
                        } else {
                            self.google_list(ui, acts);
                        }
                    }
                }
            });
    }

    fn google_list(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let p = self.p();
        let size = self.cfg.preview_size;
        let row_h = (22.0 + size * 1.25 + 6.0).round();
        let text = if self.cfg.preview_text.is_empty() { "The quick brown fox".to_string() } else { self.cfg.preview_text.clone() };
        let (bg, fg) = if self.cfg.invert_preview { (Some(p.text), p.content) } else { (None, p.text) };
        let center = self.cfg.center_align;
        let n = self.gvisible.len();
        let mut click: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt("glist").auto_shrink([false, false]).show_rows(ui, row_h, n, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in range {
                let gi = self.gvisible[i];
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::hover());
                let id = Id::new(("grow", gi));
                let resp = ui.interact(rect, id, Sense::click());
                let key = self.gcat[gi].key();
                let faces: Vec<usize> = self.gfam_faces(gi).to_vec();
                let lib_fam = faces.first().map(|f| self.face_fam[*f]);
                let selected = self.gsel == Some(gi) || lib_fam.map(|f| self.selected.contains(&f)).unwrap_or(false);
                let painter = ui.painter_at(rect);
                if let Some(bg) = bg {
                    painter.rect_filled(rect, CornerRadius::ZERO, bg);
                }
                let band = rect.shrink2(vec2(6.0, 1.0));
                if selected {
                    painter.rect_filled(band, CornerRadius::same(6), if bg.is_some() { p.accent.gamma_multiply(0.35) } else { p.sel });
                } else if resp.hovered() && bg.is_none() {
                    painter.rect_filled(band, CornerRadius::same(6), p.hover);
                }
                let hy = rect.top() + 12.0;
                let x0 = rect.left() + 18.0;
                let hcol = if bg.is_some() { fg } else { p.text };
                let state = self.faces_state(&faces);
                let dc = pos2(x0 + 4.0, hy);
                let dresp = ui.interact(Rect::from_center_size(dc, vec2(16.0, 16.0)), id.with("dot"), Sense::click());
                let busy = self.gjobs.contains(&key);
                if busy {
                    let t = ui.input(|i| i.time) as f32;
                    let a = t * 6.0;
                    painter.circle_stroke(dc, 3.6, Stroke::new(1.4_f32, p.weak));
                    painter.circle_filled(dc + vec2(3.6 * a.cos(), 3.6 * a.sin()), 1.6, p.accent);
                    ui.ctx().request_repaint();
                } else {
                    let pp = Pal { text: hcol, ..p };
                    paint_dot(&painter, dc, if faces.is_empty() { (0, 1) } else { state }, &pp, if dresp.hovered() { 4.6 } else { 4.0 });
                }
                let g = &self.gcat[gi];
                let nr = painter.text(pos2(x0 + 15.0, hy), Align2::LEFT_CENTER, &g.name, FontId::proportional(11.5), hcol.gamma_multiply(0.85));
                let cr = painter.text(pos2(nr.right() + 7.0, hy), Align2::LEFT_CENTER, &g.kind, FontId::proportional(10.5), hcol.gamma_multiply(0.6));
                let sr = painter.text(pos2(cr.right() + 6.0, hy), Align2::LEFT_CENTER, g.nstyles.to_string(), FontId::proportional(11.0), hcol);
                let mut extra = String::new();
                if !g.axes.is_empty() {
                    extra.push_str("variable  ");
                }
                if !faces.is_empty() {
                    extra.push_str("in library");
                }
                if !extra.is_empty() {
                    painter.text(pos2(sr.right() + 8.0, hy), Align2::LEFT_CENTER, extra.trim(), FontId::proportional(10.5), hcol.gamma_multiply(0.5));
                }
                let prev_rect = Rect::from_min_max(pos2(rect.left() + 14.0, rect.top() + 20.0), pos2(rect.right() - 40.0, rect.bottom()));
                let pp_ = ui.painter_at(prev_rect);
                match self.gpreview_font(gi) {
                    (Some(ff), _) => paint_on_baseline(&pp_, prev_rect, &text, FontId::new(size, ff), fg, center, None),
                    (None, Some(why)) => {
                        pp_.text(prev_rect.left_top() + vec2(2.0, 6.0), Align2::LEFT_TOP, why, FontId::proportional(13.0), p.weak);
                    }
                    (None, None) => {
                        let w = (prev_rect.width() * 0.55).min(size * text.chars().count() as f32 * 0.45);
                        pp_.rect_filled(Rect::from_min_size(prev_rect.min + vec2(2.0, size * 0.2), vec2(w, size * 0.75)), CornerRadius::same(4), p.skeleton);
                    }
                }
                // interactions
                let squirrel = self.gcat[gi].provider == Provider::FontSquirrel;
                let tip = if faces.is_empty() && squirrel {
                    "Get it in your browser – FontCraft adds and activates it when the download is in your Downloads folder"
                } else if faces.is_empty() {
                    "Download and activate (available in Word, Photoshop, Affinity…)"
                } else if state.0 == state.1 {
                    "Active in all apps – click to switch off"
                } else {
                    "Click to activate"
                };
                if dresp.on_hover_text(tip).clicked() && !busy {
                    if faces.is_empty() {
                        self.google_download(gi, true);
                    } else {
                        let paths: Vec<String> = faces.iter().map(|f| self.faces[*f].path_key.clone()).collect();
                        acts.push(Action::SetPaths(paths, state.0 != state.1));
                    }
                }
                if resp.clicked() {
                    match lib_fam {
                        Some(fi) => {
                            let m = ui.input(|i| i.modifiers);
                            acts.push(Action::Click(fi, m.command, m.shift));
                            self.gsel = None;
                        }
                        None => click = Some(gi),
                    }
                }
                if resp.double_clicked() && faces.is_empty() && !busy {
                    self.google_download(gi, true);
                }
                resp.context_menu(|ui| self.google_menu(ui, gi, acts));
            }
        });
        if let Some(gi) = click {
            self.gsel = Some(gi);
            self.primary = None;
            self.selected.clear();
        }
    }

    fn google_menu(&mut self, ui: &mut Ui, gi: usize, acts: &mut Vec<Action>) {
        let name = self.gcat[gi].name.clone();
        let faces: Vec<usize> = self.gfam_faces(gi).to_vec();
        ui.label(RichText::new(&name).strong());
        ui.separator();
        if faces.is_empty() {
            if ui.button("Download and activate").clicked() {
                self.google_download(gi, true);
                ui.close_menu();
            }
            if ui.button("Add to library (without activating)").clicked() {
                self.google_download(gi, false);
                ui.close_menu();
            }
        } else {
            let paths: Vec<String> = faces.iter().map(|f| self.faces[*f].path_key.clone()).collect();
            if ui.button("Activate").clicked() {
                acts.push(Action::SetPaths(paths.clone(), true));
                ui.close_menu();
            }
            if ui.button("Deactivate").clicked() {
                acts.push(Action::SetPaths(paths, false));
                ui.close_menu();
            }
            let fi = self.face_fam[faces[0]];
            ui.menu_button("Add to collection", |ui| {
                for c in &self.cfg.collections {
                    if ui.button(&c.name).clicked() {
                        acts.push(Action::AddToCollection(vec![fi], c.id));
                        ui.close_menu();
                    }
                }
                if ui.button("New collection…").clicked() {
                    acts.push(Action::NewCollection(vec![fi]));
                    ui.close_menu();
                }
            });
            if ui.button("Delete…").on_hover_text("Deactivate it, or also delete the downloaded copy (FontCraft asks first)").clicked() {
                self.google_remove(gi);
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button("Copy family name").clicked() {
            acts.push(Action::Copy(name.clone()));
            ui.close_menu();
        }
        if ui.button(format!("Open on {}", self.gcat[gi].provider.site())).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(self.gcat[gi].url()));
            ui.close_menu();
        }
    }

    /// Details for a Google family that hasn't been downloaded yet.
    pub(super) fn google_details(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(gi) = self.gsel.filter(|g| *g < self.gcat.len()) else { return };
        let p = self.p();
        let mut close = false;
        let (font, why) = self.gpreview_font(gi);
        let g = self.gcat[gi].clone();
        let busy = self.gjobs.contains(&g.key());
        egui::SidePanel::right("details")
            .default_width(420.0)
            .min_width(320.0)
            .max_width(760.0)
            .resizable(true)
            .frame(egui::Frame::new().fill(p.detail).inner_margin(egui::Margin::symmetric(18, 14)).stroke(Stroke::new(1.0_f32, p.line)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if ui.add(egui::Button::new(RichText::new("✖").size(14.0).color(p.weak)).frame(false)).on_hover_text("Close").clicked() {
                            close = true;
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let f = FontId::new(26.0, font.clone().unwrap_or(FontFamily::Proportional));
                            ui.add(egui::Label::new(RichText::new(&g.name).font(f).color(p.text)).truncate());
                        });
                    });
                });
                let mut sub = format!("{} · {} style{} · {}", g.kind, g.nstyles, if g.nstyles == 1 { "" } else { "s" }, g.provider.label());
                if !g.designers.is_empty() {
                    sub.push_str(&format!(" · by {}", g.designers.join(", ")));
                }
                ui.add(egui::Label::new(RichText::new(sub).color(p.weak).size(12.0)).wrap());
                ui.add_space(10.0);
                let squirrel = g.provider == Provider::FontSquirrel;
                ui.horizontal(|ui| {
                    if busy {
                        ui.spinner();
                        ui.label(RichText::new(if squirrel { "Waiting for the download in your browser…" } else { "Downloading…" }).color(p.weak));
                    } else if squirrel {
                        let b = egui::Button::new(RichText::new("Get font in browser").color(Color32::BLACK)).fill(p.accent);
                        if ui
                            .add(b)
                            .on_hover_text("Font Squirrel hands out its fonts only to a browser. FontCraft adds and activates the family as soon as the download is in your Downloads folder.")
                            .clicked()
                        {
                            self.google_download(gi, true);
                        }
                    } else {
                        let b = egui::Button::new(RichText::new("Activate").color(Color32::BLACK)).fill(p.accent);
                        if ui.add(b).on_hover_text("Download all styles and make them available in every app").clicked() {
                            self.google_download(gi, true);
                        }
                        if ui.button("Add to library").on_hover_text("Download without activating").clicked() {
                            self.google_download(gi, false);
                        }
                    }
                    if ui.button(g.provider.site()).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(g.url()));
                    }
                });
                ui.add_space(12.0);
                egui::ScrollArea::vertical().id_salt("gdetails").auto_shrink([false, false]).show(ui, |ui| {
                    let text = if self.cfg.preview_text.is_empty() { "Handgloves".to_string() } else { self.cfg.preview_text.clone() };
                    match &font {
                        Some(ff) => {
                            for s in [14.0, 20.0, 28.0, 40.0, 56.0] {
                                ui.horizontal(|ui| {
                                    ui.add_sized([30.0, s * 1.2], egui::Label::new(RichText::new(format!("{s:.0}")).monospace().size(11.0).color(p.weak)));
                                    ui.add(egui::Label::new(RichText::new(&text).font(FontId::new(s, ff.clone())).color(p.text)).wrap());
                                });
                            }
                            ui.add_space(6.0);
                            ui.label(RichText::new("ABCDEFGHIJKLMNOPQRSTUVWXYZ\nabcdefghijklmnopqrstuvwxyz\n0123456789 !?&@").font(FontId::new(24.0, ff.clone())).color(p.text));
                        }
                        None => match why {
                            Some(w) => {
                                ui.label(RichText::new(w).color(p.weak));
                            }
                            None => {
                                ui.spinner();
                            }
                        },
                    }
                    if !g.style_names.is_empty() {
                        ui.add_space(12.0);
                        ui.label(RichText::new("STYLES").size(11.0).strong().color(p.weak));
                        ui.horizontal_wrapped(|ui| {
                            for s in &g.style_names {
                                egui::Frame::new().fill(p.input).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                                    ui.label(RichText::new(s).size(11.5).color(p.text));
                                });
                            }
                        });
                    }
                    if !g.license.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new("LICENSE").size(11.0).strong().color(p.weak));
                        ui.add(egui::Label::new(RichText::new(&g.license).size(12.0).color(p.text)).wrap());
                    }
                    if !g.axes.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new("VARIABLE AXES").size(11.0).strong().color(p.weak));
                        for (tag, min, max) in &g.axes {
                            ui.label(format!("{tag}: {min:.0} – {max:.0}"));
                        }
                    }
                    if !g.added.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new(format!("Added to {} on {}", g.provider.label(), g.added)).size(11.5).color(p.weak));
                    }
                });
            });
        if close {
            self.gsel = None;
        }
        let _ = acts;
    }
}
