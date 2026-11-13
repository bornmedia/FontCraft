//! The settings window: a list of sections on the left, the chosen section on the right.

use super::ui::{paint_icon, Icon};
use super::*;
use egui::{pos2, vec2, Align2, FontId, Rect, RichText, Sense, StrokeKind, Ui};

fn heading(ui: &mut Ui, p: &Pal, s: &str) {
    ui.add_space(14.0);
    ui.label(RichText::new(s).size(11.0).strong().color(p.weak));
    ui.add_space(2.0);
}

fn note(ui: &mut Ui, p: &Pal, s: &str) {
    ui.add(egui::Label::new(RichText::new(s).size(12.0).color(p.weak)).wrap());
}

/// The sections of the settings window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Appearance,
    FontList,
    Library,
    Import,
    Activation,
    Startup,
    Auto,
    Sync,
    Shortcuts,
    About,
}

impl Page {
    const ALL: [Page; 10] = [
        Page::Appearance,
        Page::FontList,
        Page::Library,
        Page::Import,
        Page::Activation,
        Page::Startup,
        Page::Auto,
        Page::Sync,
        Page::Shortcuts,
        Page::About,
    ];
    fn title(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::FontList => "Font list",
            Page::Library => "Library & folders",
            Page::Import => "Import",
            Page::Activation => "Activation",
            Page::Startup => "Start-up & tray",
            Page::Auto => "Auto-activation",
            Page::Sync => "Sync",
            Page::Shortcuts => "Keyboard shortcuts",
            Page::About => "About FontCraft",
        }
    }
    fn icon(self) -> Icon {
        match self {
            Page::Appearance => Icon::Theme,
            Page::FontList => Icon::List,
            Page::Library => Icon::Folder,
            Page::Import => Icon::Import,
            Page::Activation => Icon::Power,
            Page::Startup => Icon::Rocket,
            Page::Auto => Icon::Bolt,
            Page::Sync => Icon::Cloud,
            Page::Shortcuts => Icon::Keyboard,
            Page::About => Icon::Help,
        }
    }
}

impl FontCraftApp {
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        let p = self.p();
        let mut open = self.show_settings;
        let mut acts: Vec<Action> = vec![];
        let screen = ctx.screen_rect();
        let w = (screen.width() - 80.0).clamp(560.0, 940.0);
        let h = (screen.height() - 120.0).clamp(360.0, 660.0);
        let nav_w = 210.0;
        egui::Window::new(RichText::new("Settings").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(w, h))
            .frame(egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(0)))
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    // the list of sections
                    let (nav, _) = ui.allocate_exact_size(vec2(nav_w, h), Sense::hover());
                    ui.painter().rect_filled(nav, CornerRadius { nw: 0, ne: 0, sw: 10, se: 0 }, p.side);
                    let mut y = nav.top() + 10.0;
                    for page in Page::ALL {
                        let r = Rect::from_min_size(pos2(nav.left() + 8.0, y), vec2(nav_w - 16.0, 34.0));
                        let resp = ui.interact(r, egui::Id::new(("settings-page", page as u8)), Sense::click());
                        let chosen = self.settings_page == page;
                        if chosen || resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(6), if chosen { p.sel } else { p.hover });
                        }
                        let col = if chosen { p.accent } else { p.weak };
                        paint_icon(ui.painter(), Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(24.0, 24.0)), page.icon(), col, p.side);
                        ui.painter().text(
                            pos2(r.left() + 38.0, r.center().y),
                            Align2::LEFT_CENTER,
                            page.title(),
                            FontId::proportional(13.5),
                            if chosen { p.text } else { p.text.gamma_multiply(0.85) },
                        );
                        if resp.clicked() {
                            self.settings_page = page;
                        }
                        y += 38.0;
                    }
                    // the chosen section
                    ui.vertical(|ui| {
                        ui.set_width(w - nav_w);
                        ui.set_height(h);
                        egui::ScrollArea::vertical().id_salt(("settings-scroll", self.settings_page as u8)).auto_shrink([false, false]).show(ui, |ui| {
                            egui::Frame::new().inner_margin(egui::Margin { left: 26, right: 26, top: 16, bottom: 20 }).show(ui, |ui| {
                                ui.set_width(w - nav_w - 52.0);
                                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                                ui.label(RichText::new(self.settings_page.title()).size(22.0).color(p.text));
                                ui.add_space(4.0);
                                match self.settings_page {
                                    Page::Appearance => self.settings_appearance(ui, &p),
                                    Page::FontList => self.settings_font_list(ui, &p),
                                    Page::Library => self.settings_library(ui, &p, &mut acts),
                                    Page::Import => self.settings_import(ui, &p),
                                    Page::Activation => self.settings_activation(ui, &p),
                                    Page::Startup => self.settings_startup(ui, &p),
                                    Page::Auto => self.settings_auto(ui, &p, &mut acts),
                                    Page::Sync => self.settings_sync(ui, &p),
                                    Page::Shortcuts => self.settings_shortcuts(ui, &p),
                                    Page::About => self.settings_about(ui, &p),
                                }
                            });
                        });
                    });
                });
            });
        self.show_settings = open;
        self.apply(acts);
    }

    fn settings_appearance(&mut self, ui: &mut Ui, p: &Pal) {
        heading(ui, p, "COLOUR THEME");
        ui.horizontal_wrapped(|ui| {
            for (t, l) in [
                (Theme::Classic, "Classic"),
                (Theme::Dark, "Dark"),
                (Theme::Sand, "Sand"),
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
                    Stroke::new(if chosen { 2.0 } else { 1.0_f32 }, if chosen { pp.accent } else { p.line }),
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
        note(ui, p, &format!("{} goes through the themes one after the other.", shortcuts::label(shortcuts::Cmd::Theme)));
        heading(ui, p, "PREVIEWS");
        if ui.checkbox(&mut self.cfg.invert_preview, "Light text on a dark background (swap the preview colours)").changed() {
            self.dirty = true;
        }
        heading(ui, p, "PANELS");
        note(ui, p, "Drag the edge of the sidebar or of the font details to make them wider or narrower. FontCraft remembers the width.");
        if ui.button("Reset both widths").clicked() {
            let d = Config::default();
            self.cfg.sidebar_w = d.sidebar_w;
            self.cfg.details_w = d.details_w;
            // the panels keep their own width while open; forget it so they take the default again
            ui.ctx().memory_mut(|m| {
                m.data.remove::<egui::containers::panel::PanelState>(egui::Id::new("side"));
                m.data.remove::<egui::containers::panel::PanelState>(egui::Id::new("details"));
            });
            self.dirty = true;
        }
    }

    fn settings_font_list(&mut self, ui: &mut Ui, p: &Pal) {
        heading(ui, p, "VIEW");
        ui.horizontal(|ui| {
            for (m, l, k) in [
                (config::ViewMode::List, "List", shortcuts::Cmd::ListView),
                (config::ViewMode::Grid, "Tiles", shortcuts::Cmd::TileView),
            ] {
                if ui.selectable_label(self.cfg.view_mode == m, l).on_hover_text(shortcuts::label(k)).clicked() {
                    self.cfg.view_mode = m;
                    self.dirty = true;
                }
            }
        });
        heading(ui, p, "TEXT ALIGNMENT");
        egui::ComboBox::from_id_salt("settings-align").selected_text(self.cfg.align.label()).width(260.0).show_ui(ui, |ui| {
            for a in config::TextAlign::ALL {
                if ui.selectable_label(self.cfg.align == a, a.label()).clicked() {
                    self.cfg.align = a;
                    self.dirty = true;
                }
            }
        });
        heading(ui, p, "PREVIEW SIZE");
        ui.horizontal(|ui| {
            if ui.add(egui::Slider::new(&mut self.cfg.preview_size, 8.0..=200.0).suffix(" px")).changed() {
                self.dirty = true;
            }
        });
        heading(ui, p, "SCROLLING");
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
        heading(ui, p, "FAMILIES");
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
        heading(ui, p, "SYSTEM FONTS");
        if ui.checkbox(&mut self.cfg.show_system, "Show the fonts installed on this computer").changed() {
            self.dirty = true;
        }
    }

    fn settings_library(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        heading(ui, p, "FOLDERS");
        note(ui, p, "FontCraft shows the fonts in the folders you add and watches them: fonts you copy in appear within seconds.");
        for f in &self.cfg.folders {
            let tag = if self.is_net_folder(f) { "  · network" } else { "" };
            ui.add(egui::Label::new(RichText::new(format!("{}{tag}", f.display())).size(12.0)).truncate());
        }
        ui.horizontal(|ui| {
            if ui.button("Add a folder…").on_hover_text(shortcuts::label(shortcuts::Cmd::AddFolder)).clicked() {
                acts.push(Action::AddFolderDialog);
            }
            if ui.button("Add a network folder…").clicked() {
                acts.push(Action::NetworkFolderDialog);
            }
            if ui.button("Rescan all folders").on_hover_text(shortcuts::label(shortcuts::Cmd::Rescan)).clicked() {
                self.rescan();
            }
        });
        note(ui, p, "To take a folder out of FontCraft, right-click it in the sidebar. Its files stay on your disk.");
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
            if ui.button("Import fonts…").on_hover_text(shortcuts::label(shortcuts::Cmd::Import)).clicked() {
                acts.push(Action::ImportDialog);
            }
        });
        note(ui, p, "Files already imported are not moved when you change the folder.");
    }

    fn settings_activation(&mut self, ui: &mut Ui, p: &Pal) {
        heading(ui, p, "HOW IT WORKS");
        note(
            ui,
            p,
            "Click the dot next to a font, folder, collection or tag to switch it on. Active fonts work in Word, Photoshop, \
Affinity and every other app without being installed. Some apps only see new fonts after a restart.",
        );
        if cfg!(windows) {
            note(
                ui,
                p,
                "While a font is active it is also listed in your personal Windows font folder, so “Package” in Illustrator \
or InDesign copies it like an installed font. It is taken out again when you switch it off.",
            );
        }
        heading(ui, p, "SHORTCUTS");
        note(
            ui,
            p,
            &format!(
                "{} activates and {} deactivates the selected fonts; Space switches them on or off.",
                shortcuts::label(shortcuts::Cmd::Activate),
                shortcuts::label(shortcuts::Cmd::Deactivate)
            ),
        );
        heading(ui, p, "WHEN FONTCRAFT CLOSES");
        if ui.checkbox(&mut self.cfg.deactivate_on_exit, "Switch all fonts off when FontCraft closes").changed() {
            self.dirty = true;
        }
        ui.add_space(6.0);
        if ui.button("Switch every font off now").clicked() {
            let all: Vec<String> = self.cfg.active.iter().cloned().collect();
            self.set_paths(all, false);
        }
    }

    fn settings_startup(&mut self, ui: &mut Ui, p: &Pal) {
        let tray_ok = cfg!(windows);
        heading(ui, p, "SYSTEM TRAY");
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
        heading(ui, p, "AUTOSTART");
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
        heading(ui, p, "UPDATES");
        if ui.checkbox(&mut self.cfg.check_updates, "Tell me when a new version of FontCraft is out").changed() {
            self.dirty = true;
        }
    }

    fn settings_auto(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        heading(ui, p, "FONTS FOR YOUR DOCUMENTS");
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
            .checkbox(&mut self.cfg.auto_google, "Download missing fonts from Google Fonts, Fontshare or Font Squirrel when a document needs them")
            .changed()
        {
            self.dirty = true;
        }
    }

    fn settings_sync(&mut self, ui: &mut Ui, p: &Pal) {
        heading(ui, p, "BETWEEN YOUR COMPUTERS");
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
        ui.add_space(8.0);
        let logo = self
            .logo
            .get_or_insert_with(|| {
                let i = crate::icon_sized(96);
                let img = egui::ColorImage::from_rgba_unmultiplied([i.width as usize, i.height as usize], &i.rgba);
                ui.ctx().load_texture("fontcraft-logo", img, egui::TextureOptions::LINEAR)
            })
            .clone();
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&logo).fit_to_exact_size(vec2(64.0, 64.0)));
            ui.vertical(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("FontCraft {}", crate::updates::VERSION)).size(17.0).color(p.text));
                ui.label(RichText::new("Every feature included – free, no account and no ads.").color(p.accent));
            });
        });
        if let Some(url) = crate::updates::releases_url() {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.cfg.check_updates, "Tell me when a new version is out").changed() {
                    self.dirty = true;
                }
                ui.hyperlink_to("Download page", url);
            });
        }
        heading(ui, p, "TIPS");
        let tips = [
            "Your font folders are watched: fonts you copy into them appear within seconds, deleted ones disappear.".to_string(),
            "The filter button next to the search has Super search: find fonts by contrast, proportion, x-height and weight.".into(),
            "Providers › Google, Fontshare and Font Squirrel: click the dot next to any font there to download and activate it.".into(),
            "Drag fonts onto a collection, tag or Favorites in the sidebar.".into(),
            format!(
                "{} jumps to the search. Several words narrow it down (e.g. serif heavy variable).",
                shortcuts::label(shortcuts::Cmd::Search)
            ),
            format!("{} shows every keyboard shortcut.", shortcuts::label(shortcuts::Cmd::Help)),
            "Playground (in the font details) tries a font on a sample page and exports the CSS.".into(),
        ];
        for t in tips {
            ui.add(egui::Label::new(RichText::new(format!("•  {t}")).size(12.0)).wrap());
        }
        heading(ui, p, "YOUR SETTINGS");
        ui.label(RichText::new(format!("Kept in {}", config::data_dir().display())).size(11.5).color(p.weak));
    }
}
