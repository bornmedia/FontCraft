//! The font list's columns (on the information line above each preview, which itself stays as it
//! is) and the collections, tags and folders a search also finds.

use super::*;
use crate::config::{Column, ColumnCfg};
use egui::{pos2, vec2, Align2, FontId, Id, Rect, RichText, Sense, Ui};

/// Where the information line starts in a row (after the activation dot).
pub(super) const INFO_X: f32 = 33.0;
/// Room kept at the row's end for the heart.
const END_ROOM: f32 = 60.0;

/// A day (seconds since 1970) as year-month-day.
pub(super) fn date_of(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    // days since 1970-01-01 to a calendar date (Howard Hinnant's method)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// "Version 2.013;PS 2.013;hotconv 1.0" → "2.013"
fn short_version(v: &str) -> String {
    let v = v.trim();
    let v = v.strip_prefix("Version").or_else(|| v.strip_prefix("version")).unwrap_or(v).trim();
    v.split([';', ' ']).next().unwrap_or("").to_string()
}

/// The visible columns with their x positions on the information line: (column, x, width). The
/// last one reaches to the end of the line.
pub(super) fn layout(cols: &[ColumnCfg], left: f32, right: f32) -> Vec<(Column, f32, f32)> {
    let shown: Vec<&ColumnCfg> = cols.iter().filter(|c| c.shown).collect();
    let mut x = left;
    let mut out = vec![];
    for (k, c) in shown.iter().enumerate() {
        let room = right - x;
        // a column that has no real room left is left out instead of squeezed onto the icons
        if room < 50.0 && !out.is_empty() {
            break;
        }
        let w = if k + 1 == shown.len() { room.max(40.0) } else { c.width.min(room.max(40.0)) };
        out.push((c.column, x, w));
        x += w;
    }
    out
}

impl FontCraftApp {
    /// The text of a column for a family (not for Name and Tags, which are drawn their own way).
    pub(super) fn column_text(&self, fi: usize, col: Column) -> String {
        let fam = &self.families[fi];
        let m = &self.faces[fam.primary].meta;
        let dash = || "—".to_string();
        let or_dash = |s: &str| if s.trim().is_empty() { dash() } else { s.trim().to_string() };
        match col {
            Column::Styles => fam.faces.len().to_string(),
            Column::Format => fam.ext.clone(),
            Column::Category => {
                let names = [("serif", "Serif"), ("sans", "Sans Serif"), ("script", "Script"), ("display", "Display"), ("symbol", "Symbol")];
                if fam.mono {
                    "Monospace".into()
                } else {
                    names.iter().find(|(t, _)| fam.auto_tags.contains(t)).map(|(_, n)| n.to_string()).unwrap_or_else(dash)
                }
            }
            Column::Source => {
                if fam.system {
                    "Installed".into()
                } else if let Some(p) = Provider::ALL.iter().find(|p| m.path.starts_with(p.fonts_dir())) {
                    p.short().into()
                } else {
                    let f = self.faces[fam.primary].folder.and_then(|i| self.cfg.folders.get(i));
                    match f {
                        Some(f) if *f == self.cfg.import_dir() => "Imported fonts".into(),
                        Some(f) => f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(dash),
                        None => dash(),
                    }
                }
            }
            Column::Designer => or_dash(&m.designer),
            Column::Foundry => or_dash(&m.manufacturer),
            Column::Languages => {
                let names: Vec<&str> = crate::library::LANGUAGES
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| fam.scripts & (1 << k) != 0)
                    .map(|(_, (n, _))| n.split(" (").next().unwrap_or(n))
                    .collect();
                if names.is_empty() {
                    dash()
                } else {
                    names.join(", ")
                }
            }
            Column::Version => or_dash(&short_version(&m.version)),
            Column::Added => {
                if fam.added == 0 {
                    dash()
                } else {
                    date_of(fam.added)
                }
            }
            Column::Name | Column::Tags => String::new(),
        }
    }

    /// The information line of a list row: every visible column, clipped to its width.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_info_line(&self, painter: &egui::Painter, rect: Rect, hy: f32, fi: usize, hcol: Color32, p: &Pal) {
        let fam = &self.families[fi];
        let left = rect.left() + INFO_X;
        for (col, x, w) in layout(&self.cfg.columns, left, rect.right() - END_ROOM) {
            let clip = painter.with_clip_rect(Rect::from_min_max(pos2(x, rect.top()), pos2(x + w - 10.0, hy + 10.0)).intersect(painter.clip_rect()));
            match col {
                Column::Name => {
                    let nr = clip.text(pos2(x, hy), Align2::LEFT_CENTER, &fam.name, FontId::proportional(11.5), hcol.gamma_multiply(0.9));
                    // the same style in more than one file (all copies are listed in the details)
                    let dups = fam.files.len().saturating_sub(fam.faces.len());
                    if dups > 0 {
                        let t = if dups == 1 { "1 duplicate".to_string() } else { format!("{dups} duplicates") };
                        clip.text(pos2(nr.right() + 8.0, hy), Align2::LEFT_CENTER, t, FontId::proportional(10.5), p.accent);
                    }
                }
                Column::Tags => {
                    // your own tags first (as chips), then the ones FontCraft worked out
                    let mut tx = x;
                    for (t, set) in &self.cfg.tags {
                        if !set.contains(&fam.name) {
                            continue;
                        }
                        let g = clip.layout_no_wrap(t.clone(), FontId::proportional(10.5), hcol);
                        let chip = Rect::from_min_size(pos2(tx, hy - 8.0), vec2(g.size().x + 12.0, 16.0));
                        clip.rect_filled(chip, CornerRadius::same(8), p.accent.gamma_multiply(0.25));
                        clip.galley(pos2(tx + 6.0, hy - g.size().y / 2.0), g, hcol);
                        tx = chip.right() + 5.0;
                    }
                    for t in &fam.auto_tags {
                        let r = clip.text(pos2(tx, hy), Align2::LEFT_CENTER, *t, FontId::proportional(10.5), hcol.gamma_multiply(0.45));
                        tx = r.right() + 7.0;
                    }
                }
                Column::Styles => {
                    clip.text(pos2(x, hy), Align2::LEFT_CENTER, self.column_text(fi, col), FontId::proportional(11.0), hcol);
                }
                _ => {
                    clip.text(pos2(x, hy), Align2::LEFT_CENTER, self.column_text(fi, col), FontId::proportional(10.5), hcol.gamma_multiply(0.6));
                }
            }
        }
    }

    /// The column titles above the list: drag a title's right edge to make it wider or narrower,
    /// right-click (or the ⋯ at the end) to choose and order the columns.
    pub(super) fn columns_header(&mut self, ui: &mut Ui, p: &Pal) {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        let painter = ui.painter_at(rect);
        painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0_f32, p.line));
        let left = rect.left() + INFO_X;
        let right = rect.right() - END_ROOM;
        let cols = layout(&self.cfg.columns, left, right);
        let n = cols.len();
        let mut resize: Option<(Column, f32)> = None;
        for (k, (col, x, w)) in cols.into_iter().enumerate() {
            let clip = painter.with_clip_rect(Rect::from_min_max(pos2(x, rect.top()), pos2(x + w - 10.0, rect.bottom())));
            clip.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, col.title().to_uppercase(), FontId::proportional(10.0), p.weak);
            // the edge between this column and the next one
            if k + 1 < n {
                let edge = Rect::from_center_size(pos2(x + w - 4.0, rect.center().y), vec2(9.0, rect.height()));
                let er = ui.interact(edge, Id::new(("colresize", col as u8)), Sense::drag());
                let hot = er.hovered() || er.dragged();
                painter.vline(x + w - 4.0, rect.y_range().shrink(4.0), Stroke::new(if hot { 2.0_f32 } else { 1.0_f32 }, if hot { p.accent } else { p.line }));
                if hot {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if er.dragged() {
                    resize = Some((col, er.drag_delta().x));
                }
            }
        }
        if let Some((col, dx)) = resize {
            if let Some(c) = self.cfg.columns.iter_mut().find(|c| c.column == col) {
                c.width = (c.width + dx).clamp(40.0, 700.0);
                self.dirty = true;
            }
        }
        // the column menu: at the ⋯ and on right-click
        let mr = Rect::from_center_size(pos2(rect.right() - 24.0, rect.center().y), vec2(26.0, 20.0));
        let mresp = ui.interact(mr, Id::new("colmenu-btn"), Sense::click());
        painter.text(mr.center(), Align2::CENTER_CENTER, "⋯", FontId::proportional(14.0), if mresp.hovered() { p.text } else { p.weak });
        let popup = Id::new("colmenu");
        if mresp.clicked() || resp.secondary_clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup));
        }
        if self.shot.is_some() && std::env::var("FONTCRAFT_SHOW").map(|s| s.contains("colmenu")).unwrap_or(false) {
            ui.memory_mut(|m| m.open_popup(popup));
        }
        let mresp = mresp.on_hover_text("Choose the columns (right-click the titles, too). Drag a title's edge to resize it.");
        egui::popup_below_widget(ui, popup, &mresp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            self.columns_menu(ui, p);
        });
    }

    fn columns_menu(&mut self, ui: &mut Ui, p: &Pal) {
        ui.set_min_width(250.0);
        ui.label(RichText::new("COLUMNS").size(11.0).color(p.weak));
        ui.label(RichText::new("Shown on the line above each preview").size(11.0).color(p.weak));
        let mut swap: Option<(usize, usize)> = None;
        let len = self.cfg.columns.len();
        for k in 0..len {
            let c = self.cfg.columns[k];
            ui.horizontal(|ui| {
                let mut on = c.shown;
                let fixed = c.column == Column::Name;
                if ui.add_enabled(!fixed, egui::Checkbox::new(&mut on, c.column.title())).changed() {
                    self.cfg.columns[k].shown = on;
                    self.dirty = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(k + 1 < len, egui::Button::new("↓").small()).on_hover_text("Further right").clicked() {
                        swap = Some((k, k + 1));
                    }
                    if ui.add_enabled(k > 0, egui::Button::new("↑").small()).on_hover_text("Further left").clicked() {
                        swap = Some((k, k - 1));
                    }
                });
            });
        }
        if let Some((a, b)) = swap {
            self.cfg.columns.swap(a, b);
            self.dirty = true;
        }
        ui.separator();
        if ui.button("Back to the standard columns").clicked() {
            self.cfg.columns = Column::defaults();
            self.dirty = true;
        }
    }

    /// While searching: the one or two best matching collections, tags and folders, above the
    /// fonts (which keep their order). Nothing when none of them matches.
    pub(super) fn search_groups(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() || !self.cfg.search_groups {
            return;
        }
        // the best first: a name that starts with the search, then the shorter name
        let rank = |name: &str| -> Option<(bool, usize)> {
            let l = name.to_lowercase();
            l.contains(&q).then(|| (!l.starts_with(&q), l.len()))
        };
        let mut colls: Vec<((bool, usize), View, String, usize)> = self
            .cfg
            .collections
            .iter()
            .filter_map(|c| rank(&c.name).map(|r| (r, View::Collection(c.id), c.name.clone(), c.families.len())))
            .collect();
        let mut tags: Vec<((bool, usize), View, String, usize)> = self
            .cfg
            .tags
            .iter()
            .filter_map(|(t, s)| rank(t).map(|r| (r, View::Tag(t.clone()), format!("#{t}"), s.len())))
            .collect();
        let mut folders: Vec<((bool, usize), View, String, usize)> =
            self.nodes.iter().filter_map(|n| rank(&n.name).map(|r| (r, View::Folder(n.path.clone()), n.name.clone(), n.fams.len()))).collect();
        for v in [&mut colls, &mut tags, &mut folders] {
            v.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.to_lowercase().cmp(&b.2.to_lowercase())));
            v.truncate(2);
        }
        if colls.is_empty() && tags.is_empty() && folders.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.add_space(14.0);
            for (title, list) in [("COLLECTIONS", colls), ("TAGS", tags), ("FOLDERS", folders)] {
                if list.is_empty() {
                    continue;
                }
                ui.label(RichText::new(title).size(10.5).strong().color(p.weak));
                for (_, view, name, count) in list {
                    let chip = egui::Button::new(RichText::new(format!("{name}  {count}")).size(12.0).color(p.text))
                        .fill(p.input)
                        .corner_radius(CornerRadius::same(10));
                    if ui.add(chip).on_hover_text("Show it").clicked() {
                        acts.push(Action::SetView(view));
                        self.search.clear();
                    }
                }
                ui.add_space(12.0);
            }
        });
        ui.add_space(4.0);
    }
}
