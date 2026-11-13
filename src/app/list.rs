//! The central area: toolbar, list view, grid view and font context menu.

use super::ui::{icon_button, paint_dot, paint_icon, Icon};
use super::*;
use egui::{pos2, vec2, Align, Align2, FontId, Id, Layout, Rect, RichText, Sense, StrokeKind, Ui};

/// Draw a preview so that every font sits on the same baseline, whatever its line spacing.
pub(super) const TILE_GAP: f32 = 12.0;

/// The tile view: how many tiles fit next to each other and how big each one is. All tiles have the
/// same size; they grow with the preview size and share out the window's width, so a wide window
/// (or a big screen) shows more tiles per row.
pub(super) fn tile_layout(avail: f32, preview: f32) -> (usize, f32, f32) {
    // the width a tile would like: enough for a few letters at the chosen size
    let want = (preview * 5.0).clamp(190.0, 520.0);
    let cols = (((avail + TILE_GAP) / (want + TILE_GAP)).floor() as usize).max(1);
    let w = ((avail - TILE_GAP * (cols as f32 - 1.0)) / cols as f32).max(120.0);
    // a calm, slightly wide shape, tall enough for two to three lines of the preview
    let h = (w * 0.62).max(48.0 + preview * 2.3).min(w * 1.1).max(48.0 + preview * 1.3);
    (cols, w, h)
}

/// `align`: where the text sits (left, centred, right, or justified when it wraps over several lines).
pub(super) fn paint_on_baseline(
    painter: &egui::Painter,
    area: Rect,
    text: &str,
    font: FontId,
    color: Color32,
    align: config::TextAlign,
    wrap: Option<f32>,
) {
    let size = font.size;
    let make = |max_rows: usize| {
        let mut job = egui::text::LayoutJob::simple(text.to_string(), font.clone(), color, wrap.unwrap_or(f32::INFINITY));
        job.halign = align.halign();
        job.justify = align.justify() && wrap.is_some();
        job.wrap.max_rows = max_rows;
        job.wrap.overflow_character = None;
        painter.layout_job(job)
    };
    let mut galley = make(usize::MAX);
    let baseline = galley
        .rows
        .first()
        .and_then(|r| r.glyphs.first())
        .map(|g| g.pos.y)
        .unwrap_or(galley.size().y * 0.8);
    let y = area.top() + size * 0.98 - baseline;
    // wrapped text (tiles): only the lines that fit completely, no half-cut line at the bottom
    if wrap.is_some() {
        let fit = galley.rows.iter().filter(|r| y + r.rect.bottom() <= area.bottom() + 1.0).count().max(1);
        if fit < galley.rows.len() {
            galley = make(fit);
        }
    }
    // the text is laid out around its anchor: left edge, middle or right edge
    let x = match align.halign() {
        Align::Min => area.left(),
        Align::Center => area.center().x,
        Align::Max => area.right(),
    };
    painter.galley(pos2(x, y), galley, color);
}

/// The alignment icon: four lines set the way the option sets text.
pub(super) fn paint_align_icon(painter: &egui::Painter, r: Rect, align: config::TextAlign, col: Color32) {
    let c = r.center();
    let s = Stroke::new(1.4_f32, col);
    let (full, short) = (14.0, 8.0);
    let widths = if align.justify() { [full, full, full, short] } else { [full, short + 2.0, full, short] };
    for (k, w) in widths.into_iter().enumerate() {
        let y = c.y - 5.0 + k as f32 * 3.33;
        // justified: every line but the last spans the full width
        let line_align = if align.justify() && k < 3 { Align::Min } else { align.halign() };
        let x0 = match line_align {
            Align::Min => c.x - full / 2.0,
            Align::Center => c.x - w / 2.0,
            Align::Max => c.x + full / 2.0 - w,
        };
        painter.hline(x0..=x0 + w, y, s);
    }
}

impl FontCraftApp {
    pub(super) fn family_menu(&self, ui: &mut Ui, fi: usize, acts: &mut Vec<Action>) {
        let targets = self.targets(fi);
        let fam = &self.families[fi];
        let label = if targets.len() > 1 { format!("{} families", targets.len()) } else { fam.name.clone() };
        ui.label(RichText::new(label).strong());
        ui.separator();
        use super::shortcuts::{label as key, Cmd};
        let item = |ui: &mut Ui, text: &str, cmd: Cmd| ui.add(egui::Button::new(text).shortcut_text(key(cmd)));
        if targets.iter().any(|i| !self.families[*i].system) {
            if item(ui, "Activate", Cmd::Activate).clicked() {
                acts.push(Action::SetFamilies(targets.clone(), true));
                ui.close_menu();
            }
            if item(ui, "Deactivate", Cmd::Deactivate).clicked() {
                acts.push(Action::SetFamilies(targets.clone(), false));
                ui.close_menu();
            }
        }
        let fav = self.cfg.favorites.contains(&fam.name);
        if item(ui, if fav { "Remove from favorites" } else { "Add to favorites" }, Cmd::Favorite).clicked() {
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
        if item(ui, "Print specimen…", Cmd::Print).clicked() {
            acts.push(Action::Specimen(targets.clone(), fam.name.clone()));
            ui.close_menu();
        }
        if item(ui, "Change family name…", Cmd::Rename).on_hover_text("Group styles into a family of your choice").clicked() {
            acts.push(Action::RenameFamily(targets.clone()));
            ui.close_menu();
        }
        if item(ui, "Copy family name", Cmd::CopyName).clicked() {
            acts.push(Action::Copy(fam.name.clone()));
            ui.close_menu();
        }
        let path = self.faces[fam.primary].meta.path.clone();
        if item(ui, "Show file in folder", Cmd::Reveal).clicked() {
            acts.push(Action::ShowInFolder(path));
            ui.close_menu();
        }
        ui.separator();
        let del = RichText::new("Delete…").color(Color32::from_rgb(0xff, 0x5c, 0x5c));
        let del_key = egui::Button::new(del).shortcut_text(key(Cmd::Delete));
        if ui.add(del_key).on_hover_text("Delete the font files (FontCraft asks first)").clicked() {
            acts.push(Action::Delete(targets.clone()));
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
            if icon_button(ui, &p, sz, Icon::Grid, self.cfg.view_mode == config::ViewMode::Grid, "Tile view").clicked() {
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
            // text alignment: a menu of all options; the icon shows the one in use
            let (ar, aresp) = ui.allocate_exact_size(sz, Sense::click());
            let popup = Id::new("alignpopup");
            let open = ui.memory(|m| m.is_popup_open(popup));
            if aresp.hovered() || open {
                ui.painter().rect_filled(ar, CornerRadius::same(6), if open { p.sel } else { p.hover });
            }
            let col = if aresp.hovered() || open { p.text } else { p.weak };
            paint_align_icon(ui.painter(), ar, self.cfg.align, col);
            if aresp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
            // (for a picture of the menu in a check)
            if self.shot.is_some() && std::env::var("FONTCRAFT_SHOW").map(|s| s.contains("alignmenu")).unwrap_or(false) {
                ui.memory_mut(|m| m.open_popup(popup));
            }
            let aresp = aresp.on_hover_text(format!("Text alignment: {}", self.cfg.align.label()));
            egui::popup_below_widget(ui, popup, &aresp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(230.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                for a in config::TextAlign::ALL {
                    let chosen = self.cfg.align == a;
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                    if chosen || resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(5), if chosen { p.sel } else { p.hover });
                    }
                    let ir = Rect::from_center_size(pos2(r.left() + 16.0, r.center().y), vec2(24.0, 24.0));
                    paint_align_icon(ui.painter(), ir, a, if chosen { p.accent } else { p.text });
                    ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, a.label(), FontId::proportional(13.0), p.text);
                    if resp.clicked() {
                        self.cfg.align = a;
                        self.dirty = true;
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
            });
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
            if ui.small_button(RichText::new("Delete…").color(Color32::from_rgb(0xff, 0x5c, 0x5c))).clicked() {
                acts.push(Action::Delete(sel.clone()));
            }
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
                    View::Smart(_) => (
                        "No font matches this smart filter",
                        "Right-click the smart filter in the sidebar and choose “Edit…” to change its rules.",
                    ),
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
                    if ui.add(egui::Button::new(RichText::new("Add folder…").color(p.on_accent)).fill(p.accent)).clicked() {
                        acts.push(Action::AddFolderDialog);
                    }
                    if ui.button("Import fonts…").clicked() {
                        acts.push(Action::ImportDialog);
                    }
                });
            }
        });
    }

    pub(super) fn preview_colors(&self) -> (Option<Color32>, Color32) {
        let p = self.p();
        if self.cfg.invert_preview {
            (Some(p.text), p.content)
        } else {
            (None, p.text)
        }
    }

    /// A scroll area for the font list that moves along when the arrow keys reach an entry outside
    /// of the visible part (`row`: the row of that entry, `row_h`: the height of a row).
    fn follow_area(&mut self, salt: &str, row_h: f32, rows_per_entry: usize) -> egui::ScrollArea {
        let mut area = egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]);
        if let Some(k) = self.scroll_to.take() {
            let row = (k / rows_per_entry.max(1)) as f32;
            let (off, view) = self.list_scroll;
            let top = row * row_h;
            if view > 0.0 && top < off {
                area = area.vertical_scroll_offset(top);
            } else if view > 0.0 && top + row_h > off + view {
                area = area.vertical_scroll_offset(top + row_h - view);
            }
        }
        area
    }

    fn list_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let p = self.p();
        let size = self.cfg.preview_size;
        let row_h = (22.0 + size * 1.25 + 6.0).round();
        let text = if self.cfg.preview_text.is_empty() { "The quick brown fox".to_string() } else { self.cfg.preview_text.clone() };
        let (bg, fg) = self.preview_colors();
        let n = self.visible.len();
        let align = self.cfg.align;
        let area = self.follow_area("list", row_h, 1);
        let out = area.show_rows(ui, row_h, n, |ui, range| {
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
                        paint_on_baseline(&pp_, prev_rect, &text, FontId::new(size, ff), fg, align, None);
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
        self.list_scroll = (out.state.offset.y, out.inner_rect.height());
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
        let gap = TILE_GAP;
        let (cols, card_w, card_h) = tile_layout(ui.available_width() - 28.0, self.cfg.preview_size);
        let rows = self.visible.len().div_ceil(cols);
        let text = if self.cfg.preview_text.is_empty() { "Aa".to_string() } else { self.cfg.preview_text.clone() };
        let (bg, fg) = self.preview_colors();
        self.tile_cols = cols;
        let area = self.follow_area("grid", card_h + gap, cols);
        let out = area.show_rows(ui, card_h + gap, rows, |ui, range| {
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
                        Stroke::new(1.5_f32, p.accent)
                    } else if resp.hovered() {
                        Stroke::new(1.0_f32, p.weak)
                    } else {
                        Stroke::new(1.0_f32, p.line)
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
                            paint_on_baseline(&pp_, prev, &text, FontId::new(self.cfg.preview_size, ff), fg, self.cfg.align, Some(w));
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
        self.list_scroll = (out.state.offset.y, out.inner_rect.height());
    }
}
