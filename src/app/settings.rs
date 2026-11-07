//! The settings window.

use super::*;
use egui::{pos2, vec2, Align2, FontId, Rect, RichText, Sense, StrokeKind, Ui};

fn heading(ui: &mut Ui, p: &Pal, s: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(s).size(11.0).strong().color(p.weak));
}

fn note(ui: &mut Ui, p: &Pal, s: &str) {
    ui.add(egui::Label::new(RichText::new(s).size(12.0).color(p.weak)).wrap());
}

impl FontCraftApp {
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        let p = self.p();
        let mut open = self.show_settings;
        let mut acts: Vec<Action> = vec![];
        let max_h = ctx.screen_rect().height() - 140.0;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .default_width(500.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                    ui.set_width(480.0);
                    self.settings_appearance(ui, &p);
                    self.settings_library(ui, &p);
                    self.settings_activation(ui, &p);
                    self.settings_auto(ui, &p, &mut acts);
                    self.settings_sync(ui, &p);
                    self.settings_about(ui, &p);
                });
            });
        self.show_settings = open;
        self.apply(acts);
    }

    fn settings_appearance(&mut self, ui: &mut Ui, p: &Pal) {
        ui.label(RichText::new("COLOUR THEME").size(11.0).strong().color(p.weak));
        ui.horizontal(|ui| {
            for (t, l) in [
                (Theme::Classic, "Classic"),
                (Theme::Dark, "Dark"),
                (Theme::Light, "Light"),
                (Theme::Midnight, "Midnight"),
                (Theme::Sepia, "Sepia"),
            ] {
                let pp = pal(t);
                let (r, resp) = ui.allocate_exact_size(vec2(84.0, 46.0), Sense::click());
                let chosen = self.cfg.theme == t;
                ui.painter().rect(
                    r,
                    CornerRadius::same(6),
                    pp.content,
                    Stroke::new(if chosen { 2.0 } else { 1.0 }, if chosen { pp.accent } else { p.line }),
                    StrokeKind::Inside,
                );
                ui.painter().rect_filled(Rect::from_min_size(r.min + vec2(1.0, 1.0), vec2(18.0, r.height() - 2.0)), CornerRadius::same(5), pp.side);
                ui.painter().circle_filled(pos2(r.left() + 30.0, r.top() + 13.0), 4.0, pp.accent);
                ui.painter().text(pos2(r.left() + 26.0, r.bottom() - 12.0), Align2::LEFT_CENTER, l, FontId::proportional(12.0), pp.text);
                if resp.clicked() {
                    self.cfg.theme = t;
                    self.dirty = true;
                }
            }
        });
        heading(ui, p, "FONT DISPLAY");
        ui.horizontal(|ui| {
            ui.label("Font list scroll speed");
            for (v, l) in [(0u8, "Slow"), (1, "Normal"), (2, "Fast")] {
                if ui.selectable_label(self.cfg.scroll_speed == v, l).clicked() {
                    self.cfg.scroll_speed = v;
                    ui.ctx().options_mut(|o| o.line_scroll_speed = scroll_speed(v));
                    self.dirty = true;
                }
            }
        });
        if ui.checkbox(&mut self.cfg.group_families, "Group styles into families").changed() {
            self.dirty = true;
            self.rescan();
        }
        note(ui, p, "Right-click a font and choose “Change family name…” to group styles yourself.");
        if !self.cfg.family_overrides.is_empty() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} family name change(s)", self.cfg.family_overrides.len())).size(12.0));
                if ui.small_button("Reset all").clicked() {
                    self.cfg.family_overrides.clear();
                    self.dirty = true;
                    self.rescan();
                }
            });
        }
        if ui.checkbox(&mut self.cfg.show_system, "Show the fonts installed on this computer").changed() {
            self.dirty = true;
        }
    }

    fn settings_library(&mut self, ui: &mut Ui, p: &Pal) {
        heading(ui, p, "IMPORT FOLDER");
        note(ui, p, "Fonts you import (or drop onto the window) are copied here. Folders you add stay where they are.");
        ui.horizontal(|ui| {
            let dir = self.cfg.import_dir();
            ui.add(egui::Label::new(RichText::new(dir.display().to_string()).monospace().size(11.0)).truncate());
        });
        ui.horizontal(|ui| {
            if ui.button("Change…").clicked() {
                if let Some(d) = rfd::FileDialog::new().set_title("Folder for imported fonts").pick_folder() {
                    let old = self.cfg.import_dir();
                    self.cfg.import_dir = Some(d.clone());
                    // keep fonts imported earlier in the library
                    if old.exists() && !self.cfg.folders.contains(&d) {
                        self.cfg.folders.push(d);
                    }
                    self.dirty = true;
                    self.rescan();
                }
            }
            if self.cfg.import_dir.is_some() && ui.button("Use default").clicked() {
                self.cfg.import_dir = None;
                self.dirty = true;
            }
            if ui.button("Rescan all folders").clicked() {
                self.rescan();
            }
        });
        note(ui, p, "Files already imported are not moved when you change the folder.");
    }

    fn settings_activation(&mut self, ui: &mut Ui, p: &Pal) {
        heading(ui, p, "ACTIVATION & APP");
        note(
            ui,
            p,
            "Click the dot next to a font, folder, collection or tag to switch it on. Active fonts work in Word, Photoshop, \
Affinity and every other app without being installed. Some apps only see new fonts after a restart.",
        );
        if ui.checkbox(&mut self.cfg.deactivate_on_exit, "Switch all fonts off when FontCraft closes").changed() {
            self.dirty = true;
        }
        let tray_ok = cfg!(windows);
        if ui
            .add_enabled(tray_ok, egui::Checkbox::new(&mut self.cfg.hide_to_tray, "Hide to the system tray when closing the window"))
            .changed()
        {
            self.dirty = true;
        }
        note(
            ui,
            p,
            if tray_ok {
                "Windows only. FontCraft keeps running next to the clock, so active fonts and folder watching continue. \
Click the tray icon to open it again; its menu can quit FontCraft."
            } else {
                "Windows only – not available on this system."
            },
        );
        let mut l = self.cfg.launch_at_login;
        let mut hidden = self.cfg.start_hidden;
        let a = ui.checkbox(&mut l, "Start FontCraft automatically when I sign in (autostart)").changed();
        note(ui, p, "Works on Windows, macOS and Linux. Keeps your active fonts available after a restart.");
        let hidden_label = if tray_ok && self.cfg.hide_to_tray { "Start hidden in the system tray" } else { "Start hidden (minimised)" };
        let b = ui.add_enabled(l, egui::Checkbox::new(&mut hidden, hidden_label)).changed();
        if (a || b) && self.shot.is_none() {
            match activation::set_launch_at_login(l, hidden) {
                Ok(()) => {
                    self.cfg.launch_at_login = l;
                    self.cfg.start_hidden = hidden;
                    self.dirty = true;
                }
                Err(e) => self.set_status(format!("Could not change the start-up setting: {e}")),
            }
        }
        if ui.button("Switch every font off now").clicked() {
            let all: Vec<String> = self.cfg.active.iter().cloned().collect();
            self.set_paths(all, false);
        }
    }

    fn settings_auto(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        heading(ui, p, "AUTO-ACTIVATION");
        note(
            ui,
            p,
            "FontCraft reads which fonts a document uses and switches them on: Photoshop, Illustrator, InDesign (also IDML), \
PDF, EPS, SVG, Word, PowerPoint and Excel files. Drop a document onto the window, or add your project folders below: \
whenever a document there is saved or added, its fonts are activated automatically.",
        );
        let mut remove = None;
        for (i, f) in self.cfg.auto_folders.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("✖").on_hover_text("Stop watching this folder").clicked() {
                    remove = Some(i);
                }
                ui.add(egui::Label::new(RichText::new(f.display().to_string()).size(12.0)).truncate());
            });
        }
        if let Some(i) = remove {
            self.cfg.auto_folders.remove(i);
            self.autoact.set_folders(self.cfg.auto_folders.clone());
            self.dirty = true;
        }
        ui.horizontal(|ui| {
            if ui.button("Add project folder…").clicked() {
                if let Some(d) = rfd::FileDialog::new().set_title("Watch a project folder").pick_folder() {
                    if !self.cfg.auto_folders.contains(&d) {
                        self.cfg.auto_folders.push(d);
                        self.autoact.set_folders(self.cfg.auto_folders.clone());
                        self.dirty = true;
                    }
                }
            }
            if ui.button("Activate fonts of a document…").clicked() {
                acts.push(Action::DocumentDialog);
            }
        });
        if ui
            .checkbox(&mut self.cfg.auto_google, "Download missing fonts from Google Fonts when a document needs them")
            .changed()
        {
            self.dirty = true;
        }
    }

    fn settings_sync(&mut self, ui: &mut Ui, p: &Pal) {
        heading(ui, p, "SYNC BETWEEN COMPUTERS");
        note(
            ui,
            p,
            "Choose a folder inside Dropbox, Google Drive, OneDrive or iCloud Drive. FontCraft keeps your collections, tags and \
favourites there – plus copies of the fonts in your collections – and every computer that uses the same folder stays in step.",
        );
        match self.cfg.sync_dir.clone() {
            None => {
                if ui.button("Choose a cloud folder…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().set_title("Folder in your cloud drive").pick_folder() {
                        self.enable_sync(d);
                    }
                }
            }
            Some(d) => {
                ui.add(egui::Label::new(RichText::new(format!("Syncing with {}", d.display())).size(12.0).color(p.accent)).truncate());
                ui.horizontal(|ui| {
                    if ui.button("Sync now").clicked() {
                        self.sync_pull(true);
                        self.sync_push();
                        self.rescan();
                        self.set_status("Synced");
                    }
                    if ui.button("Stop syncing").clicked() {
                        self.disable_sync();
                    }
                });
            }
        }
    }

    fn settings_about(&mut self, ui: &mut Ui, p: &Pal) {
        heading(ui, p, "TIPS");
        for t in [
            "Your font folders are watched: fonts you copy into them appear within seconds, deleted ones disappear.",
            "The filter button next to the search has Super search: find fonts by contrast, proportion, x-height and weight.",
            "Providers › Google: click the dot next to any Google font to download and activate it.",
            "Drag fonts onto a collection, tag or Favorites in the sidebar.",
            "Ctrl/Cmd-click and Shift-click select several fonts; Space switches them on or off.",
            "Ctrl/Cmd+F jumps to the search. Several words narrow it down (e.g. serif heavy variable).",
            "Playground (in the font details) tries a font on a sample page and exports the CSS.",
        ] {
            ui.label(RichText::new(format!("•  {t}")).size(12.0));
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("FontCraft {} — every feature included, free, no account and no ads.", crate::updates::VERSION))
                .color(p.accent),
        );
        if let Some(url) = crate::updates::releases_url() {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.cfg.check_updates, "Tell me when a new version is out").changed() {
                    self.dirty = true;
                }
                ui.hyperlink_to("Download page", url);
            });
        }
        ui.label(RichText::new(format!("Settings are kept in {}", config::data_dir().display())).size(11.0).color(p.weak));
    }
}
