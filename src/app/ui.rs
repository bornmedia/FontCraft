//! Shared drawing helpers plus the top bar, icon rail and preview-text bar.

use super::*;
use egui::{pos2, vec2, Align, Align2, FontId, Id, Layout, Painter, Rect, RichText, Sense, StrokeKind, Ui};

pub(super) const SIDEBAR_W: f32 = 200.0;
const RAIL_W: f32 = 44.0;

pub(super) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Active indicator: filled dot = active, ring in accent = partly active, plain ring = inactive.
pub(super) fn paint_dot(painter: &Painter, c: egui::Pos2, state: (usize, usize), p: &Pal, r: f32) {
    let (a, n) = state;
    if n > 0 && a == n {
        painter.circle_filled(c, r, p.accent);
    } else if a > 0 {
        painter.circle_stroke(c, r - 0.6, Stroke::new(1.6_f32, p.accent));
    } else {
        painter.circle_stroke(c, r - 0.6, Stroke::new(1.4_f32, p.text));
    }
}

pub(super) fn paint_toggle(ui: &Ui, rect: Rect, t: f32, p: &Pal) {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((rect.height() / 2.0) as u8), lerp_color(p.toggle_off, p.accent, t));
    let r = rect.height() / 2.0 - 2.5;
    let cx = rect.left() + r + 2.5 + (rect.width() - 2.0 * r - 5.0) * t;
    painter.circle_filled(pos2(cx, rect.center().y), r, Color32::WHITE);
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Lines,
    List,
    Grid,
    Invert,
    Align,
    Reset,
    Folder,
    Theme,
    Gear,
    Help,
    Filter,
    Plus,
    Finder,
}

pub(super) fn paint_icon(painter: &Painter, r: Rect, icon: Icon, col: Color32, bg: Color32) {
    let c = r.center();
    let s = Stroke::new(1.4_f32, col);
    match icon {
        Icon::Lines => {
            for dy in [-5.0, 0.0, 5.0] {
                painter.hline(c.x - 7.0..=c.x + 7.0, c.y + dy, s);
            }
        }
        Icon::List => {
            for dy in [-4.0, 0.0, 4.0] {
                painter.rect_filled(Rect::from_center_size(pos2(c.x, c.y + dy), vec2(14.0, 2.0)), CornerRadius::ZERO, col);
            }
        }
        Icon::Grid => {
            for (dx, dy) in [(-3.5, -3.5), (3.5, -3.5), (-3.5, 3.5), (3.5, 3.5)] {
                painter.rect_filled(Rect::from_center_size(pos2(c.x + dx, c.y + dy), vec2(5.5, 5.5)), CornerRadius::same(1), col);
            }
        }
        Icon::Invert => {
            let rr = Rect::from_center_size(c, vec2(26.0, 14.0));
            painter.rect_filled(rr, CornerRadius::same(4), bg);
            painter.rect_filled(
                Rect::from_min_max(rr.min, pos2(rr.center().x, rr.max.y)),
                CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 },
                col,
            );
            painter.rect_stroke(rr, CornerRadius::same(4), Stroke::new(1.2_f32, col), StrokeKind::Inside);
        }
        Icon::Align => {
            for (dy, w) in [(-5.0, 14.0), (-1.7, 9.0), (1.7, 14.0), (5.0, 9.0)] {
                painter.hline(c.x - 7.0..=c.x - 7.0 + w, c.y + dy, s);
            }
        }
        Icon::Reset => {
            let rad = 6.0;
            let pts: Vec<egui::Pos2> = (0..=20)
                .map(|i| {
                    let a = -2.4 + i as f32 / 20.0 * 4.9;
                    pos2(c.x + rad * a.cos(), c.y + rad * a.sin())
                })
                .collect();
            let end = pts[0];
            painter.add(egui::Shape::line(pts, s));
            painter.add(egui::Shape::convex_polygon(
                vec![end + vec2(-3.5, -1.0), end + vec2(2.5, -2.5), end + vec2(0.5, 3.0)],
                col,
                Stroke::NONE,
            ));
        }
        Icon::Folder => {
            let rr = Rect::from_center_size(c + vec2(0.0, 1.0), vec2(16.0, 11.0));
            painter.rect_stroke(rr, CornerRadius::same(2), s, StrokeKind::Middle);
            painter.hline(rr.left()..=rr.left() + 6.0, rr.top() - 2.0, s);
        }
        Icon::Theme => {
            painter.circle_stroke(c, 7.0, s);
            let pts: Vec<egui::Pos2> = (0..=16)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 / 16.0 * std::f32::consts::PI;
                    pos2(c.x + 7.0 * a.cos(), c.y + 7.0 * a.sin())
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
        }
        Icon::Gear => {
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                painter.line_segment(
                    [pos2(c.x + 5.0 * a.cos(), c.y + 5.0 * a.sin()), pos2(c.x + 8.0 * a.cos(), c.y + 8.0 * a.sin())],
                    Stroke::new(2.2_f32, col),
                );
            }
            painter.circle_stroke(c, 5.0, s);
            painter.circle_filled(c, 2.0, col);
        }
        Icon::Help => {
            painter.circle_stroke(c, 8.0, s);
            painter.text(c, Align2::CENTER_CENTER, "?", FontId::proportional(11.0), col);
        }
        Icon::Filter => {
            for (dy, w) in [(-4.0, 14.0), (0.0, 9.0), (4.0, 4.0)] {
                painter.hline(c.x - w / 2.0..=c.x + w / 2.0, c.y + dy, s);
            }
        }
        Icon::Finder => {
            // a picture frame with a magnifying glass
            let fr = Rect::from_center_size(c + vec2(-2.0, -1.5), vec2(15.0, 12.0));
            painter.rect_stroke(fr, CornerRadius::same(2), s, StrokeKind::Middle);
            painter.text(fr.center(), Align2::CENTER_CENTER, "A", FontId::proportional(8.5), col);
            let g = c + vec2(5.0, 5.0);
            painter.circle_filled(g, 4.2, bg);
            painter.circle_stroke(g, 3.2, Stroke::new(1.5_f32, col));
            painter.line_segment([g + vec2(2.3, 2.3), g + vec2(5.0, 5.0)], Stroke::new(1.8_f32, col));
        }
        Icon::Plus => {
            painter.circle_filled(c, 7.0, col);
            painter.hline(c.x - 3.5..=c.x + 3.5, c.y, Stroke::new(1.6_f32, bg));
            painter.vline(c.x, c.y - 3.5..=c.y + 3.5, Stroke::new(1.6_f32, bg));
        }
    }
}

/// A slider with two handles choosing a range inside 0..1.
pub(super) fn range_slider(ui: &mut Ui, id: Id, range: &mut (f32, f32), p: &Pal) {
    let (rect, resp) = ui.allocate_exact_size(vec2(120.0, 18.0), Sense::click_and_drag());
    let x = |v: f32| rect.left() + 6.0 + v * (rect.width() - 12.0);
    let to_v = |px: f32| ((px - rect.left() - 6.0) / (rect.width() - 12.0)).clamp(0.0, 1.0);
    let painter = ui.painter();
    let cy = rect.center().y;
    painter.hline(x(0.0)..=x(1.0), cy, Stroke::new(3.0_f32, p.toggle_off));
    painter.hline(x(range.0)..=x(range.1), cy, Stroke::new(3.0_f32, p.accent));
    if let Some(pos) = resp.interact_pointer_pos() {
        let v = to_v(pos.x);
        let which = ui.memory_mut(|m| {
            let w: Option<u8> = m.data.get_temp(id);
            match w {
                Some(w) if resp.dragged() && !resp.drag_started() => w,
                _ => {
                    let w = if (v - range.0).abs() <= (v - range.1).abs() { 0 } else { 1 };
                    m.data.insert_temp(id, w);
                    w
                }
            }
        });
        if which == 0 {
            range.0 = v.min(range.1);
        } else {
            range.1 = v.max(range.0);
        }
    }
    for v in [range.0, range.1] {
        painter.circle(pos2(x(v), cy), 6.0, Color32::WHITE, Stroke::new(1.5_f32, p.accent));
    }
}

pub(super) fn icon_button(ui: &mut Ui, p: &Pal, size: egui::Vec2, icon: Icon, active: bool, tip: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() || active {
        ui.painter().rect_filled(r, CornerRadius::same(6), if active { p.sel } else { p.hover });
    }
    let col = if active || resp.hovered() { p.text } else { p.weak };
    paint_icon(ui.painter(), r, icon, col, p.content);
    resp.on_hover_text(tip)
}

impl FontCraftApp {
    pub(super) fn app_menu(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        if ui.button("Add folder…").clicked() {
            acts.push(Action::AddFolderDialog);
            ui.close_menu();
        }
        if ui.button("Add network folder…").on_hover_text("A shared folder on a server, a NAS or another computer").clicked() {
            acts.push(Action::NetworkFolderDialog);
            ui.close_menu();
        }
        if ui.button("Import fonts…").clicked() {
            acts.push(Action::ImportDialog);
            ui.close_menu();
        }
        if ui
            .button("Activate fonts used in a document…")
            .on_hover_text("Pick a PDF, PSD, AI, SVG, IDML… and FontCraft switches on the fonts it uses")
            .clicked()
        {
            acts.push(Action::DocumentDialog);
            ui.close_menu();
        }
        ui.separator();
        if ui.button("New collection").clicked() {
            acts.push(Action::NewCollection(vec![]));
            ui.close_menu();
        }
        if ui.button("New tag").clicked() {
            acts.push(Action::NewTag(vec![]));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Rescan folders").clicked() {
            acts.push(Action::Rescan);
            ui.close_menu();
        }
        if ui.button("Settings").clicked() {
            self.show_settings = true;
            ui.close_menu();
        }
    }

    pub(super) fn top_bar(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let p = self.p();
        egui::TopBottomPanel::top("top")
            .exact_height(34.0)
            .frame(egui::Frame::new().fill(p.top).inner_margin(egui::Margin::symmetric(8, 0)))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                ui.horizontal_centered(|ui| {
                    ui.menu_button(RichText::new("FontCraft  ▼").size(13.0).color(p.text), |ui| self.app_menu(ui, acts));
                });
                let sw = 300.0f32.min(full.width() * 0.3);
                let sr = Rect::from_center_size(full.center(), vec2(sw, 24.0));
                let te = egui::TextEdit::singleline(&mut self.search)
                    .hint_text(RichText::new("Search fonts...").color(p.weak))
                    .horizontal_align(Align::Center)
                    .margin(egui::Margin { left: 26, right: 26, top: 4, bottom: 4 })
                    .frame(true);
                let resp = ui.put(sr, te);
                if self.search_focus {
                    resp.request_focus();
                    self.search_focus = false;
                }
                // a little X that empties the search in one click (Esc in the box does the same)
                if !self.search.is_empty() {
                    let xr = Rect::from_center_size(pos2(sr.right() - 13.0, sr.center().y), vec2(20.0, 20.0));
                    let xresp = ui.interact(xr, Id::new("searchclear"), Sense::click());
                    let col = if xresp.hovered() { p.text } else { p.weak };
                    if xresp.hovered() {
                        ui.painter().circle_filled(xr.center(), 9.0, p.sel);
                    }
                    let c = xr.center();
                    let s = 3.6;
                    let st = Stroke::new(1.6_f32, col);
                    ui.painter().line_segment([c + vec2(-s, -s), c + vec2(s, s)], st);
                    ui.painter().line_segment([c + vec2(-s, s), c + vec2(s, -s)], st);
                    let esc = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
                    if xresp.on_hover_text("Clear the search").clicked() || esc {
                        self.search.clear();
                        resp.request_focus();
                    }
                }
                let fr = Rect::from_center_size(pos2(sr.right() + 18.0, sr.center().y), vec2(24.0, 24.0));
                let fresp = ui.interact(fr, Id::new("filterbtn"), Sense::click());
                let fcol = if self.filter != Filter::All || self.shape_active() {
                    p.accent
                } else if fresp.hovered() {
                    p.text
                } else {
                    p.weak
                };
                paint_icon(ui.painter(), fr, Icon::Filter, fcol, p.top);
                let popup = Id::new("filterpopup");
                if fresp.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup));
                }
                let fresp = fresp.on_hover_text("Filter, sort and Super search");
                egui::popup_below_widget(ui, popup, &fresp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                    ui.set_min_width(250.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("SUPER SEARCH").size(11.0).color(p.weak));
                        if self.shape_active() && ui.small_button("Reset").clicked() {
                            self.shape_range = [(0.0, 1.0); 4];
                        }
                    });
                    ui.label(RichText::new("Find fonts by how they look.").size(11.5).color(p.weak));
                    for k in 0..4 {
                        ui.horizontal(|ui| {
                            ui.add_sized([70.0, 18.0], egui::Label::new(RichText::new(crate::library::SHAPE_NAMES[k]).size(12.0)));
                            range_slider(ui, Id::new(("shape", k)), &mut self.shape_range[k], &p);
                            let (a, b) = self.shape_range[k];
                            ui.label(RichText::new(format!("{a:.2}–{b:.2}")).monospace().size(10.5).color(p.weak));
                        });
                    }
                    ui.separator();
                    ui.label(RichText::new("SHOW").size(11.0).color(p.weak));
                    for (f, l) in [
                        (Filter::All, "All fonts"),
                        (Filter::Variable, "Variable fonts"),
                        (Filter::Mono, "Monospace fonts"),
                        (Filter::Italic, "Families with italics"),
                    ] {
                        ui.radio_value(&mut self.filter, f, l);
                    }
                    ui.separator();
                    ui.label(RichText::new("SORT").size(11.0).color(p.weak));
                    for (s, l) in [
                        (SortBy::NameAz, "Name A–Z"),
                        (SortBy::NameZa, "Name Z–A"),
                        (SortBy::Recent, "Recently added"),
                        (SortBy::Styles, "Most styles"),
                        (SortBy::Popular, "Most popular (online sources)"),
                    ] {
                        if ui.radio_value(&mut self.cfg.sort, s, l).changed() {
                            self.dirty = true;
                        }
                    }
                });
                let hr = Rect::from_center_size(pos2(full.right() - 16.0, full.center().y), vec2(26.0, 26.0));
                let hresp = ui.interact(hr, Id::new("helpbtn"), Sense::click());
                paint_icon(ui.painter(), hr, Icon::Help, if hresp.hovered() { p.text } else { p.weak }, p.top);
                if hresp.on_hover_text("Help & settings").clicked() {
                    self.show_settings = true;
                }
            });
    }

    pub(super) fn bottom_bar(&mut self, ctx: &egui::Context) {
        let p = self.p();
        egui::TopBottomPanel::bottom("previewbar")
            .exact_height(38.0)
            .frame(
                egui::Frame::new()
                    .fill(p.content)
                    .inner_margin(egui::Margin { left: 14, right: 10, top: 0, bottom: 0 })
                    .stroke(Stroke::new(1.0_f32, p.line)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.menu_button(RichText::new("Samples ▲").size(12.0).color(p.weak), |ui| {
                            ui.label(RichText::new("SAMPLE TEXTS").size(11.0).color(p.weak));
                            for t in PRESETS {
                                if ui.button(*t).clicked() {
                                    self.cfg.preview_text = t.to_string();
                                    self.dirty = true;
                                    ui.close_menu();
                                }
                            }
                        })
                        .response
                        .on_hover_text("Sample texts");
                        if let Some(u) = self.update.clone() {
                            let b = egui::Button::new(RichText::new(format!("FontCraft {} is available – download", u.version)).size(11.5).color(Color32::BLACK))
                                .fill(p.accent)
                                .corner_radius(CornerRadius::same(10));
                            if ui.add(b).on_hover_text("Opens the download page of the newest version").clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(u.url));
                            }
                        }
                        if self.scan_rx.is_some() {
                            let (d, t) = self.scan_progress;
                            ui.label(RichText::new(format!("Scanning fonts… {d}/{t}")).size(11.5).color(p.weak));
                            ui.spinner();
                        } else if let Some((s, t)) = &self.status {
                            if t.elapsed().as_secs() < 7 {
                                ui.label(RichText::new(s).size(11.5).color(p.accent));
                            }
                        }
                        let w = ui.available_width();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.cfg.preview_text)
                                    .hint_text("Type your preview text…")
                                    .frame(false)
                                    .desired_width(w)
                                    .text_color(p.text),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                    });
                });
            });
    }

    pub(super) fn rail(&mut self, ctx: &egui::Context) {
        let p = self.p();
        egui::SidePanel::left("rail")
            .exact_width(RAIL_W)
            .resizable(false)
            .frame(egui::Frame::new().fill(p.top).inner_margin(egui::Margin::symmetric(6, 10)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                let sz = vec2(32.0, 32.0);
                if icon_button(ui, &p, sz, Icon::Lines, false, "Show / hide sidebar").clicked() {
                    self.sidebar_open = !self.sidebar_open;
                }
                if icon_button(ui, &p, sz, Icon::Folder, false, "Add a font folder").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().set_title("Add a font folder").pick_folder() {
                        self.add_folder(dir);
                    }
                }
                if icon_button(ui, &p, sz, Icon::Finder, self.view == View::Finder, "Font finder: find the font in a picture").clicked() {
                    self.view = View::Finder;
                    self.selected.clear();
                    self.primary = None;
                    self.gsel = None;
                }
                if icon_button(ui, &p, sz, Icon::Theme, false, "Colour theme (click to switch)").clicked() {
                    self.cfg.theme = match self.cfg.theme {
                        Theme::Dark => Theme::Sand,
                        Theme::Sand => Theme::Light,
                        Theme::Light => Theme::Midnight,
                        Theme::Midnight => Theme::Sepia,
                        Theme::Sepia => Theme::Classic,
                        Theme::Classic => Theme::Dark,
                    };
                    self.dirty = true;
                }
                if icon_button(ui, &p, sz, Icon::Gear, self.show_settings, "Settings").clicked() {
                    self.show_settings = !self.show_settings;
                }
            });
    }
}
