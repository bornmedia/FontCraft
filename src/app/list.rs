//! The central area: toolbar, list view, grid view and font context menu.

use super::ui::{icon_button, paint_dot, paint_icon, Icon};
use super::*;
use egui::{pos2, vec2, Align, Align2, FontId, Id, Layout, Rect, RichText, Sense, StrokeKind, Ui};

/// Draw a preview so that every font sits on the same baseline, whatever its line spacing.
pub(super) fn paint_on_baseline(
    painter: &egui::Painter,
    area: Rect,
    text: &str,
    font: FontId,
    color: Color32,
    center: bool,
    wrap: Option<f32>,
) {
    let size = font.size;
    let galley = match wrap {
        Some(w) => painter.layout(text.to_string(), font, color, w),
        None => painter.layout_no_wrap(text.to_string(), font, color),
    };
    let baseline = galley
        .rows
        .first()
        .and_then(|r| r.glyphs.first())
        .map(|g| g.pos.y)
        .unwrap_or(galley.size().y * 0.8);
    let y = area.top() + size * 0.98 - baseline;
    let x = if center { area.center().x - galley.size().x / 2.0 } else { area.left() };
    painter.galley(pos2(x, y), galley, color);
}

impl FontCraftApp {
    pub(super) fn family_menu(&self, ui: &mut Ui, fi: usize, acts: &mut Vec<Action>) {
        let targets = self.targets(fi);
        let fam = &self.families[fi];
        let label = if targets.len() > 1 { format!("{} families", targets.len()) } else { fam.name.clone() };
        ui.label(RichText::new(label).strong());
        ui.separator();
        if targets.iter().any(|i| !self.families[*i].system) {
            if ui.button("Activate").clicked() {
                acts.push(Action::SetFamilies(targets.clone(), true));
                ui.close_menu();
            }
            if ui.button("Deactivate").clicked() {
                acts.push(Action::SetFamilies(targets.clone(), false));
                ui.close_menu();
            }
        }
        let fav = self.cfg.favorites.contains(&fam.name);
        if ui.button(if fav { "Remove from favorites" } else { "Add to favorites" }).clicked() {
            acts.push(Action::ToggleFavorite(targets.clone()));
            ui.close_menu();
        }
        ui.menu_button("Add to collection", |ui| {
            for c in &self.cfg.collections {
                if ui.button(&c.name).clicked() {
                    acts.push(Action::AddToCollection(targets.clone(), c.id));
                    ui.close_menu();
                }
            }
            if !self.cfg.collections.is_empty() {
                ui.separator();
            }
            if ui.button("New collection…").clicked() {
                acts.push(Action::NewCollection(targets.clone()));
                ui.close_menu();
            }
        });
        ui.menu_button("Tags", |ui| {
            for (t, set) in &self.cfg.tags {
                let has = set.contains(&fam.name);
                if ui.button(format!("{}#{t}", if has { "✔ " } else { "    " })).clicked() {
                    if has {
                        acts.push(Action::RemoveTag(targets.clone(), t.clone()));
                    } else {
                        acts.push(Action::AddTag(targets.clone(), t.clone()));
                    }
                    ui.close_menu();
                }
            }
            if !self.cfg.tags.is_empty() {
                ui.separator();
            }
            if ui.button("New tag…").clicked() {
                acts.push(Action::NewTag(targets.clone()));
                ui.close_menu();
            }
        });
        match &self.view {
            View::Collection(id) => {
                if ui.button("Remove from this collection").clicked() {
                    acts.push(Action::RemoveFromCollection(targets.clone(), *id));
                    ui.close_menu();
                }
            }
            View::Tag(t) => {
                if ui.button(format!("Remove tag #{t}")).clicked() {
                    acts.push(Action::RemoveTag(targets.clone(), t.clone()));
                    ui.close_menu();
                }
            }
            _ => {}
        }
        ui.separator();
        if ui.button("Copy CSS @font-face").clicked() {
            acts.push(Action::ExportCss(targets.clone(), fam.name.clone()));
            ui.close_menu();
        }
        if ui.button("Print specimen…").clicked() {
            acts.push(Action::Specimen(targets.clone(), fam.name.clone()));
            ui.close_menu();
        }
        if ui.button("Change family name…").on_hover_text("Group styles into a family of your choice").clicked() {
            acts.push(Action::RenameFamily(targets.clone()));
            ui.close_menu();
        }
        if ui.button("Copy family name").clicked() {
            acts.push(Action::Copy(fam.name.clone()));
            ui.close_menu();
        }
        let path = self.faces[fam.primary].meta.path.clone();
        if ui.button("Show file in folder").clicked() {
            acts.push(Action::ShowInFolder(path));
            ui.close_menu();
        }
    }

    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        let p = self.p();
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            let sz = vec2(28.0, 28.0);
            if icon_button(ui, &p, sz, Icon::List, self.cfg.view_mode == config::ViewMode::List, "List view").clicked() {
                self.cfg.view_mode = config::ViewMode::List;
                self.dirty = true;
            }
            if icon_button(ui, &p, sz, Icon::Grid, self.cfg.view_mode == config::ViewMode::Grid, "Grid view").clicked() {
                self.cfg.view_mode = config::ViewMode::Grid;
                self.dirty = true;
            }
            let (r, resp) = ui.allocate_exact_size(vec2(40.0, 28.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(6), p.hover);
            }
            let (a, b) = if self.cfg.invert_preview { (p.content, p.text) } else { (p.text, p.content) };
            paint_icon(ui.painter(), r, Icon::Invert, a, b);
            if resp.on_hover_text("Swap preview colours (light on dark / dark on light)").clicked() {
                self.cfg.invert_preview = !self.cfg.invert_preview;
                self.dirty = true;
            }
            if icon_button(ui, &p, sz, Icon::Align, self.cfg.center_align, "Centre the previews").clicked() {
                self.cfg.center_align = !self.cfg.center_align;
                self.dirty = true;
            }
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                if icon_button(ui, &p, sz, Icon::Reset, false, "Reset size and text").clicked() {
                    let d = Config::default();
                    self.cfg.preview_size = d.preview_size;
                    self.cfg.preview_text = d.preview_text;
                    self.dirty = true;
                }
                ui.add_space(6.0);
                ui.label(RichText::new(format!("{:.0}px", self.cfg.preview_size)).size(12.5).color(p.text));
                ui.add_space(10.0);
                ui.spacing_mut().slider_width = (ui.available_width() - 10.0).max(80.0);
                if ui.add(egui::Slider::new(&mut self.cfg.preview_size, 8.0..=200.0).show_value(false)).changed() {
                    self.dirty = true;
                }
            });
        });
    }

    pub(super) fn central(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let p = self.p();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.content).inner_margin(egui::Margin { left: 0, right: 0, top: 6, bottom: 0 }))
            .show(ctx, |ui| {
                self.toolbar(ui);
                if self.selected.len() > 1 {
                    self.selection_bar(ui, acts);
                }
                ui.add_space(4.0);
                if self.visible.is_empty() {
                    self.empty_state(ui, acts);
                    return;
                }
                match self.cfg.view_mode {
                    config::ViewMode::List => self.list_view(ui, acts),
                    config::ViewMode::Grid => self.grid_view(ui, acts),
                }
            });
    }

    fn selection_bar(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let p = self.p();
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(RichText::new(format!("{} selected", self.selected.len())).color(p.accent).strong());
            let sel: Vec<usize> = self.selected.iter().copied().collect();
            if ui.small_button("Activate").clicked() {
                acts.push(Action::SetFamilies(sel.clone(), true));
            }
            if ui.small_button("Deactivate").clicked() {
                acts.push(Action::SetFamilies(sel.clone(), false));
            }
            ui.menu_button("Add to collection…", |ui| {
                for c in &self.cfg.collections {
                    if ui.button(&c.name).clicked() {
                        acts.push(Action::AddToCollection(sel.clone(), c.id));
                        ui.close_menu();
                    }
                }
                if ui.button("New collection…").clicked() {
                    acts.push(Action::NewCollection(sel.clone()));
                    ui.close_menu();
                }
            });
            ui.menu_button("Tag…", |ui| {
                for t in self.cfg.tags.keys() {
                    if ui.button(format!("#{t}")).clicked() {
                        acts.push(Action::AddTag(sel.clone(), t.clone()));
                        ui.close_menu();
                    }
                }
                if ui.button("New tag…").clicked() {
                    acts.push(Action::NewTag(sel.clone()));
                    ui.close_menu();
                }
            });
            if ui.small_button("Clear").clicked() {
                self.selected.clear();
                if let Some(pr) = self.primary {
                    self.selected.insert(pr);
                }
            }
        });
    }

    fn empty_state(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let p = self.p();
        ui.add_space(90.0);
        ui.vertical_centered(|ui| {
            if self.scan_rx.is_some() && self.scans_done == 0 {
                ui.spinner();
                ui.label(RichText::new("Looking for fonts…").color(p.weak));
                return;
            }
            let (title, sub) = if !self.search.is_empty() || self.filter != Filter::All {
                ("No fonts match", "Try another search or filter.")
            } else {
                match &self.view {
                    View::All | View::Recent | View::Inactive if self.cfg.folders.is_empty() => (
                        "Your library is empty",
                        "Add a folder with fonts or import font files.\nYou can also drop fonts and folders anywhere in this window.",
                    ),
                    View::Collection(_) => (
                        "This collection is empty",
                        "Drag fonts from the list onto the collection in the sidebar,\nor right-click a font and choose “Add to collection”.",
                    ),
                    View::Tag(_) => ("No fonts with this tag", "Drag fonts onto the tag, or right-click a font and choose “Tags”."),
                    View::Favorites => ("No favorites yet", "Click the heart next to a font to add it here."),
                    View::Active => (
                        "No active fonts",
                        "Click the dot next to a font to make it available in Word, Photoshop, Affinity and every other app.",
                    ),
                    _ => ("Nothing here yet", ""),
                }
            };
            ui.label(RichText::new("Aa").size(72.0).color(p.toggle_off));
            ui.label(RichText::new(title).size(18.0).strong().color(p.text));
            ui.label(RichText::new(sub).color(p.weak));
            ui.add_space(12.0);
            if self.cfg.folders.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() / 2.0 - 120.0);
                    if ui.add(egui::Button::new(RichText::new("Add folder…").color(Color32::WHITE)).fill(p.accent)).clicked() {
                        acts.push(Action::AddFolderDialog);
                    }
                    if ui.button("Import fonts…").clicked() {
                        acts.push(Action::ImportDialog);
                    }
                });
            }
        });
    }

    fn preview_colors(&self) -> (Option<Color32>, Color32) {
        let p = self.p();
        if self.cfg.invert_preview {
            (Some(p.text), p.content)
        } else {
            (None, p.text)
        }
    }

    fn list_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let p = self.p();
        let size = self.cfg.preview_size;
        let row_h = (22.0 + size * 1.25 + 6.0).round();
        let text = if self.cfg.preview_text.is_empty() { "The quick brown fox".to_string() } else { self.cfg.preview_text.clone() };
        let (bg, fg) = self.preview_colors();
        let n = self.visible.len();
        let center = self.cfg.center_align;
        egui::ScrollArea::vertical().id_salt("list").auto_shrink([false, false]).show_rows(ui, row_h, n, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in range {
                let fi = self.visible[i];
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::hover());
                let id = Id::new(("row", fi));
                let resp = ui.interact(rect, id, Sense::click_and_drag());
                let selected = self.selected.contains(&fi);
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
                let fam = &self.families[fi];
                let face = &self.faces[fam.primary];
                let hy = rect.top() + 12.0;
                let x0 = rect.left() + 18.0;
                let hcol = if bg.is_some() { fg } else { p.text };
                let state = self.fam_state(fi);
                let dc = pos2(x0 + 4.0, hy);
                let dresp = ui.interact(Rect::from_center_size(dc, vec2(16.0, 16.0)), id.with("dot"), Sense::click());
                let pp = Pal { text: hcol, ..p };
                paint_dot(&painter, dc, state, &pp, if dresp.hovered() { 4.6 } else { 4.0 });
                let nr = painter.text(pos2(x0 + 15.0, hy), Align2::LEFT_CENTER, &fam.name, FontId::proportional(11.5), hcol.gamma_multiply(0.85));
                let fr = painter.text(pos2(nr.right() + 7.0, hy), Align2::LEFT_CENTER, &fam.ext, FontId::proportional(10.5), hcol.gamma_multiply(0.6));
                let cr = painter.text(pos2(fr.right() + 6.0, hy), Align2::LEFT_CENTER, fam.faces.len().to_string(), FontId::proportional(11.0), hcol);
                let mut tx = cr.right() + 10.0;
                if fam.system {
                    let r = painter.text(pos2(tx, hy), Align2::LEFT_CENTER, "installed", FontId::proportional(10.5), hcol.gamma_multiply(0.5));
                    tx = r.right() + 8.0;
                }
                // your own tags first (as chips), then the ones FontCraft worked out
                let limit = rect.right() - 60.0;
                for (t, set) in &self.cfg.tags {
                    if !set.contains(&fam.name) || tx > limit {
                        continue;
                    }
                    let g = painter.layout_no_wrap(t.clone(), FontId::proportional(10.5), hcol);
                    let chip = Rect::from_min_size(pos2(tx, hy - 8.0), vec2(g.size().x + 12.0, 16.0));
                    painter.rect_filled(chip, CornerRadius::same(8), p.accent.gamma_multiply(0.25));
                    painter.galley(pos2(tx + 6.0, hy - g.size().y / 2.0), g, hcol);
                    tx = chip.right() + 5.0;
                }
                for t in &fam.auto_tags {
                    if tx > limit {
                        break;
                    }
                    let r = painter.text(pos2(tx, hy), Align2::LEFT_CENTER, *t, FontId::proportional(10.5), hcol.gamma_multiply(0.45));
                    tx = r.right() + 7.0;
                }
                let prev_rect = Rect::from_min_max(pos2(rect.left() + 14.0, rect.top() + 20.0), pos2(rect.right() - 40.0, rect.bottom()));
                let pp_ = ui.painter_at(prev_rect);
                match self.loader.family(face.id, &face.meta.path, face.meta.index) {
                    Some(ff) => {
                        paint_on_baseline(&pp_, prev_rect, &text, FontId::new(size, ff), fg, center, None);
                    }
                    None => {
                        if self.loader.is_failed(face.id) {
                            pp_.text(prev_rect.left_top() + vec2(2.0, 6.0), Align2::LEFT_TOP, "(preview not available)", FontId::proportional(13.0), p.weak);
                        } else {
                            let w = (prev_rect.width() * 0.55).min(size * text.chars().count() as f32 * 0.45);
                            pp_.rect_filled(Rect::from_min_size(prev_rect.min + vec2(2.0, size * 0.2), vec2(w, size * 0.75)), CornerRadius::same(4), p.skeleton);
                        }
                    }
                }
                let fav = self.cfg.favorites.contains(&fam.name);
                let hr = Rect::from_center_size(pos2(rect.right() - 24.0, hy), vec2(20.0, 18.0));
                let hresp = ui.interact(hr, id.with("heart"), Sense::click());
                if fav || resp.hovered() || hresp.hovered() {
                    let c = if fav {
                        p.heart
                    } else if hresp.hovered() {
                        p.heart.gamma_multiply(0.7)
                    } else {
                        p.weak
                    };
                    ui.painter().text(hr.center(), Align2::CENTER_CENTER, "❤", FontId::proportional(12.0), c);
                }
                if hresp.on_hover_text(if fav { "Remove from favorites" } else { "Add to favorites" }).clicked() {
                    acts.push(Action::ToggleFavorite(vec![fi]));
                }
                if !fam.system {
                    let tip = if state.0 == state.1 { "Active in all apps – click to switch off" } else { "Click to activate (Word, Photoshop, Affinity…)" };
                    if dresp.on_hover_text(tip).clicked() {
                        acts.push(Action::SetFamilies(self.targets(fi), state.0 != state.1));
                    }
                }
                self.row_interactions(ui, &resp, fi, acts);
            }
        });
    }

    fn row_interactions(&self, ui: &Ui, resp: &egui::Response, fi: usize, acts: &mut Vec<Action>) {
        if resp.clicked() {
            let m = ui.input(|i| i.modifiers);
            acts.push(Action::Click(fi, m.command, m.shift));
        }
        if resp.double_clicked() && !self.families[fi].system {
            let (a, n) = self.fam_state(fi);
            acts.push(Action::SetFamilies(vec![fi], a != n));
        }
        if resp.drag_started() {
            resp.dnd_set_drag_payload(DragFams(self.targets(fi)));
        }
        resp.context_menu(|ui| self.family_menu(ui, fi, acts));
    }

    fn grid_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let p = self.p();
        let gap = 12.0;
        let avail = ui.available_width() - 28.0;
        let min_w = 220.0f32.max(self.cfg.preview_size * 3.2);
        let cols = (((avail + gap) / (min_w + gap)).floor() as usize).max(1);
        let card_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
        let card_h = 70.0 + self.cfg.preview_size * 2.4;
        let rows = self.visible.len().div_ceil(cols);
        let text = if self.cfg.preview_text.is_empty() { "Aa".to_string() } else { self.cfg.preview_text.clone() };
        let (bg, fg) = self.preview_colors();
        egui::ScrollArea::vertical().id_salt("grid").auto_shrink([false, false]).show_rows(ui, card_h + gap, rows, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for r in range {
                let (row_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), card_h + gap), Sense::hover());
                for c in 0..cols {
                    let k = r * cols + c;
                    if k >= self.visible.len() {
                        break;
                    }
                    let fi = self.visible[k];
                    let rect = Rect::from_min_size(
                        pos2(row_rect.left() + 14.0 + c as f32 * (card_w + gap), row_rect.top() + gap / 2.0),
                        vec2(card_w, card_h),
                    );
                    let id = Id::new(("card", fi));
                    let resp = ui.interact(rect, id, Sense::click_and_drag());
                    let selected = self.selected.contains(&fi);
                    let stroke = if selected {
                        Stroke::new(1.5, p.accent)
                    } else if resp.hovered() {
                        Stroke::new(1.0, p.weak)
                    } else {
                        Stroke::new(1.0, p.line)
                    };
                    ui.painter().rect(rect, CornerRadius::same(8), bg.unwrap_or(p.card), stroke, StrokeKind::Inside);
                    let fam = &self.families[fi];
                    let face = &self.faces[fam.primary];
                    let inner = rect.shrink(12.0);
                    let state = self.fam_state(fi);
                    let hcol = if bg.is_some() { fg } else { p.text };
                    let dc = pos2(inner.left() + 4.0, inner.top() + 6.0);
                    let dresp = ui.interact(Rect::from_center_size(dc, vec2(16.0, 16.0)), id.with("dot"), Sense::click());
                    let pp = Pal { text: hcol, ..p };
                    paint_dot(ui.painter(), dc, state, &pp, 4.0);
                    let clip = ui.painter_at(Rect::from_min_max(inner.min, pos2(inner.right() - 22.0, inner.top() + 14.0)));
                    clip.text(
                        pos2(inner.left() + 15.0, dc.y),
                        Align2::LEFT_CENTER,
                        format!("{}  {}  {}", fam.name, fam.ext, fam.faces.len()),
                        FontId::proportional(11.5),
                        hcol.gamma_multiply(0.85),
                    );
                    let fav = self.cfg.favorites.contains(&fam.name);
                    let hr = Rect::from_center_size(pos2(inner.right() - 6.0, dc.y), vec2(18.0, 16.0));
                    let hresp = ui.interact(hr, id.with("heart"), Sense::click());
                    if fav || resp.hovered() || hresp.hovered() {
                        ui.painter().text(hr.center(), Align2::CENTER_CENTER, "❤", FontId::proportional(12.0), if fav { p.heart } else { p.weak });
                    }
                    if hresp.clicked() {
                        acts.push(Action::ToggleFavorite(vec![fi]));
                    }
                    let prev = Rect::from_min_max(pos2(inner.left(), inner.top() + 22.0), inner.max);
                    let pp_ = ui.painter_at(prev);
                    match self.loader.family(face.id, &face.meta.path, face.meta.index) {
                        Some(ff) => {
                            let w = prev.width();
                            paint_on_baseline(&pp_, prev, &text, FontId::new(self.cfg.preview_size, ff), fg, self.cfg.center_align, Some(w));
                        }
                        None => {
                            pp_.rect_filled(
                                Rect::from_min_size(prev.min + vec2(0.0, 6.0), vec2(prev.width() * 0.7, self.cfg.preview_size * 0.75)),
                                CornerRadius::same(4),
                                p.skeleton,
                            );
                        }
                    }
                    if !fam.system && dresp.clicked() {
                        acts.push(Action::SetFamilies(self.targets(fi), state.0 != state.1));
                    }
                    self.row_interactions(ui, &resp, fi, acts);
                }
            }
        });
    }
}
