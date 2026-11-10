//! Keyboard shortcuts, dropped files and drag overlays.

use super::*;
use egui::{pos2, vec2, Align2, FontFamily, FontId, Id, Rect};

impl FontCraftApp {
    pub(super) fn keyboard(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            self.search_focus = true;
        }
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let (up, down, space, all, esc) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Space),
                i.modifiers.command && i.key_pressed(egui::Key::A),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if (up || down) && !self.visible.is_empty() {
            let pos = self.primary.and_then(|p| self.visible.iter().position(|x| *x == p));
            let next = match pos {
                None => 0,
                Some(k) if up => k.saturating_sub(1),
                Some(k) => (k + 1).min(self.visible.len() - 1),
            };
            acts.push(Action::Click(self.visible[next], false, false));
        }
        if space && !self.selected.is_empty() {
            let sel: Vec<usize> = self.selected.iter().copied().collect();
            let any_inactive = sel.iter().any(|f| {
                let (a, n) = self.fam_state(*f);
                a < n
            });
            acts.push(Action::SetFamilies(sel, any_inactive));
        }
        if all {
            self.selected = self.visible.iter().copied().collect();
        }
        if esc {
            if self.selected.len() > 1 {
                self.selected.clear();
                if let Some(p) = self.primary {
                    self.selected.insert(p);
                }
            } else {
                self.selected.clear();
                self.primary = None;
            }
        }
    }

    /// "Change family name…": group the chosen styles under a name of your choice.
    pub(super) fn family_rename_window(&mut self, ctx: &egui::Context) {
        let Some((keys, mut name)) = self.family_rename.take() else { return };
        let mut done = None;
        let mut open = true;
        egui::Window::new("Change family name")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!("{} style{} will be shown as one family named:", keys.len(), if keys.len() == 1 { "" } else { "s" }));
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(280.0));
                r.request_focus();
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() || enter {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                    if ui.button("Use the font's own name").clicked() {
                        name.clear();
                        done = Some(true);
                    }
                });
            });
        match done {
            Some(true) => {
                let n = name.trim().to_string();
                for k in keys {
                    if n.is_empty() {
                        self.cfg.family_overrides.remove(&k);
                    } else {
                        self.cfg.family_overrides.insert(k, n.clone());
                    }
                }
                self.dirty = true;
                self.rescan();
            }
            Some(false) => {}
            None if open => self.family_rename = Some((keys, name)),
            None => {}
        }
    }

    pub(super) fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if dropped.is_empty() {
            return;
        }
        let mut files = vec![];
        for p in dropped {
            if p.is_dir() {
                self.add_folder(p);
            } else if library::is_font_file(&p) {
                files.push(p);
            } else if finder::is_image(&p) {
                self.finder_load_path(p);
            } else if crate::autoact::is_document(&p) {
                self.autoact.check(p, false);
            }
        }
        if !files.is_empty() {
            self.import_files(files);
        }
    }

    pub(super) fn overlays(&self, ctx: &egui::Context) {
        let p = self.p();
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let r = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("drop")));
            painter.rect_filled(r, CornerRadius::ZERO, Color32::from_black_alpha(150));
            let msg = if self.view == View::Finder { "Drop a picture to find its font" } else { "Drop fonts or folders to add them – or a picture to find its font" };
            painter.text(r.center(), Align2::CENTER_CENTER, msg, FontId::proportional(22.0), Color32::WHITE);
        }
        if let Some(pl) = egui::DragAndDrop::payload::<DragFams>(ctx) {
            if let Some(pos) = ctx.pointer_interact_pos() {
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("dragghost")));
                let txt = if pl.0.len() == 1 {
                    self.families.get(pl.0[0]).map(|f| f.name.clone()).unwrap_or_default()
                } else {
                    format!("{} families", pl.0.len())
                };
                let g = painter.layout_no_wrap(txt, FontId::new(13.0, FontFamily::Proportional), Color32::WHITE);
                let r = Rect::from_min_size(pos + vec2(14.0, 8.0), g.size() + vec2(20.0, 10.0));
                painter.rect_filled(r, CornerRadius::same(12), p.accent);
                painter.galley(r.min + vec2(10.0, 5.0), g, Color32::WHITE);
                let _ = pos2(0.0, 0.0);
            }
        }
    }
}
