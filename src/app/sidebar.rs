//! The sidebar: library views, collections, tags, system fonts and the folder tree.

use super::ui::{paint_dot, paint_icon, Icon};
use super::*;
use egui::{pos2, vec2, Align2, FontId, Id, Rect, RichText, Sense, StrokeKind, Ui};

pub(super) const SIDEBAR_MIN: f32 = 170.0;
pub(super) const SIDEBAR_MAX: f32 = 460.0;
pub(super) const DETAILS_MIN: f32 = 300.0;
pub(super) const DETAILS_MAX: f32 = 1000.0;

/// Is the mouse button held down (a panel edge still being dragged)? The width is saved once it's let go.
fn ctx_dragging(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.pointer.any_down())
}

impl FontCraftApp {
    #[allow(clippy::too_many_arguments)]
    fn side_item(
        ui: &mut Ui,
        p: &Pal,
        id: Id,
        depth: usize,
        dot: Option<((usize, usize), Option<Color32>)>,
        label: &str,
        count: Option<usize>,
        selected: bool,
        arrow: Option<bool>,
    ) -> (egui::Response, Option<egui::Response>, Option<egui::Response>) {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        let drop_hover = resp.dnd_hover_payload::<DragFams>().is_some();
        if selected || drop_hover {
            ui.painter().rect_filled(rect, CornerRadius::same(5), p.sel);
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(5), p.hover);
        }
        if drop_hover {
            ui.painter().rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.2_f32, p.accent), StrokeKind::Inside);
        }
        let x0 = rect.left() + 8.0 + depth as f32 * 10.0;
        let mut arrow_resp = None;
        if let Some(open) = arrow {
            let ar = Rect::from_center_size(pos2(x0 - 6.0, rect.center().y), vec2(12.0, 18.0));
            let r = ui.interact(ar, id.with("arrow"), Sense::click());
            let c = ar.center();
            let col = if r.hovered() { p.text } else { p.weak };
            let pts = if open {
                vec![c + vec2(-3.0, -1.5), c + vec2(3.0, -1.5), c + vec2(0.0, 2.0)]
            } else {
                vec![c + vec2(-1.5, -3.0), c + vec2(2.0, 0.0), c + vec2(-1.5, 3.0)]
            };
            ui.painter().add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
            arrow_resp = Some(r);
        }
        let mut dot_resp = None;
        let mut tx = x0 + 2.0;
        if let Some((state, color)) = dot {
            let dc = pos2(x0 + 6.0, rect.center().y);
            let dr = Rect::from_center_size(dc, vec2(16.0, 18.0));
            let r = ui.interact(dr, id.with("dot"), Sense::click());
            let rad = if r.hovered() { 5.0 } else { 4.5 };
            match color {
                Some(c) => {
                    let pp = Pal { accent: c, ..*p };
                    paint_dot(ui.painter(), dc, state, &pp, rad);
                }
                None => paint_dot(ui.painter(), dc, state, p, rad),
            }
            let (a, n) = state;
            dot_resp = Some(r.on_hover_text(if n > 0 && a == n { "Switch all off" } else { "Switch all on" }));
            tx = x0 + 16.0;
        }
        let mut right = rect.right() - 8.0;
        if let Some(c) = count {
            let r = ui.painter().text(pos2(right, rect.center().y), Align2::RIGHT_CENTER, c.to_string(), FontId::proportional(11.0), p.weak);
            right = r.left() - 6.0;
        }
        let clip = ui.painter().with_clip_rect(Rect::from_min_max(rect.min, pos2(right, rect.max.y)));
        clip.text(pos2(tx, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), p.text);
        (resp, dot_resp, arrow_resp)
    }

    fn section_header(&mut self, ui: &mut Ui, key: &str, title: &str, plus: Option<&str>) -> (bool, bool) {
        let p = self.p();
        ui.add_space(8.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
        let open = !self.cfg.collapsed.contains(key);
        let c = pos2(rect.left() + 12.0, rect.center().y);
        let pts = if open {
            vec![c + vec2(-3.5, -1.5), c + vec2(3.5, -1.5), c + vec2(0.0, 2.5)]
        } else {
            vec![c + vec2(-3.5, 2.0), c + vec2(3.5, 2.0), c + vec2(0.0, -2.0)]
        };
        ui.painter().add(egui::Shape::convex_polygon(pts, p.text, Stroke::NONE));
        ui.painter().text(pos2(rect.left() + 26.0, rect.center().y), Align2::LEFT_CENTER, title, FontId::proportional(12.0), p.text);
        let mut plus_clicked = false;
        if let Some(tip) = plus {
            let pr = Rect::from_center_size(pos2(rect.right() - 12.0, rect.center().y), vec2(20.0, 20.0));
            let pres = ui.interact(pr, Id::new(("plus", key)), Sense::click());
            if resp.hovered() || pres.hovered() {
                paint_icon(ui.painter(), pr, Icon::Plus, if pres.hovered() { p.accent } else { p.weak }, p.side);
            }
            plus_clicked = pres.on_hover_text(tip).clicked();
        }
        if resp.clicked() && !plus_clicked {
            if open {
                self.cfg.collapsed.insert(key.to_string());
            } else {
                self.cfg.collapsed.remove(key);
            }
            self.dirty = true;
        }
        (open, plus_clicked)
    }

    /// None: not renaming this item. Some(None): still typing (or left empty). Some(Some(name)): done.
    fn rename_field(&mut self, ui: &mut Ui, key: &str) -> Option<Option<String>> {
        let (k, buf) = self.renaming.as_mut()?;
        if k != key {
            return None;
        }
        let r = ui.add(egui::TextEdit::singleline(buf).desired_width(f32::INFINITY));
        r.request_focus();
        if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let new = buf.trim().to_string();
            self.renaming = None;
            self.dirty = true;
            return Some(if new.is_empty() { None } else { Some(new) });
        }
        Some(None)
    }

    fn hint_line(ui: &mut Ui, p: &Pal, text: &str) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(26.0);
            ui.label(RichText::new(text).size(11.5).color(p.weak));
        });
    }

    /// "Print…", "Save as PDF…" and the specimen sheet, for a collection, tag or smart filter.
    fn print_items(ui: &mut Ui, fams: &[usize], title: &str, acts: &mut Vec<Action>) {
        if ui.button("Print…").on_hover_text("A PDF of all its fonts with your preview text, size and colours").clicked() {
            acts.push(Action::PrintPdf(fams.to_vec(), title.to_string(), false));
            ui.close_menu();
        }
        if ui.button("Save as PDF…").clicked() {
            acts.push(Action::PrintPdf(fams.to_vec(), title.to_string(), true));
            ui.close_menu();
        }
        if ui.button("Specimen sheet (every style)…").on_hover_text("Opens in your browser").clicked() {
            acts.push(Action::Specimen(fams.to_vec(), title.to_string()));
            ui.close_menu();
        }
    }

    fn collection_menu(&mut self, ui: &mut Ui, id: u64, name: &str, synced: bool, fams: &[usize], acts: &mut Vec<Action>) {
        if ui.button("Rename").clicked() {
            self.renaming = Some((format!("c:{id}"), name.to_string()));
            ui.close_menu();
        }
        ui.menu_button("Colour", |ui| {
            for c in COLLECTION_COLORS {
                let (rr, resp) = ui.allocate_exact_size(vec2(90.0, 18.0), Sense::click());
                ui.painter().rect_filled(rr, CornerRadius::same(4), Color32::from_rgb(c[0], c[1], c[2]));
                if resp.clicked() {
                    if let Some(cc) = self.cfg.collections.iter_mut().find(|x| x.id == id) {
                        cc.color = c;
                    }
                    self.dirty = true;
                    ui.close_menu();
                }
            }
        });
        ui.separator();
        if ui.button("Switch all on").clicked() {
            acts.push(Action::SetFamilies(fams.to_vec(), true));
            ui.close_menu();
        }
        if ui.button("Switch all off").clicked() {
            acts.push(Action::SetFamilies(fams.to_vec(), false));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Export fonts to folder…").clicked() {
            acts.push(Action::ExportCollection(id));
            ui.close_menu();
        }
        if synced {
            // older per-collection folder copies can still be switched off
            if ui.button("Stop syncing to folder").clicked() {
                acts.push(Action::SyncCollection(id, false));
                ui.close_menu();
            }
        }
        Self::print_items(ui, fams, name, acts);
        if ui.button("Export CSS…").clicked() {
            acts.push(Action::ExportCss(fams.to_vec(), name.to_string()));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Delete collection").clicked() {
            acts.push(Action::DeleteCollection(id));
            ui.close_menu();
        }
    }

    pub(super) fn sidebar(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let p = self.p();
        // drag its right edge to make it wider or narrower; the width is remembered
        let resp = egui::SidePanel::left("side")
            .default_width(self.cfg.sidebar_w.clamp(SIDEBAR_MIN, SIDEBAR_MAX))
            .width_range(SIDEBAR_MIN..=SIDEBAR_MAX)
            .resizable(true)
            .frame(egui::Frame::new().fill(p.side).inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 6 }))
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("sideadd")
                    .exact_height(34.0)
                    .frame(egui::Frame::new().fill(p.side))
                    .show_inside(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(4.0);
                            ui.menu_button(RichText::new("➕  Add").size(12.5).color(p.text), |ui| self.app_menu(ui, acts))
                                .response
                                .on_hover_text("Add folders, import fonts, new collections and tags");
                        });
                    });
                self.sidebar_body(ui, &p, acts, false, f32::INFINITY);
            });
        self.remember_width(resp.response.rect.width(), true);
    }

    /// The row above "All": on its right the arrow that hides the sidebar (or, in the sidebar that
    /// pops out at the window's edge, keeps it open again). It shows while the mouse is over the sidebar.
    fn sidebar_top_row(&mut self, ui: &mut Ui, p: &Pal, peek: bool) {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let over = ui.ctx().pointer_hover_pos().map(|pp| ui.clip_rect().contains(pp)).unwrap_or(false) || self.demo_side;
        let ar = Rect::from_center_size(pos2(row.right() - 12.0, row.center().y), vec2(24.0, 22.0));
        let resp = ui.interact(ar, Id::new(("side-hide", peek)), Sense::click());
        if over || peek {
            if resp.hovered() {
                ui.painter().rect_filled(ar, CornerRadius::same(5), p.hover);
            }
            let col = if resp.hovered() { p.text } else { p.weak };
            // two chevrons: « hides the sidebar, » keeps it open
            let dir = if peek { 1.0 } else { -1.0 };
            let c = ar.center();
            for dx in [-3.0, 2.0] {
                let tip = c + vec2(dx + dir * 2.5, 0.0);
                let back = c + vec2(dx - dir * 2.5, 0.0);
                ui.painter().line_segment([pos2(back.x, c.y - 4.5), tip], Stroke::new(1.5_f32, col));
                ui.painter().line_segment([pos2(back.x, c.y + 4.5), tip], Stroke::new(1.5_f32, col));
            }
        }
        let tip = if peek {
            "Keep the sidebar open".to_string()
        } else {
            format!(
                "Hide the sidebar ({}). It comes back when the mouse goes to the left edge of the window.",
                super::shortcuts::label(super::shortcuts::Cmd::Sidebar)
            )
        };
        if resp.on_hover_text(tip).clicked() {
            self.sidebar_open = peek;
            self.side_peek = false;
        }
    }

    /// While the sidebar is hidden: it pops out over the list when the mouse reaches the left edge of
    /// the window, and goes away again when the mouse leaves it.
    pub(super) fn sidebar_peek(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let p = self.p();
        let area = ctx.available_rect();
        let pointer = ctx.pointer_hover_pos();
        let strip = Rect::from_min_max(area.left_top(), pos2(area.left() + 10.0, area.bottom()));
        if pointer.map(|pp| strip.contains(pp)).unwrap_or(false) || self.demo_side {
            self.side_peek = true;
        }
        if !self.side_peek {
            return;
        }
        let w = self.cfg.sidebar_w.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
        let rect = Rect::from_min_size(area.left_top(), vec2(w, area.height()));
        egui::Area::new(Id::new("sidepeek")).order(egui::Order::Foreground).fixed_pos(rect.min).show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.side)
                .stroke(Stroke::new(1.0_f32, p.line))
                .shadow(ctx.style().visuals.popup_shadow)
                .inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 6 })
                .show(ui, |ui| {
                    ui.set_width(w - 12.0);
                    ui.set_height(area.height() - 10.0);
                    self.sidebar_body(ui, &p, acts, true, area.height() - 10.0);
                });
        });
        // it stays while the mouse is over it (with a little room), a menu from it is open, a name is
        // being typed, or fonts are being dragged onto it
        let inside = pointer.map(|pp| rect.expand(14.0).contains(pp)).unwrap_or(false);
        let busy = ctx.memory(|m| m.any_popup_open()) || ctx.is_context_menu_open() || self.renaming.is_some();
        if !inside && !busy && !self.demo_side {
            self.side_peek = false;
        }
    }

    /// What the sidebar shows: the arrow row, the library views, collections, tags, providers and folders.
    fn sidebar_body(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>, peek: bool, max_h: f32) {
        let p = *p;
        self.sidebar_top_row(ui, &p, peek);
        {
            {
                egui::ScrollArea::vertical().id_salt("sidescroll").max_height(max_h - 30.0).auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.sidebar_library(ui, &p, acts);
                    self.sidebar_collections(ui, &p, acts);
                    self.sidebar_smart(ui, &p, acts);
                    self.sidebar_tags(ui, &p, acts);
                    let (open, _) = self.section_header(ui, "sec:providers", "PROVIDERS", None);
                    if open {
                        for prov in Provider::ALL {
                            let prefix = format!("{}/", prov.tag());
                            let all: Vec<usize> = self.gdl.iter().filter(|(k, _)| k.starts_with(&prefix)).flat_map(|(_, v)| v.iter().copied()).collect();
                            let state = if all.is_empty() { (0, 1) } else { self.faces_state(&all) };
                            let n = self.gcat.iter().filter(|g| g.provider == prov).count();
                            let count = if n == 0 { None } else { Some(n) };
                            let view = View::Online(prov);
                            let (r, d, _) = Self::side_item(ui, &p, Id::new(("side-online", prov.tag())), 1, Some((state, None)), prov.short(), count, self.view == view, None);
                            if r.on_hover_text(prov.about()).clicked() {
                                acts.push(Action::SetView(view.clone()));
                            }
                            if d.map(|d| d.clicked()).unwrap_or(false) {
                                if all.is_empty() {
                                    acts.push(Action::SetView(view));
                                } else {
                                    let paths: Vec<String> = all.iter().map(|f| self.faces[*f].path_key.clone()).collect();
                                    acts.push(Action::SetPaths(paths, !(state.0 == state.1)));
                                }
                            }
                        }
                    }
                    if self.cfg.show_system {
                        let (open, _) = self.section_header(ui, "sec:system", "SYSTEM", None);
                        if open {
                            let n = self.families_in_view(&View::Installed).len();
                            let (r, _, _) = Self::side_item(ui, &p, Id::new("side-installed"), 1, Some(((1, 1), None)), "Installed", Some(n), self.view == View::Installed, None);
                            if r.clicked() {
                                acts.push(Action::SetView(View::Installed));
                            }
                        }
                    }
                    self.sidebar_folders(ui, &p, acts);
                    if peek {
                        ui.add_space(8.0);
                        ui.menu_button(RichText::new("➕  Add").size(12.5).color(p.text), |ui| self.app_menu(ui, acts));
                    }
                    ui.add_space(12.0);
                });
            }
        }
    }

    /// Keep the width the person dragged a side panel to (`sidebar`: the left one, else the details).
    pub(super) fn remember_width(&mut self, w: f32, sidebar: bool) {
        let slot = if sidebar { &mut self.cfg.sidebar_w } else { &mut self.cfg.details_w };
        if (*slot - w).abs() > 0.5 && !ctx_dragging(&self.ctx) {
            *slot = w;
            self.dirty = true;
        }
    }

    fn sidebar_library(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        let entries: [(View, bool); 5] =
            [(View::All, false), (View::Recent, false), (View::Favorites, true), (View::Active, true), (View::Inactive, true)];
        for (v, has_dot) in entries {
            let fams = self.families_in_view(&v);
            let state = match v {
                View::Active => (1, 1),
                View::Inactive => (0, 1),
                _ => self.fams_state(&fams),
            };
            let title = self.view_title(&v);
            let dot = if has_dot { Some((state, None)) } else { None };
            let (r, d, _) = Self::side_item(ui, p, Id::new(("side", format!("{v:?}"))), 0, dot, &title, Some(fams.len()), self.view == v, None);
            if r.clicked() {
                acts.push(Action::SetView(v.clone()));
            }
            if let Some(d) = d {
                if d.clicked() {
                    if v == View::Favorites {
                        acts.push(Action::SetFamilies(fams.clone(), !(state.0 == state.1 && state.1 > 0)));
                    } else {
                        acts.push(Action::SetView(v.clone()));
                    }
                }
            }
            if v == View::Favorites {
                if let Some(pl) = r.dnd_release_payload::<DragFams>() {
                    let add: Vec<usize> =
                        pl.0.iter().copied().filter(|i| !self.cfg.favorites.contains(&self.families[*i].name)).collect();
                    if !add.is_empty() {
                        acts.push(Action::ToggleFavorite(add));
                    }
                }
            }
        }
        let (r, _, _) = Self::side_item(ui, p, Id::new("side-finder"), 0, None, "Font finder", None, self.view == View::Finder, None);
        if r.on_hover_text("Find the font in a picture").clicked() {
            acts.push(Action::SetView(View::Finder));
        }
    }

    fn sidebar_collections(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        let (open, plus) = self.section_header(ui, "sec:collections", "COLLECTIONS", Some("New collection"));
        if plus {
            acts.push(Action::NewCollection(vec![]));
        }
        if !open {
            return;
        }
        if self.cfg.collections.is_empty() {
            Self::hint_line(ui, p, "Press + and drag fonts here");
        }
        let cols: Vec<(u64, String, [u8; 3], bool)> =
            self.cfg.collections.iter().map(|c| (c.id, c.name.clone(), c.color, c.sync_dir.is_some())).collect();
        for (id, name, color, synced) in cols {
            if let Some(res) = self.rename_field(ui, &format!("c:{id}")) {
                if let Some(new) = res {
                    if let Some(c) = self.cfg.collections.iter_mut().find(|c| c.id == id) {
                        c.name = new;
                    }
                }
                continue;
            }
            let v = View::Collection(id);
            let fams = self.families_in_view(&v);
            let state = self.fams_state(&fams);
            let col = Color32::from_rgb(color[0], color[1], color[2]);
            let label = if synced { format!("{name} (copied)") } else { name.clone() };
            let (r, d, _) = Self::side_item(ui, p, Id::new(("coll", id)), 1, Some((state, Some(col))), &label, Some(fams.len()), self.view == v, None);
            if d.map(|d| d.clicked()).unwrap_or(false) {
                acts.push(Action::SetFamilies(fams.clone(), !(state.0 == state.1 && state.1 > 0)));
            }
            if r.clicked() {
                acts.push(Action::SetView(v.clone()));
            }
            if let Some(pl) = r.dnd_release_payload::<DragFams>() {
                acts.push(Action::AddToCollection(pl.0.clone(), id));
            }
            r.context_menu(|ui| self.collection_menu(ui, id, &name, synced, &fams, acts));
        }
    }

    fn sidebar_smart(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        let (open, plus) = self.section_header(ui, "sec:smart", "SMART FILTERS", Some("New smart filter"));
        if plus {
            acts.push(Action::NewSmart);
        }
        if !open {
            return;
        }
        if self.cfg.smart_filters.is_empty() {
            Self::hint_line(ui, p, "Saved searches: language,\ncategory, activation… Press +");
        }
        let list: Vec<(u64, String)> = self.cfg.smart_filters.iter().map(|s| (s.id, s.name.clone())).collect();
        for (id, name) in list {
            let v = View::Smart(id);
            let fams = self.families_in_view(&v);
            let state = self.fams_state(&fams);
            let (r, d, _) = Self::side_item(ui, p, Id::new(("smart", id)), 1, Some((state, None)), &name, Some(fams.len()), self.view == v, None);
            if d.map(|d| d.clicked()).unwrap_or(false) {
                acts.push(Action::SetFamilies(fams.clone(), !(state.0 == state.1 && state.1 > 0)));
            }
            if r.clicked() {
                acts.push(Action::SetView(v.clone()));
            }
            if r.double_clicked() {
                acts.push(Action::EditSmart(id));
            }
            r.context_menu(|ui| {
                if ui.button("Edit…").clicked() {
                    acts.push(Action::EditSmart(id));
                    ui.close_menu();
                }
                if ui.button("Duplicate…").clicked() {
                    acts.push(Action::DuplicateSmart(id));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Switch all on").clicked() {
                    acts.push(Action::SetFamilies(fams.clone(), true));
                    ui.close_menu();
                }
                if ui.button("Switch all off").clicked() {
                    acts.push(Action::SetFamilies(fams.clone(), false));
                    ui.close_menu();
                }
                ui.separator();
                Self::print_items(ui, &fams, &name, acts);
                ui.separator();
                if ui.button("Delete smart filter").on_hover_text("The fonts stay where they are").clicked() {
                    acts.push(Action::DeleteSmart(id));
                    ui.close_menu();
                }
            });
        }
    }

    fn sidebar_tags(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        let (open, plus) = self.section_header(ui, "sec:tags", "TAGS", Some("New tag"));
        if plus {
            acts.push(Action::NewTag(vec![]));
        }
        if !open {
            return;
        }
        if self.cfg.tags.is_empty() {
            Self::hint_line(ui, p, "Label fonts: serif, logo, client…");
        }
        let tags: Vec<String> = self.cfg.tags.keys().cloned().collect();
        for t in tags {
            let key = format!("t:{t}");
            if let Some(res) = self.rename_field(ui, &key) {
                if let Some(new) = res {
                    if new != t && !self.cfg.tags.contains_key(&new) {
                        if let Some(set) = self.cfg.tags.remove(&t) {
                            self.cfg.tags.insert(new.clone(), set);
                            if self.view == View::Tag(t.clone()) {
                                self.view = View::Tag(new);
                            }
                        }
                    }
                }
                continue;
            }
            let v = View::Tag(t.clone());
            let fams = self.families_in_view(&v);
            let state = self.fams_state(&fams);
            let (r, d, _) = Self::side_item(ui, p, Id::new(("tag", &t)), 1, Some((state, None)), &format!("#{t}"), Some(fams.len()), self.view == v, None);
            if d.map(|d| d.clicked()).unwrap_or(false) {
                acts.push(Action::SetFamilies(fams.clone(), !(state.0 == state.1 && state.1 > 0)));
            }
            if r.clicked() {
                acts.push(Action::SetView(v.clone()));
            }
            if let Some(pl) = r.dnd_release_payload::<DragFams>() {
                acts.push(Action::AddTag(pl.0.clone(), t.clone()));
            }
            r.context_menu(|ui| {
                if ui.button("Rename").clicked() {
                    self.renaming = Some((key.clone(), t.clone()));
                    ui.close_menu();
                }
                if ui.button("Switch all on").clicked() {
                    acts.push(Action::SetFamilies(fams.clone(), true));
                    ui.close_menu();
                }
                if ui.button("Switch all off").clicked() {
                    acts.push(Action::SetFamilies(fams.clone(), false));
                    ui.close_menu();
                }
                ui.separator();
                Self::print_items(ui, &fams, &format!("#{t}"), acts);
                ui.separator();
                if ui.button("Delete tag").clicked() {
                    acts.push(Action::DeleteTag(t.clone()));
                    ui.close_menu();
                }
            });
        }
    }

    fn sidebar_folders(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        let (open, plus) = self.section_header(ui, "sec:folders", "FOLDERS", Some("Add a font folder"));
        if plus {
            acts.push(Action::AddFolderDialog);
        }
        if !open {
            return;
        }
        if self.cfg.folders.is_empty() {
            Self::hint_line(ui, p, "Drop a folder of fonts here\nor press + to add one");
        }
        for ni in 0..self.nodes.len() {
            let n = &self.nodes[ni];
            if n.ancestors.iter().any(|a| self.cfg.collapsed.contains(&format!("dir:{}", self.nodes[*a].path.display()))) {
                continue;
            }
            let key = format!("dir:{}", n.path.display());
            let is_open = !self.cfg.collapsed.contains(&key);
            let state = self.faces_state(&n.faces);
            let v = View::Folder(n.path.clone());
            let paths: Vec<String> = n.faces.iter().map(|f| self.faces[*f].path_key.clone()).collect();
            let (path, depth, root, mut name, count, kids) = (n.path.clone(), n.depth, n.root, n.name.clone(), n.fams.len(), n.has_children);
            let net = depth == 0 && self.is_net_folder(&path);
            let away = depth == 0 && self.offline.contains(&path);
            if away {
                name.push_str("  · offline");
            } else if net {
                name.push_str("  · network");
            }
            let (r, d, a) = Self::side_item(
                ui,
                p,
                Id::new(("node", &path)),
                depth + 1,
                Some((state, None)),
                &name,
                Some(count),
                self.view == v,
                if kids { Some(is_open) } else { None },
            );
            if a.map(|a| a.clicked()).unwrap_or(false) {
                if is_open {
                    self.cfg.collapsed.insert(key.clone());
                } else {
                    self.cfg.collapsed.remove(&key);
                }
                self.dirty = true;
            }
            if d.map(|d| d.clicked()).unwrap_or(false) {
                acts.push(Action::SetPaths(paths.clone(), !(state.0 == state.1 && state.1 > 0)));
            }
            if r.clicked() {
                acts.push(Action::SetView(v.clone()));
            }
            let tip = if away {
                format!("{}\nCan't be reached right now. Its active fonts come back when it's online again.", path.display())
            } else if net {
                format!("Network folder: {}", path.display())
            } else {
                path.display().to_string()
            };
            let r = r.on_hover_text(tip);
            r.context_menu(|ui| {
                if ui.button("Switch all fonts in folder on").clicked() {
                    acts.push(Action::SetPaths(paths.clone(), true));
                    ui.close_menu();
                }
                if ui.button("Switch all fonts in folder off").clicked() {
                    acts.push(Action::SetPaths(paths.clone(), false));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Rescan").clicked() {
                    acts.push(Action::Rescan);
                    ui.close_menu();
                }
                if ui.button("Show in file browser").clicked() {
                    acts.push(Action::ShowInFolder(path.clone()));
                    ui.close_menu();
                }
                if depth == 0 {
                    ui.separator();
                    if ui.button("Remove folder from FontCraft").on_hover_text("The font files stay on your disk").clicked() {
                        acts.push(Action::RemoveFolder(root));
                        ui.close_menu();
                    }
                }
            });
        }
        // network folders: a share on a server, NAS or another computer
        ui.add_space(2.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
        let col = if resp.hovered() { p.accent } else { p.weak };
        ui.painter().text(
            pos2(rect.left() + 26.0, rect.center().y),
            Align2::LEFT_CENTER,
            "+ Add a network folder…",
            FontId::proportional(12.0),
            col,
        );
        if resp.on_hover_text("A shared folder on a server, a NAS or another computer").clicked() {
            acts.push(Action::NetworkFolderDialog);
        }
    }
}
