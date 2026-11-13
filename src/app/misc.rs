//! Keyboard shortcuts, dropped files and drag overlays.

use super::*;
use egui::{pos2, vec2, Align2, FontFamily, FontId, Id, Rect};

impl FontCraftApp {
    /// Keyboard shortcuts (the list is in `shortcuts.rs`, and in Settings › Keyboard shortcuts).
    pub(super) fn keyboard(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        use shortcuts::Cmd;
        // a question window (delete, network folder, family name) has the keyboard to itself
        if self.delete.is_some() || self.netdlg.is_some() || self.family_rename.is_some() {
            return;
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        let sel: Vec<usize> = self.selected.iter().copied().collect();
        let library = !matches!(self.view, View::Online(_) | View::Finder);
        for cmd in shortcuts::pressed(ctx, typing) {
            match cmd {
                Cmd::Activate | Cmd::Deactivate if !sel.is_empty() => acts.push(Action::SetFamilies(sel.clone(), cmd == Cmd::Activate)),
                Cmd::Toggle if !sel.is_empty() => {
                    let any_inactive = sel.iter().any(|f| {
                        let (a, n) = self.fam_state(*f);
                        a < n
                    });
                    acts.push(Action::SetFamilies(sel.clone(), any_inactive));
                }
                Cmd::Delete if !sel.is_empty() && library => acts.push(Action::Delete(sel.clone())),
                Cmd::Favorite if !sel.is_empty() => acts.push(Action::ToggleFavorite(sel.clone())),
                Cmd::Rename if !sel.is_empty() => acts.push(Action::RenameFamily(sel.clone())),
                Cmd::CopyName if !sel.is_empty() => {
                    let names: Vec<String> = sel.iter().map(|f| self.families[*f].name.clone()).collect();
                    acts.push(Action::Copy(names.join(", ")));
                }
                Cmd::Reveal => {
                    if let Some(fi) = self.primary.filter(|f| *f < self.families.len()) {
                        let face = self.families[fi].primary;
                        acts.push(Action::ShowInFolder(self.faces[face].meta.path.clone()));
                    }
                }
                Cmd::Print if !sel.is_empty() => {
                    let title = if sel.len() == 1 { self.families[sel[0]].name.clone() } else { format!("{} families", sel.len()) };
                    acts.push(Action::Specimen(sel.clone(), title));
                }
                Cmd::Up | Cmd::Down | Cmd::Left | Cmd::Right | Cmd::First | Cmd::Last if library && !self.visible.is_empty() => {
                    let n = self.visible.len();
                    let tiles = self.cfg.view_mode == config::ViewMode::Grid;
                    // in the tile view up/down go a whole row
                    let step = if tiles { self.tile_cols.max(1) } else { 1 };
                    let pos = self.primary.and_then(|p| self.visible.iter().position(|x| *x == p));
                    let next = match (cmd, pos) {
                        (Cmd::First, _) => 0,
                        (Cmd::Last, _) => n - 1,
                        (Cmd::Left | Cmd::Right, _) if !tiles => continue,
                        (_, None) => 0,
                        (Cmd::Up, Some(k)) => k.saturating_sub(step),
                        (Cmd::Down, Some(k)) => (k + step).min(n - 1),
                        (Cmd::Left, Some(k)) => k.saturating_sub(1),
                        (_, Some(k)) => (k + 1).min(n - 1),
                    };
                    acts.push(Action::Click(self.visible[next], false, false));
                    self.scroll_to = Some(next);
                }
                Cmd::SelectAll if library => self.selected = self.visible.iter().copied().collect(),
                Cmd::Escape => {
                    if self.selected.len() > 1 {
                        self.selected.clear();
                        if let Some(p) = self.primary {
                            self.selected.insert(p);
                        }
                    } else {
                        self.selected.clear();
                        self.primary = None;
                        self.gsel = None;
                    }
                }
                Cmd::ListView | Cmd::TileView => {
                    self.cfg.view_mode = if cmd == Cmd::ListView { config::ViewMode::List } else { config::ViewMode::Grid };
                    self.dirty = true;
                }
                Cmd::Bigger | Cmd::Smaller => {
                    let step = if cmd == Cmd::Bigger { 4.0 } else { -4.0 };
                    self.cfg.preview_size = (self.cfg.preview_size + step).clamp(8.0, 200.0);
                    self.dirty = true;
                }
                Cmd::ResetSize => {
                    let d = Config::default();
                    self.cfg.preview_size = d.preview_size;
                    self.cfg.preview_text = d.preview_text;
                    self.dirty = true;
                }
                Cmd::Sidebar => self.sidebar_open = !self.sidebar_open,
                Cmd::Invert => {
                    self.cfg.invert_preview = !self.cfg.invert_preview;
                    self.dirty = true;
                }
                Cmd::Theme => {
                    self.cfg.theme = next_theme(self.cfg.theme);
                    self.dirty = true;
                }
                Cmd::Search => self.search_focus = true,
                Cmd::Finder => acts.push(Action::SetView(View::Finder)),
                Cmd::AddFolder => acts.push(Action::AddFolderDialog),
                Cmd::Import => acts.push(Action::ImportDialog),
                Cmd::NewCollection => acts.push(Action::NewCollection(sel.clone())),
                Cmd::NewTag => acts.push(Action::NewTag(sel.clone())),
                Cmd::Rescan => acts.push(Action::Rescan),
                Cmd::Settings => self.show_settings = !self.show_settings,
                Cmd::Help => {
                    self.show_settings = true;
                    self.settings_page = settings::Page::Shortcuts;
                }
                _ => {}
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
        // fonts and folders go in one import (folders are added or copied as set in Settings › Import)
        let mut items: Vec<(PathBuf, PathBuf)> = vec![];
        let mut dirs = vec![];
        for p in dropped {
            if p.is_dir() {
                dirs.push(p);
            } else if library::is_font_file(&p) {
                if let Some(n) = p.file_name() {
                    items.push((p.clone(), PathBuf::from(n)));
                }
            } else if finder::is_image(&p) {
                self.finder_load_path(p);
            } else if crate::autoact::is_document(&p) {
                self.autoact.check(p, false);
            }
        }
        items.extend(self.folder_items(dirs));
        if !items.is_empty() {
            self.import_start(items);
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
                let g = painter.layout_no_wrap(txt, FontId::new(13.0, FontFamily::Proportional), p.on_accent);
                let r = Rect::from_min_size(pos + vec2(14.0, 8.0), g.size() + vec2(20.0, 10.0));
                painter.rect_filled(r, CornerRadius::same(12), p.accent);
                painter.galley(r.min + vec2(10.0, 5.0), g, p.on_accent);
                let _ = pos2(0.0, 0.0);
            }
        }
    }
}
