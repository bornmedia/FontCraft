//! The right-hand font details panel.

use super::ui::{paint_dot, paint_toggle};
use super::*;
use egui::{pos2, vec2, Align, Align2, FontFamily, FontId, Id, Layout, Rect, RichText, Sense, StrokeKind, Ui};

fn human_size(b: u64) -> String {
    if b > 1_000_000 {
        format!("{:.1} MB", b as f64 / 1_000_000.0)
    } else {
        format!("{:.0} KB", b as f64 / 1000.0)
    }
}

fn weight_name(w: u16) -> &'static str {
    match w {
        0..=149 => "Thin",
        150..=249 => "Extra Light",
        250..=349 => "Light",
        350..=449 => "Regular",
        450..=549 => "Medium",
        550..=649 => "Semi Bold",
        650..=749 => "Bold",
        750..=849 => "Extra Bold",
        _ => "Black",
    }
}

impl FontCraftApp {
    pub(super) fn details(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(fi) = self.primary else { return };
        if fi >= self.families.len() {
            return;
        }
        let p = self.p();
        let mut close = false;
        use super::sidebar::{DETAILS_MAX, DETAILS_MIN};
        // drag its left edge to make it wider or narrower; the width is remembered
        let panel = egui::SidePanel::right("details")
            .default_width(self.cfg.details_w.clamp(DETAILS_MIN, DETAILS_MAX))
            .width_range(DETAILS_MIN..=DETAILS_MAX)
            .resizable(true)
            .frame(egui::Frame::new().fill(p.detail).inner_margin(egui::Margin::symmetric(18, 14)).stroke(Stroke::new(1.0_f32, p.line)))
            .show(ctx, |ui| {
                let fam_name = self.families[fi].name.clone();
                let nfaces = self.families[fi].faces.len();
                let system = self.families[fi].system;
                let primary = self.families[fi].primary;
                let pf = &self.faces[primary];
                let fam_font = self.loader.family(pf.id, &pf.meta.path, pf.meta.index);
                ui.horizontal(|ui| {
                    let font = FontId::new(26.0, fam_font.clone().unwrap_or(FontFamily::Proportional));
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if ui.add(egui::Button::new(RichText::new("✖").size(14.0).color(p.weak)).frame(false)).on_hover_text("Close").clicked() {
                            close = true;
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(&fam_name).font(font).color(p.text)).truncate());
                        });
                    });
                });
                let src = if system {
                    "Installed on this computer".to_string()
                } else if let Some(prov) = google::Provider::ALL.iter().find(|p| self.faces[primary].meta.path.starts_with(p.fonts_dir())) {
                    prov.label().to_string()
                } else {
                    let path = self.faces[primary].meta.path.parent().map(|d| d.to_path_buf());
                    path.and_then(|d| self.node_index.get(&d).map(|i| self.nodes[*i].name.clone())).unwrap_or_default()
                };
                let styles = format!("{nfaces} style{}", if nfaces == 1 { "" } else { "s" });
                let line = if src.is_empty() { styles } else { format!("{styles} · {src}") };
                ui.label(RichText::new(line).color(p.weak).size(12.0));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if !system {
                        let (a, n) = self.fam_state(fi);
                        let (tr, tresp) = ui.allocate_exact_size(vec2(38.0, 20.0), Sense::click());
                        let t = ui.ctx().animate_bool(Id::new(("dt", fi)), a == n);
                        paint_toggle(ui, tr, t, &p);
                        if tresp.clicked() {
                            acts.push(Action::SetFamilies(vec![fi], a != n));
                        }
                        let label = if a == n {
                            "Active in all apps".to_string()
                        } else if a == 0 {
                            "Not active".to_string()
                        } else {
                            format!("{a} of {n} styles active")
                        };
                        ui.label(RichText::new(label).color(if a > 0 { p.accent } else { p.weak }));
                    } else {
                        ui.label(RichText::new("✔ Installed — always available").color(p.weak));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let fav = self.cfg.favorites.contains(&fam_name);
                        let heart = RichText::new("❤").color(if fav { p.heart } else { p.weak });
                        if ui.add(egui::Button::new(heart).frame(false)).on_hover_text("Favorite").clicked() {
                            acts.push(Action::ToggleFavorite(vec![fi]));
                        }
                        ui.menu_button(RichText::new("…").color(p.text), |ui| self.family_menu(ui, fi, acts));
                    });
                });
                // tags on this family
                let tags: Vec<String> = self.cfg.tags.iter().filter(|(_, s)| s.contains(&fam_name)).map(|(t, _)| t.clone()).collect();
                let in_cols: Vec<(String, [u8; 3])> =
                    self.cfg.collections.iter().filter(|c| c.families.contains(&fam_name)).map(|c| (c.name.clone(), c.color)).collect();
                if !tags.is_empty() || !in_cols.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (n, c) in &in_cols {
                            let col = Color32::from_rgb(c[0], c[1], c[2]);
                            egui::Frame::new().fill(col.gamma_multiply(0.18)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                                ui.label(RichText::new(n).size(11.5).color(p.text));
                            });
                        }
                        for t in &tags {
                            egui::Frame::new().fill(p.input).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                                ui.label(RichText::new(format!("#{t}")).size(11.5).color(p.text));
                            });
                        }
                    });
                }
                ui.add_space(8.0);
                let has_var = self.families[fi].variable;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.spacing_mut().button_padding.x = 0.0;
                    let mut tabs = vec![(Tab::Styles, "Styles"), (Tab::Glyphs, "Glyphs"), (Tab::Waterfall, "Waterfall"), (Tab::Tester, "Tester")];
                    if has_var {
                        tabs.push((Tab::Variable, "Variable"));
                    }
                    tabs.push((Tab::Playground, "Playground"));
                    tabs.push((Tab::Info, "Info"));
                    for (t, l) in tabs {
                        let sel = self.tab == t;
                        let txt = RichText::new(l).size(13.0).color(if sel { p.text } else { p.weak });
                        let r = ui.add(egui::Button::new(txt).frame(false));
                        if sel {
                            ui.painter().hline(r.rect.x_range(), r.rect.bottom() + 3.0, Stroke::new(2.0_f32, p.accent));
                        }
                        if r.clicked() {
                            self.tab = t;
                        }
                    }
                });
                if self.tab == Tab::Variable && !has_var {
                    self.tab = Tab::Styles;
                }
                let r = ui.max_rect();
                ui.painter().hline(r.x_range(), ui.cursor().top() + 3.0, Stroke::new(1.0_f32, p.line));
                ui.add_space(12.0);
                match self.tab {
                    Tab::Styles => self.tab_styles(ui, fi, acts),
                    Tab::Glyphs => self.tab_glyphs(ui, fi),
                    Tab::Waterfall => self.tab_waterfall(ui, fi),
                    Tab::Tester => self.tab_tester(ui, fi),
                    Tab::Variable => self.tab_variable(ui, fi),
                    Tab::Info => self.tab_info(ui, fi, acts),
                    Tab::Playground => self.tab_playground(ui, fi),
                }
            });
        self.remember_width(panel.response.rect.width(), false);
        if close {
            self.primary = None;
            self.selected.clear();
        }
    }

    pub(super) fn face_picker(&mut self, ui: &mut Ui, fi: usize, only_variable: bool) -> usize {
        let faces: Vec<usize> = self.families[fi]
            .faces
            .iter()
            .copied()
            .filter(|i| !only_variable || !self.faces[*i].meta.axes.is_empty())
            .collect();
        if faces.is_empty() {
            return self.families[fi].primary;
        }
        if self.detail_face >= faces.len() {
            self.detail_face = faces.iter().position(|x| *x == self.families[fi].primary).unwrap_or(0);
        }
        if faces.len() > 1 {
            let cur = self.faces[faces[self.detail_face]].meta.style.clone();
            egui::ComboBox::from_id_salt(("facepick", fi)).selected_text(cur).width(ui.available_width().min(260.0)).show_ui(ui, |ui| {
                for (k, f) in faces.iter().enumerate() {
                    if ui.selectable_label(self.detail_face == k, &self.faces[*f].meta.style).clicked() {
                        self.detail_face = k;
                        self.glyph_sel = None;
                    }
                }
            });
            ui.add_space(6.0);
        }
        faces[self.detail_face]
    }

    fn tab_styles(&mut self, ui: &mut Ui, fi: usize, acts: &mut Vec<Action>) {
        let p = self.p();
        let size = self.cfg.preview_size.min(40.0);
        let text = self.cfg.preview_text.clone();
        let faces = self.families[fi].faces.clone();
        let system = self.families[fi].system;
        egui::ScrollArea::vertical().id_salt("styles").auto_shrink([false, false]).show(ui, |ui| {
            for f in faces {
                let face = &self.faces[f];
                let active = self.act[f];
                let style = face.meta.style.clone();
                let path_key = face.path_key.clone();
                let font = self.loader.family(face.id, &face.meta.path, face.meta.index);
                ui.horizontal(|ui| {
                    if !system {
                        let (dr, dresp) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::click());
                        paint_dot(ui.painter(), dr.center(), (active as usize, 1), &p, 4.0);
                        if dresp.on_hover_text("Switch just this style on or off").clicked() {
                            acts.push(Action::SetPaths(vec![path_key.clone()], !active));
                        }
                    }
                    ui.label(RichText::new(&style).size(11.5).color(p.weak));
                });
                match font {
                    Some(ff) => {
                        ui.label(RichText::new(&text).font(FontId::new(size, ff)).color(p.text));
                    }
                    None => {
                        ui.spinner();
                    }
                }
                ui.add_space(10.0);
            }
        });
    }

    fn tab_glyphs(&mut self, ui: &mut Ui, fi: usize) {
        let p = self.p();
        let f = self.face_picker(ui, fi, false);
        let face_id = self.faces[f].id;
        if self.glyphs.as_ref().map(|g| g.0) != Some(face_id) {
            let list = library::glyph_list(&self.faces[f].meta.path, self.faces[f].meta.index);
            self.glyphs = Some((face_id, list));
            self.glyph_sel = None;
        }
        let face = &self.faces[f];
        let Some(ff) = self.loader.family(face.id, &face.meta.path, face.meta.index) else {
            ui.spinner();
            return;
        };
        ui.add(
            egui::TextEdit::singleline(&mut self.glyph_query)
                .hint_text("Search glyphs: a character, a name (e.g. ampersand) or U+0041")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        let all = &self.glyphs.as_ref().unwrap().1;
        let q = self.glyph_query.trim().to_lowercase();
        let idx: Vec<usize> = (0..all.len())
            .filter(|i| {
                if q.is_empty() {
                    return true;
                }
                let g = &all[*i];
                let code = format!("u+{:04x}", g.ch as u32);
                q.chars().count() == 1 && g.ch.to_lowercase().to_string() == q
                    || code.contains(q.trim_start_matches("0x"))
                    || g.name.as_ref().map(|n| n.to_lowercase().contains(&q)).unwrap_or(false)
                    || (q.chars().count() > 2 && library::unicode_block(g.ch).to_lowercase().contains(&q))
            })
            .collect();
        if idx.is_empty() {
            ui.label(RichText::new(if all.is_empty() { "No characters found in this font." } else { "No glyph matches your search." }).color(p.weak));
            return;
        }
        let sel = self
            .glyph_sel
            .filter(|s| idx.contains(s))
            .unwrap_or_else(|| idx.iter().copied().find(|i| all[*i].ch == 'A').unwrap_or(idx[0]));
        let g = &all[sel];
        egui::Frame::new().fill(p.card).corner_radius(CornerRadius::same(8)).stroke(Stroke::new(1.0_f32, p.line)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(130.0, 130.0), Sense::hover());
                ui.painter().hline(r.x_range(), r.center().y + 30.0, Stroke::new(1.0_f32, p.line));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, g.ch.to_string(), FontId::new(96.0, ff.clone()), p.text);
                ui.vertical(|ui| {
                    ui.add_space(14.0);
                    ui.label(RichText::new(format!("U+{:04X}", g.ch as u32)).monospace().size(15.0).color(p.text));
                    if let Some(n) = &g.name {
                        ui.label(RichText::new(n).color(p.weak));
                    }
                    ui.label(RichText::new(format!("{} of {} characters", idx.len(), all.len())).color(p.weak).size(12.0));
                    if ui.small_button("Copy character").clicked() {
                        ui.ctx().copy_text(g.ch.to_string());
                    }
                });
            });
        });
        ui.add_space(6.0);
        // Unicode blocks present in this font
        let mut blocks: Vec<(&'static str, usize)> = vec![];
        for i in &idx {
            let b = library::unicode_block(all[*i].ch);
            match blocks.iter_mut().find(|(n, _)| *n == b) {
                Some(e) => e.1 += 1,
                None => blocks.push((b, 1)),
            }
        }
        if let Some(b) = &self.glyph_block {
            if !blocks.iter().any(|(n, _)| n == b) {
                self.glyph_block = None;
            }
        }
        let shown: Vec<usize> = idx
            .iter()
            .copied()
            .filter(|i| self.glyph_block.as_deref().map(|b| library::unicode_block(all[*i].ch) == b).unwrap_or(true))
            .collect();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("glyphblock")
                .selected_text(self.glyph_block.clone().unwrap_or_else(|| "All blocks".into()))
                .width(200.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.glyph_block.is_none(), "All blocks").clicked() {
                        self.glyph_block = None;
                    }
                    for (b, n) in &blocks {
                        if ui.selectable_label(self.glyph_block.as_deref() == Some(*b), format!("{b}  ({n})")).clicked() {
                            self.glyph_block = Some(b.to_string());
                        }
                    }
                });
            ui.label(RichText::new(format!("Displaying {} of {} glyphs", shown.len(), all.len())).size(11.5).color(p.weak));
        });
        ui.add_space(4.0);
        // rows: a heading per block, then the glyphs of that block
        enum Row {
            Head(&'static str),
            Cells(Vec<usize>),
        }
        let cell = 44.0;
        let cols = ((ui.available_width() / cell).floor() as usize).max(1);
        let mut rows: Vec<Row> = vec![];
        let mut cur: Option<&'static str> = None;
        let mut line: Vec<usize> = vec![];
        for gi in &shown {
            let b = library::unicode_block(all[*gi].ch);
            if cur != Some(b) {
                if !line.is_empty() {
                    rows.push(Row::Cells(std::mem::take(&mut line)));
                }
                rows.push(Row::Head(b));
                cur = Some(b);
            }
            line.push(*gi);
            if line.len() == cols {
                rows.push(Row::Cells(std::mem::take(&mut line)));
            }
        }
        if !line.is_empty() {
            rows.push(Row::Cells(line));
        }
        let mut new_sel = None;
        let mut copy: Option<String> = None;
        egui::ScrollArea::vertical().id_salt("glyphs").auto_shrink([false, false]).show_rows(ui, cell, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for r in range {
                match &rows[r] {
                    Row::Head(b) => {
                        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), cell), Sense::hover());
                        ui.painter().text(pos2(rect.left() + 2.0, rect.bottom() - 10.0), Align2::LEFT_BOTTOM, *b, FontId::proportional(12.5), p.text);
                    }
                    Row::Cells(cells) => {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            for gi in cells {
                                let gi = *gi;
                                let (rect, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
                                let bg = if gi == sel {
                                    p.accent_soft
                                } else if resp.hovered() {
                                    p.hover
                                } else {
                                    p.card
                                };
                                ui.painter().rect(rect.shrink(1.0), CornerRadius::same(4), bg, Stroke::new(1.0_f32, p.line), StrokeKind::Inside);
                                ui.painter_at(rect).text(rect.center(), Align2::CENTER_CENTER, all[gi].ch.to_string(), FontId::new(22.0, ff.clone()), p.text);
                                let resp = resp.on_hover_text(format!(
                                    "U+{:04X}{}  ·  double-click to copy",
                                    all[gi].ch as u32,
                                    all[gi].name.as_ref().map(|n| format!(" {n}")).unwrap_or_default()
                                ));
                                if resp.clicked() {
                                    new_sel = Some(gi);
                                }
                                if resp.double_clicked() {
                                    copy = Some(all[gi].ch.to_string());
                                }
                                resp.context_menu(|ui| {
                                    if ui.button("Copy character").clicked() {
                                        copy = Some(all[gi].ch.to_string());
                                        ui.close_menu();
                                    }
                                    if ui.button("Copy code (U+…)").clicked() {
                                        copy = Some(format!("U+{:04X}", all[gi].ch as u32));
                                        ui.close_menu();
                                    }
                                    if ui.button("Copy HTML entity").clicked() {
                                        copy = Some(format!("&#x{:X};", all[gi].ch as u32));
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                    }
                }
            }
        });
        if new_sel.is_some() {
            self.glyph_sel = new_sel;
        }
        if let Some(c) = copy {
            ui.ctx().copy_text(c.clone());
            self.set_status(format!("Copied {c}"));
        }
    }

    fn tab_waterfall(&mut self, ui: &mut Ui, fi: usize) {
        let p = self.p();
        let f = self.face_picker(ui, fi, false);
        let face = &self.faces[f];
        let Some(ff) = self.loader.family(face.id, &face.meta.path, face.meta.index) else {
            ui.spinner();
            return;
        };
        let text = self.cfg.preview_text.clone();
        egui::ScrollArea::both().id_salt("water").auto_shrink([false, false]).show(ui, |ui| {
            for s in [9.0, 10.0, 12.0, 14.0, 16.0, 18.0, 21.0, 24.0, 28.0, 32.0, 36.0, 42.0, 48.0, 60.0, 72.0, 96.0] {
                ui.horizontal(|ui| {
                    ui.add_sized([34.0, s * 1.2], egui::Label::new(RichText::new(format!("{s:.0}")).monospace().size(11.0).color(p.weak)));
                    ui.add(egui::Label::new(RichText::new(&text).font(FontId::new(s, ff.clone())).color(p.text)).extend());
                });
            }
        });
    }

    fn tab_tester(&mut self, ui: &mut Ui, fi: usize) {
        let p = self.p();
        let f = self.face_picker(ui, fi, false);
        let face = &self.faces[f];
        let Some(ff) = self.loader.family(face.id, &face.meta.path, face.meta.index) else {
            ui.spinner();
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("Size").color(p.weak));
            ui.add(egui::Slider::new(&mut self.tester_size, 8.0..=120.0).suffix(" px"));
        });
        ui.label(RichText::new("Type anything below to try the font.").color(p.weak).size(12.0));
        egui::ScrollArea::vertical().id_salt("tester").auto_shrink([false, false]).show(ui, |ui| {
            if ui
                .add(
                    egui::TextEdit::multiline(&mut self.cfg.tester_text)
                        .font(FontId::new(self.tester_size, ff))
                        .desired_width(f32::INFINITY)
                        .desired_rows(10),
                )
                .changed()
            {
                self.dirty = true;
            }
        });
    }

    fn tab_variable(&mut self, ui: &mut Ui, fi: usize) {
        let p = self.p();
        let f = self.face_picker(ui, fi, true);
        let axes = self.faces[f].meta.axes.clone();
        if axes.is_empty() {
            ui.label(RichText::new("This style has no variable axes.").color(p.weak));
            return;
        }
        ui.label(RichText::new("Move the sliders to explore every in-between style of this variable font.").color(p.weak).size(12.0));
        ui.add_space(4.0);
        egui::Grid::new("axes").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            for a in &axes {
                let v = self.var_values.entry(a.tag.clone()).or_insert(a.def);
                ui.label(RichText::new(format!("{} ({})", a.name, a.tag)).color(p.text));
                ui.add(egui::Slider::new(v, a.min..=a.max).fixed_decimals(0));
                ui.end_row();
            }
        });
        if ui.small_button("Reset").clicked() {
            self.var_values.clear();
        }
        ui.add_space(8.0);
        let vars: Vec<(String, f32)> = axes.iter().map(|a| (a.tag.clone(), *self.var_values.get(&a.tag).unwrap_or(&a.def))).collect();
        let text: String = {
            let t = if self.cfg.preview_text.is_empty() { "Handgloves" } else { &self.cfg.preview_text };
            t.chars().take(60).collect()
        };
        let key = activation::fnv(&format!("{}{:?}{}{:?}", self.faces[f].id, vars, text, self.cfg.theme));
        if self.var_tex.as_ref().map(|t| t.0) != Some(key) {
            let face = &self.faces[f];
            if let Some(img) = library::render_line(&face.meta.path, face.meta.index, &text, 64.0, &vars, p.text) {
                let tex = ui.ctx().load_texture("var-preview", img, egui::TextureOptions::LINEAR);
                self.var_tex = Some((key, tex));
            }
        }
        if let Some((_, tex)) = &self.var_tex {
            let sz = tex.size_vec2();
            let scale = (ui.available_width() / sz.x).clamp(0.4, 1.0);
            egui::ScrollArea::horizontal().id_salt("varimg").show(ui, |ui| {
                ui.image(egui::load::SizedTexture::new(tex.id(), sz * scale));
            });
        }
    }

    fn tab_info(&mut self, ui: &mut Ui, fi: usize, acts: &mut Vec<Action>) {
        let p = self.p();
        let f = self.face_picker(ui, fi, false);
        let m = self.faces[f].meta.clone();
        egui::ScrollArea::vertical().id_salt("info").auto_shrink([false, false]).show(ui, |ui| {
            let row = |ui: &mut Ui, k: &str, v: &str| {
                if v.is_empty() {
                    return;
                }
                ui.label(RichText::new(k).color(p.weak).size(12.0));
                ui.add(egui::Label::new(RichText::new(v).color(p.text).size(12.5)).wrap());
                ui.end_row();
            };
            egui::Grid::new("infogrid").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                row(ui, "Family", &m.family);
                row(ui, "Style", &m.style);
                row(ui, "Full name", &m.full_name);
                row(ui, "PostScript", &m.postscript);
                row(ui, "Version", &m.version);
                row(ui, "Designer", &m.designer);
                row(ui, "Foundry", &m.manufacturer);
                row(ui, "Website", &m.url);
                row(ui, "Format", &m.format);
                row(ui, "Weight", &format!("{} ({})", m.weight, weight_name(m.weight)));
                row(ui, "Width", &m.width.to_string());
                row(ui, "Italic", if m.italic { "Yes" } else { "No" });
                row(ui, "Monospace", if m.monospace { "Yes" } else { "No" });
                row(ui, "Glyphs", &m.glyph_count.to_string());
                row(ui, "File size", &human_size(m.file_size));
                row(ui, "File", &m.path.display().to_string());
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Show in folder").clicked() {
                    acts.push(Action::ShowInFolder(m.path.clone()));
                }
                if !m.postscript.is_empty() && ui.button("Copy PostScript name").clicked() {
                    acts.push(Action::Copy(m.postscript.clone()));
                }
                if ui.button("Copy CSS").clicked() {
                    acts.push(Action::ExportCss(vec![fi], m.family.clone()));
                }
                if ui.button("Print specimen").clicked() {
                    acts.push(Action::Specimen(vec![fi], m.family.clone()));
                }
            });
            if !m.axes.is_empty() {
                ui.add_space(10.0);
                ui.label(RichText::new("VARIABLE AXES").size(11.0).strong().color(p.weak));
                for a in &m.axes {
                    ui.label(format!("{} ({}) — {:.0} to {:.0}, default {:.0}", a.name, a.tag, a.min, a.max, a.def));
                }
            }
            if !m.features.is_empty() {
                ui.add_space(10.0);
                ui.label(RichText::new("OPENTYPE FEATURES").size(11.0).strong().color(p.weak));
                ui.horizontal_wrapped(|ui| {
                    for t in &m.features {
                        let nm = library::feature_name(t);
                        let r = egui::Frame::new()
                            .fill(p.input)
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| ui.label(RichText::new(t).monospace().size(11.5).color(p.text)));
                        if !nm.is_empty() {
                            r.response.on_hover_text(nm);
                        }
                    }
                });
            }
            for (k, v) in [("DESCRIPTION", &m.description), ("COPYRIGHT", &m.copyright)] {
                if !v.is_empty() {
                    ui.add_space(10.0);
                    ui.label(RichText::new(k).size(11.0).strong().color(p.weak));
                    ui.add(egui::Label::new(RichText::new(v.as_str()).size(12.0).color(p.text)).wrap());
                }
            }
            let _ = (pos2(0.0, 0.0), Rect::NOTHING);
        });
    }
}
