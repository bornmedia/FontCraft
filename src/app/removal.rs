//! The "Delete fonts" window (with its own warning for online, folder and system fonts)
//! and the "Add a network folder" window.

use super::*;
use crate::{netfs, remove};
use egui::{vec2, Align2, RichText, Ui};
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
pub enum SysChoice {
    Move,
    Delete,
}

pub struct OnlineFam {
    pub name: String,
    pub provider: Provider,
    /// the family's folder inside FontCraft's download folder
    pub dir: PathBuf,
    pub files: Vec<PathBuf>,
}

pub struct SysFont {
    pub path: PathBuf,
    pub family: String,
    /// the operating system uses it itself
    pub os: bool,
    /// macOS doesn't allow it to be deleted at all
    pub protected: bool,
}

pub struct DeleteDialog {
    pub title: String,
    pub online: Vec<OnlineFam>,
    /// false: only switch them off and keep the downloaded copy
    pub online_delete: bool,
    pub folder: Vec<PathBuf>,
    pub folder_network: bool,
    pub system: Vec<SysFont>,
    pub sys_choice: SysChoice,
    pub move_dir: String,
    pub skip_os: bool,
    pub activate_moved: bool,
    pub running: bool,
}

pub struct DelResult {
    pub outcome: remove::Outcome,
    pub online_removed: Vec<String>,
    pub online_off: Vec<String>,
    pub moved_dir: Option<PathBuf>,
    pub activate_moved: bool,
}

pub struct NetDialog {
    pub address: String,
    pub error: Option<String>,
    pub busy: Option<Receiver<Result<PathBuf, String>>>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn file_list(ui: &mut Ui, p: &Pal, id: &str, items: Vec<(String, Option<&str>)>) {
    egui::ScrollArea::vertical().id_salt(id).max_height(96.0).auto_shrink([false, true]).show(ui, |ui| {
        for (name, warn) in items {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new(name).size(12.5).color(p.text));
                if let Some(w) = warn {
                    ui.label(RichText::new(format!("⚠ {w}")).size(11.5).color(Color32::from_rgb(0xf0, 0xa0, 0x30)));
                }
            });
        }
    });
}

fn warning_box(ui: &mut Ui, color: Color32, text: &str) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).size(12.5).color(color));
        });
}

const RED: Color32 = Color32::from_rgb(0xff, 0x5c, 0x5c);
const ORANGE: Color32 = Color32::from_rgb(0xf0, 0xa0, 0x30);

fn os_name() -> &'static str {
    if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "your Linux system"
    }
}

impl FontCraftApp {
    /// Is this library folder on the network? (worked out while scanning)
    pub(super) fn is_net_folder(&self, p: &Path) -> bool {
        self.net_folders.iter().any(|n| p.starts_with(n))
    }

    /// The folder system fonts are moved into: the one used last time, else a folder on this
    /// computer that is already in the library, else a new one. Second value: already in the library.
    fn default_move_dir(&self) -> (PathBuf, bool) {
        let online: Vec<PathBuf> = Provider::ALL.iter().map(|p| p.fonts_dir()).collect();
        let local = |d: &PathBuf| !self.is_net_folder(d) && !self.offline.contains(d) && !online.contains(d);
        if let Some(d) = &self.cfg.moved_dir {
            if self.cfg.folders.contains(d) && local(d) {
                return (d.clone(), true);
            }
        }
        if let Some(d) = self.cfg.folders.iter().find(|d| local(d)) {
            return (d.clone(), true);
        }
        (config::data_dir().join("Moved system fonts"), false)
    }

    /// Open the "Delete fonts" window for families of the library.
    pub(super) fn open_delete(&mut self, fams: Vec<usize>) {
        let fams: Vec<usize> = fams.into_iter().filter(|f| *f < self.families.len()).collect();
        if fams.is_empty() || (self.shot.is_some() && std::env::var("FONTCRAFT_SHOW").is_err()) {
            return;
        }
        let gdirs: Vec<(Provider, PathBuf)> = Provider::ALL.iter().map(|p| (*p, p.fonts_dir())).collect();
        let mut seen = HashSet::new();
        let mut online: Vec<OnlineFam> = vec![];
        let mut folder = vec![];
        let mut system = vec![];
        for fi in &fams {
            let fam = &self.families[*fi];
            for x in &fam.files {
                let face = &self.faces[*x];
                let path = face.meta.path.clone();
                if !seen.insert(path.clone()) {
                    continue;
                }
                if face.system {
                    system.push(SysFont {
                        os: remove::is_os_font(&path, &face.meta.family) || remove::is_os_font(&path, &fam.name),
                        protected: remove::is_protected(&path),
                        family: fam.name.clone(),
                        path,
                    });
                    continue;
                }
                let og = gdirs.iter().find_map(|(p, d)| {
                    let rest = path.strip_prefix(d).ok()?;
                    let first = rest.components().next()?;
                    Some((*p, d.join(first.as_os_str())))
                });
                match og {
                    Some((prov, dir)) => match online.iter_mut().find(|o| o.dir == dir) {
                        Some(o) => o.files.push(path),
                        None => online.push(OnlineFam {
                            name: fam.name.clone(),
                            provider: prov,
                            dir,
                            files: vec![path],
                        }),
                    },
                    None => folder.push(path),
                }
            }
        }
        let title = if fams.len() == 1 { self.families[fams[0]].name.clone() } else { plural(fams.len(), "family", "families") };
        self.show_delete(title, online, folder, system);
    }

    /// Open the "Delete fonts" window for a family of an online source.
    pub(super) fn open_delete_online(&mut self, gi: usize) {
        let Some(g) = self.gcat.get(gi).cloned() else { return };
        let files: Vec<PathBuf> = self.gfam_faces(gi).iter().map(|f| self.faces[*f].meta.path.clone()).collect();
        let online = vec![OnlineFam { name: g.name.clone(), provider: g.provider, dir: g.dir(), files }];
        self.show_delete(g.name, online, vec![], vec![]);
    }

    fn show_delete(&mut self, title: String, online: Vec<OnlineFam>, folder: Vec<PathBuf>, system: Vec<SysFont>) {
        let (dir, _) = self.default_move_dir();
        let folder_network = folder.iter().any(|f| self.is_net_folder(f));
        self.delete = Some(DeleteDialog {
            title,
            online,
            online_delete: false,
            folder,
            folder_network,
            system,
            sys_choice: SysChoice::Move,
            move_dir: dir.display().to_string(),
            skip_os: true,
            activate_moved: false,
            running: false,
        });
    }

    pub(super) fn delete_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.delete.take() else { return };
        let p = self.p();
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        let (default_dir, _) = self.default_move_dir();
        let local_folders: Vec<PathBuf> = self
            .cfg
            .folders
            .iter()
            .filter(|f| !self.is_net_folder(f) && !self.offline.contains(*f))
            .cloned()
            .collect();
        egui::Window::new(RichText::new("Delete fonts").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(540.0);
                ui.add_enabled_ui(!d.running, |ui| {
                    ui.label(RichText::new(format!("Delete {}?", d.title)).size(17.0).color(p.text));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(ctx.screen_rect().height() * 0.62).show(ui, |ui| {
                        if !d.online.is_empty() {
                            self.delete_online_section(ui, &p, &mut d);
                        }
                        if !d.folder.is_empty() {
                            Self::delete_folder_section(ui, &p, &d);
                        }
                        if !d.system.is_empty() {
                            self.delete_system_section(ui, &p, &mut d, &default_dir, &local_folders);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.separator();
                let (perm, moved, online_del, online_off) = Self::delete_counts(&d);
                if d.running {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        let admin = !d.system.is_empty();
                        ui.label(RichText::new(if admin {
                            "Working… your system may ask for administrator permission."
                        } else {
                            "Working…"
                        }).color(p.weak));
                    });
                    return;
                }
                if perm > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{} will be deleted permanently. This can't be undone. Are you sure?",
                            plural(perm, "file", "files")
                        ))
                        .color(RED)
                        .strong(),
                    );
                }
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if perm > 0 {
                            format!("Yes, delete {}", plural(perm, "file", "files"))
                                + &if moved > 0 { format!(" and move {moved}") } else { String::new() }
                        } else if moved > 0 {
                            format!("Move {}", plural(moved, "file", "files"))
                        } else if online_del > 0 {
                            format!("Deactivate and delete local {}", if online_del == 1 { "copy" } else { "copies" })
                        } else if online_off > 0 {
                            "Deactivate".to_string()
                        } else {
                            "Nothing to do".to_string()
                        };
                        let nothing = perm + moved + online_del + online_off == 0;
                        let fill = if perm > 0 { RED.gamma_multiply(0.85) } else { p.accent };
                        let tc = if perm > 0 { Color32::WHITE } else { p.on_accent };
                        let b = egui::Button::new(RichText::new(label).color(tc).strong()).fill(fill);
                        if ui.add_enabled(!nothing, b).clicked() {
                            go = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if go {
            d.running = true;
            self.run_delete(&d);
        }
        if (open && !cancel) || d.running {
            self.delete = Some(d);
        }
    }

    /// (files deleted for good, system files moved, online copies deleted, online families only switched off)
    fn delete_counts(d: &DeleteDialog) -> (usize, usize, usize, usize) {
        let sys = Self::system_targets(d).len();
        let (sys_del, sys_move) = if d.sys_choice == SysChoice::Delete { (sys, 0) } else { (0, sys) };
        let (od, oo) = if d.online_delete { (d.online.len(), 0) } else { (0, d.online.len()) };
        (d.folder.len() + sys_del, sys_move, od, oo)
    }

    fn system_targets(d: &DeleteDialog) -> Vec<PathBuf> {
        d.system.iter().filter(|s| !s.protected && !(d.skip_os && s.os)).map(|s| s.path.clone()).collect()
    }

    fn delete_online_section(&self, ui: &mut Ui, p: &Pal, d: &mut DeleteDialog) {
        ui.add_space(6.0);
        let sources: BTreeSet<&str> = d.online.iter().map(|o| o.provider.label()).collect();
        let sources = sources.into_iter().collect::<Vec<_>>().join(", ");
        ui.label(RichText::new(format!("From {sources} — {}", plural(d.online.len(), "family", "families"))).strong().color(p.text));
        file_list(ui, p, "del-online", d.online.iter().map(|o| (o.name.clone(), None)).collect());
        let downloaded = d.online.iter().filter(|o| !o.files.is_empty()).count();
        if downloaded == 0 {
            warning_box(
                ui,
                p.accent,
                "These fonts aren't downloaded, so there is nothing on your computer to delete – they are only \
                 deactivated. They stay in the online list.",
            );
            d.online_delete = false;
            return;
        }
        d.online_delete = true;
        warning_box(
            ui,
            p.accent,
            &format!(
                "This deactivates them and deletes the local copy FontCraft downloaded, so your apps can't use them \
                 any more. They stay in the {sources} list: to use them again, activate them there – FontCraft then \
                 downloads them again."
            ),
        );
    }

    fn delete_folder_section(ui: &mut Ui, p: &Pal, d: &DeleteDialog) {
        ui.add_space(10.0);
        ui.label(RichText::new(format!("From your folders — {}", plural(d.folder.len(), "file", "files"))).strong().color(p.text));
        file_list(
            ui,
            p,
            "del-folder",
            d.folder.iter().map(|f| (f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), None)).collect(),
        );
        let bin = if cfg!(windows) { "the Recycle Bin" } else { "the Trash" };
        warning_box(
            ui,
            RED,
            &format!("⚠ These files are deleted from your disk permanently. They don't go to {bin} – this is irreversible."),
        );
        if d.folder_network {
            ui.add_space(4.0);
            warning_box(
                ui,
                ORANGE,
                "Some of them are in a network folder: they disappear for everyone who uses that folder, not just for you.",
            );
        }
    }

    fn delete_system_section(&self, ui: &mut Ui, p: &Pal, d: &mut DeleteDialog, default_dir: &Path, local_folders: &[PathBuf]) {
        ui.add_space(10.0);
        ui.label(RichText::new(format!("System fonts — {}", plural(d.system.len(), "file", "files"))).strong().color(p.text));
        file_list(
            ui,
            p,
            "del-system",
            d.system
                .iter()
                .map(|s| {
                    let name = s.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let warn = if s.protected {
                        Some("protected by macOS – left out")
                    } else if s.os {
                        Some("used by the system")
                    } else {
                        None
                    };
                    (format!("{name}  ({})", s.family), warn)
                })
                .collect(),
        );
        warning_box(ui, ORANGE, &format!("⚠ Careful: {}", remove::os_warning()));
        let os_count = d.system.iter().filter(|s| s.os && !s.protected).count();
        if os_count > 0 {
            ui.checkbox(
                &mut d.skip_os,
                format!("Leave out the {} that {} uses itself (recommended)", plural(os_count, "font", "fonts"), os_name()),
            );
        }
        ui.add_space(6.0);
        ui.radio_value(&mut d.sys_choice, SysChoice::Move, RichText::new("Move them to a folder in FontCraft instead (recommended)").strong());
        if d.sys_choice == SysChoice::Move {
            ui.indent("move-dir", |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.move_dir).desired_width(330.0));
                    if ui.button("Choose…").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().set_title("Move the system fonts into").pick_folder() {
                            d.move_dir = dir.display().to_string();
                        }
                    }
                });
                if local_folders.len() > 1 {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Your folders:").size(11.5).color(p.weak));
                        for f in local_folders {
                            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.display().to_string());
                            if ui.small_button(name).on_hover_text(f.display().to_string()).clicked() {
                                d.move_dir = f.display().to_string();
                            }
                        }
                    });
                }
                let target = PathBuf::from(d.move_dir.trim());
                let covered = self.cfg.folders.iter().any(|f| target.starts_with(f));
                let note = if d.move_dir.trim().is_empty() {
                    "Choose a folder".to_string()
                } else if covered {
                    "This folder is already in your library.".to_string()
                } else if target == default_dir {
                    "A new folder – FontCraft adds it to your Folders.".to_string()
                } else {
                    "FontCraft adds this folder to your Folders.".to_string()
                };
                ui.label(RichText::new(note).size(11.5).color(p.weak));
                ui.checkbox(&mut d.activate_moved, "Activate them in FontCraft right away, so your apps keep them");
            });
        }
        ui.radio_value(&mut d.sys_choice, SysChoice::Delete, "Delete them permanently");
        if d.sys_choice == SysChoice::Delete {
            warning_box(ui, RED, "⚠ The font files are deleted from the system permanently. This is irreversible.");
        }
    }

    fn run_delete(&mut self, d: &DeleteDialog) {
        let online: Vec<(String, PathBuf, Vec<PathBuf>)> = d.online.iter().map(|o| (o.name.clone(), o.dir.clone(), o.files.clone())).collect();
        let online_delete = d.online_delete;
        let folder = d.folder.clone();
        let system = Self::system_targets(d);
        let move_to = if d.sys_choice == SysChoice::Move && !system.is_empty() {
            Some(PathBuf::from(d.move_dir.trim())).filter(|p| !p.as_os_str().is_empty())
        } else {
            None
        };
        let activate_moved = d.activate_moved;
        // online families are switched off in any case (the files themselves go in the background)
        for (_, _, files) in &online {
            for f in files {
                self.cfg.active.remove(&f.to_string_lossy().to_string());
            }
        }
        self.dirty = true;
        self.refresh_active();
        let system_move_missing = d.sys_choice == SysChoice::Move && move_to.is_none() && !system.is_empty();
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let mut outcome = remove::Outcome::default();
            let mut online_removed = vec![];
            let mut online_off = vec![];
            for (name, dir, files) in online {
                activation::deactivate(&files);
                if online_delete && !files.is_empty() {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    match std::fs::remove_dir_all(&dir) {
                        Ok(()) => {
                            outcome.removed.extend(files);
                            online_removed.push(name);
                        }
                        Err(e) => outcome.failed.push((dir, e.to_string())),
                    }
                } else {
                    online_off.push(name);
                }
            }
            let merge = |o: &mut remove::Outcome, x: remove::Outcome| {
                o.removed.extend(x.removed);
                o.after_restart.extend(x.after_restart);
                o.failed.extend(x.failed);
                o.cancelled |= x.cancelled;
                o.moved.extend(x.moved);
            };
            if !folder.is_empty() {
                let x = remove::remove_files(&folder, None, false);
                merge(&mut outcome, x);
            }
            if system_move_missing {
                for f in &system {
                    outcome.failed.push((f.clone(), "no folder was chosen to move it into".into()));
                }
            } else if !system.is_empty() {
                let x = remove::remove_files(&system, move_to.as_deref(), true);
                merge(&mut outcome, x);
            }
            let _ = tx.send(DelResult { outcome, online_removed, online_off, moved_dir: move_to, activate_moved });
            ctx.request_repaint();
        });
        self.del_rx = Some(rx);
    }

    /// The deletion finished: tidy up the settings, report and rescan.
    pub(super) fn poll_delete(&mut self) {
        let Some(rx) = &self.del_rx else { return };
        let Ok(r) = rx.try_recv() else { return };
        self.del_rx = None;
        self.delete = None;
        let o = &r.outcome;
        let gone: Vec<&PathBuf> = o.removed.iter().chain(o.after_restart.iter()).collect();
        for g in &gone {
            let k = g.to_string_lossy().to_string();
            self.cfg.active.remove(&k);
            self.cfg.added.remove(&k);
            let prefix = format!("{k}#");
            self.cfg.family_overrides.retain(|key, _| !key.starts_with(&prefix));
        }
        if let Some(dir) = &r.moved_dir {
            if !o.moved.is_empty() {
                if !self.cfg.folders.iter().any(|f| dir.starts_with(f)) {
                    self.cfg.folders.push(dir.clone());
                }
                self.cfg.moved_dir = Some(dir.clone());
                if r.activate_moved {
                    let new: Vec<PathBuf> = o.moved.iter().map(|(_, n)| n.clone()).collect();
                    for n in &new {
                        self.cfg.active.insert(n.to_string_lossy().to_string());
                    }
                    self.spawn_activation(new, true, false);
                }
            }
        }
        self.dirty = true;
        self.refresh_active();
        self.selected.clear();
        self.primary = None;
        self.gsel = None;
        // the message
        let moved = o.moved.len();
        let deleted = gone.len().saturating_sub(moved);
        let mut parts = vec![];
        if deleted > 0 {
            parts.push(format!("Deleted {}", plural(deleted, "font file", "font files")));
        }
        if moved > 0 {
            let dir = r.moved_dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
            parts.push(format!("moved {} to {dir}", plural(moved, "system font", "system fonts")));
        }
        if !r.online_off.is_empty() && r.online_removed.is_empty() && deleted == 0 {
            parts.push(format!("Deactivated {}", r.online_off.join(", ")));
        }
        if !o.after_restart.is_empty() {
            parts.push(format!("{} still in use – gone after the next restart", plural(o.after_restart.len(), "file is", "files are")));
        }
        if o.cancelled {
            parts.push("administrator permission was not given".into());
        }
        if !o.failed.is_empty() {
            let (f, why) = &o.failed[0];
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            parts.push(format!("{} could not be removed ({name}: {why})", plural(o.failed.len(), "file", "files")));
        }
        let mut msg = parts.join(" · ");
        if let Some(c) = msg.get(0..1) {
            msg = c.to_uppercase() + &msg[1..];
        }
        if !o.failed.is_empty() || !o.after_restart.is_empty() {
            self.del_report = Some(Self::report_text(o));
        }
        self.set_status(if msg.is_empty() { "Nothing was deleted".to_string() } else { msg });
        self.rescan();
    }

    fn report_text(o: &remove::Outcome) -> String {
        let mut s = String::new();
        if !o.after_restart.is_empty() {
            s.push_str("Still in use – removed at the next restart:\n");
            for f in &o.after_restart {
                s.push_str(&format!("   {}\n", f.display()));
            }
            s.push('\n');
        }
        if !o.failed.is_empty() {
            s.push_str("Could not be removed:\n");
            for (f, why) in &o.failed {
                s.push_str(&format!("   {} – {why}\n", f.display()));
            }
        }
        s
    }

    pub(super) fn delete_report_window(&mut self, ctx: &egui::Context) {
        let Some(text) = self.del_report.clone() else { return };
        let mut open = true;
        let mut close = false;
        egui::Window::new("Some fonts were not removed")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    ui.label(RichText::new(text).size(12.5));
                });
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
        if !open || close {
            self.del_report = None;
        }
    }

    // -------------------------------------------------------------- network folders

    pub(super) fn network_window(&mut self, ctx: &egui::Context) {
        let Some(mut nd) = self.netdlg.take() else { return };
        let p = self.p();
        if let Some(rx) = &nd.busy {
            match rx.try_recv() {
                Ok(Ok(path)) => {
                    self.add_folder(path.clone());
                    self.set_status(format!("Added network folder {}", path.display()));
                    return;
                }
                Ok(Err(e)) => {
                    nd.error = Some(e);
                    nd.busy = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                Err(_) => nd.busy = None,
            }
        }
        let mut open = true;
        let mut cancel = false;
        let mut start = false;
        egui::Window::new(RichText::new("Add a network folder").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label("The address of a shared folder on a server, a NAS or another computer:");
                let busy = nd.busy.is_some();
                let r = ui.add_enabled(
                    !busy,
                    egui::TextEdit::singleline(&mut nd.address)
                        .desired_width(f32::INFINITY)
                        .hint_text(if cfg!(windows) { r"\\server\fonts" } else { "smb://server/fonts" }),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    start = true;
                }
                ui.label(RichText::new(format!("For example:  {}", netfs::examples())).size(11.5).color(p.weak));
                if let Some(e) = &nd.error {
                    ui.add_space(4.0);
                    warning_box(ui, RED, e);
                }
                ui.add_space(6.0);
                let tip = if cfg!(windows) {
                    "If the server asks for a password, open the address once in File Explorer and tick “Remember my credentials”."
                } else if cfg!(target_os = "macos") {
                    "smb:// addresses are connected through the Finder, which asks for the password if the server needs one."
                } else {
                    "smb:// addresses are connected through your file manager (GIO), which asks for the password if needed."
                };
                ui.label(RichText::new(tip).size(11.5).color(p.weak));
                ui.label(
                    RichText::new(
                        "Fonts in a network folder are activated like any others. When the network is away, FontCraft \
                         remembers which of them were active and switches them on again when it's back.",
                    )
                    .size(11.5)
                    .color(p.weak),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Browse…")).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().set_title("Choose a network folder").pick_folder() {
                            nd.address = dir.display().to_string();
                            nd.error = None;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let b = egui::Button::new(RichText::new("Add folder").color(p.on_accent).strong()).fill(p.accent);
                        if ui.add_enabled(!busy && !nd.address.trim().is_empty(), b).clicked() {
                            start = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        if busy {
                            ui.spinner();
                            ui.label(RichText::new("Connecting…").color(p.weak));
                        }
                    });
                });
            });
        if start && nd.busy.is_none() && !nd.address.trim().is_empty() {
            let (tx, rx) = channel();
            let addr = nd.address.clone();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                let r = netfs::connect(&addr, std::time::Duration::from_secs(40)).and_then(|p| {
                    if netfs::reachable(&p, std::time::Duration::from_secs(10)) {
                        Ok(p)
                    } else {
                        Err(format!(
                            "{} can't be reached. Check the address, that you are connected to the network, and that you \
                             have signed in to the server.",
                            p.display()
                        ))
                    }
                });
                let _ = tx.send(r);
                ctx2.request_repaint();
            });
            nd.error = None;
            nd.busy = Some(rx);
        }
        if open && !cancel {
            self.netdlg = Some(nd);
        }
    }
}
