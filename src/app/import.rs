//! Importing fonts: copied into the import folder, with a choice for fonts that are already in
//! the library (from the same place, or as another copy), an optional report afterwards, and the
//! options in Settings › Import.

use super::*;
use crate::config::{DupAction, FolderImport};
use egui::{vec2, Align2, RichText, Ui};
use std::path::Path;

/// A font being imported that is already in the library.
pub struct Dup {
    pub src: PathBuf,
    /// where it goes inside the import folder
    pub rel: PathBuf,
    /// the font already in the library
    pub existing: PathBuf,
}

pub struct ImportPlan {
    pub new: Vec<(PathBuf, PathBuf)>,
    /// already in the library from the very same file
    pub same_path: Vec<Dup>,
    /// another copy of the font is already in the library
    pub dups: Vec<Dup>,
    pub bad: Vec<(PathBuf, String)>,
    /// family names of everything in the import (for "put them in a new collection")
    pub families: BTreeSet<String>,
    pub same_choice: DupAction,
    pub dup_choice: DupAction,
    pub remember: bool,
}

#[derive(Default)]
pub struct ImportReport {
    pub imported: Vec<PathBuf>,
    pub duplicates: Vec<PathBuf>,
    pub used: Vec<PathBuf>,
    pub replaced: Vec<PathBuf>,
    pub reread: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
    /// "replace" for a font outside the import folder: the new one was added, the person's own file kept
    pub kept_original: Vec<PathBuf>,
    pub bad: Vec<(PathBuf, String)>,
    pub collection: Option<String>,
}

pub fn dup_label(a: DupAction, same_path: bool) -> &'static str {
    match (a, same_path) {
        (DupAction::ImportDuplicate, _) => "Import the font as a duplicate",
        (DupAction::UseExisting, _) => "Use the font already in the library",
        (DupAction::Replace, true) => "Read the font again (it may have changed)",
        (DupAction::Replace, false) => "Replace the font already in the library with the new one",
        (DupAction::Skip, _) => "Skip the font",
    }
}

const SAME_PATH_CHOICES: [DupAction; 3] = [DupAction::UseExisting, DupAction::Replace, DupAction::Skip];
const DUP_CHOICES: [DupAction; 4] = [DupAction::ImportDuplicate, DupAction::UseExisting, DupAction::Replace, DupAction::Skip];

/// A free name for `target` (an identical file already there counts as free).
fn free_name(src: &Path, target: PathBuf) -> PathBuf {
    let same = |p: &Path| std::fs::metadata(p).map(|m| Some(m.len()) == std::fs::metadata(src).ok().map(|s| s.len())).unwrap_or(false);
    if !target.exists() || same(&target) {
        return target;
    }
    let stem = target.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = target.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    for k in 2..1000 {
        let t = target.with_file_name(format!("{stem} ({k}){ext}"));
        if !t.exists() || same(&t) {
            return t;
        }
    }
    target
}

fn copy_into(src: &Path, target: PathBuf, keep_both: bool) -> Result<PathBuf, String> {
    let t = if keep_both { free_name(src, target) } else { target };
    if let Some(d) = t.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    if t.exists() && std::fs::metadata(&t).map(|m| m.len()).ok() == std::fs::metadata(src).map(|m| m.len()).ok() {
        return Ok(t);
    }
    std::fs::copy(src, &t).map_err(|e| e.to_string())?;
    Ok(t)
}

/// The date for the name of an "Imported" collection (year-month-day).
fn today() -> String {
    let days = (now() / 86_400) as i64;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicates_get_free_names_and_identical_files_are_reused() {
        let d = std::env::temp_dir().join("fontcraft-import-test");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        let a = d.join("src").join("Font.ttf");
        std::fs::write(&a, b"one font").unwrap();
        let first = copy_into(&a, d.join("lib").join("Font.ttf"), true).unwrap();
        assert_eq!(first, d.join("lib").join("Font.ttf"));
        // the same file again: nothing new
        assert_eq!(copy_into(&a, d.join("lib").join("Font.ttf"), true).unwrap(), first);
        // a different font with the same file name: kept next to it
        let b = d.join("src").join("Other.ttf");
        std::fs::write(&b, b"another, longer font").unwrap();
        assert_eq!(copy_into(&b, d.join("lib").join("Font.ttf"), true).unwrap(), d.join("lib").join("Font (2).ttf"));
        let _ = std::fs::remove_dir_all(&d);
        let t = today();
        assert_eq!(t.len(), 10, "{t}");
        assert!(t.starts_with("20"), "{t}");
    }
}

impl FontCraftApp {
    /// Folders dropped onto the window or imported: added where they are, or copied (Settings › Import).
    pub(super) fn import_folders(&mut self, dirs: Vec<PathBuf>) {
        let items = self.folder_items(dirs);
        if !items.is_empty() {
            self.import_start(items);
        }
    }

    /// The fonts of folders to copy (empty when folders are added where they are – that happens here).
    pub(super) fn folder_items(&mut self, dirs: Vec<PathBuf>) -> Vec<(PathBuf, PathBuf)> {
        if self.cfg.import_folders == FolderImport::AddInPlace {
            for d in dirs {
                self.add_folder(d);
            }
            return vec![];
        }
        let keep = self.cfg.import_folders == FolderImport::CopyKeepStructure;
        let mut items = vec![];
        for d in dirs {
            let top = d.file_name().map(PathBuf::from).unwrap_or_default();
            for e in walkdir::WalkDir::new(&d).into_iter().filter_map(|e| e.ok()) {
                if !e.file_type().is_file() || !library::is_font_file(e.path()) {
                    continue;
                }
                let rel = if keep {
                    top.join(e.path().strip_prefix(&d).unwrap_or(e.path()))
                } else {
                    PathBuf::from(e.file_name())
                };
                items.push((e.path().to_path_buf(), rel));
            }
        }
        items
    }

    /// Look at what is being imported, then ask about fonts already in the library or go ahead.
    pub(super) fn import_start(&mut self, items: Vec<(PathBuf, PathBuf)>) {
        if items.is_empty() {
            self.set_status("No font files to import");
            return;
        }
        let mut by_key: HashMap<String, PathBuf> = HashMap::new();
        let mut paths: HashSet<String> = HashSet::new();
        for f in &self.faces {
            let m = &f.meta;
            by_key.entry(library::font_key(&m.postscript, &m.full_name, &m.style)).or_insert_with(|| m.path.clone());
            paths.insert(f.path_key.to_lowercase());
        }
        let mut plan = ImportPlan {
            new: vec![],
            same_path: vec![],
            dups: vec![],
            bad: vec![],
            families: BTreeSet::new(),
            same_choice: self.cfg.dup_same_path,
            dup_choice: self.cfg.dup_action,
            remember: false,
        };
        let mut seen: HashSet<String> = HashSet::new();
        for (src, rel) in items {
            let key = src.to_string_lossy().to_lowercase();
            if !seen.insert(key.clone()) {
                continue;
            }
            if !library::is_font_file(&src) {
                plan.bad.push((src, "not a font file".into()));
                continue;
            }
            let names = library::quick_names(&src);
            if names.is_empty() {
                plan.bad.push((src, "could not be read as a font".into()));
                continue;
            }
            for (_, fam) in &names {
                if !fam.is_empty() {
                    plan.families.insert(fam.clone());
                }
            }
            if paths.contains(&key) {
                plan.same_path.push(Dup { existing: src.clone(), src, rel });
            } else if let Some(existing) = names.iter().find_map(|(k, _)| by_key.get(k)) {
                plan.dups.push(Dup { src, rel, existing: existing.clone() });
            } else {
                plan.new.push((src, rel));
            }
        }
        let ask = (!plan.same_path.is_empty() && !self.cfg.dup_same_path_quiet) || (!plan.dups.is_empty() && !self.cfg.dup_quiet);
        if ask && self.shot.is_none() {
            self.import_ask = Some(plan);
        } else {
            self.import_run(plan);
        }
    }

    fn import_run(&mut self, plan: ImportPlan) {
        let root = self.cfg.import_dir();
        let mut rep = ImportReport { bad: plan.bad, ..Default::default() };
        let mut activate: Vec<PathBuf> = vec![];
        let mut forget: Vec<String> = vec![];
        let mut reread: Vec<String> = vec![];
        let mut keep_active: Vec<String> = vec![];
        for (src, rel) in plan.new {
            match copy_into(&src, root.join(&rel), true) {
                Ok(t) => {
                    activate.push(t.clone());
                    rep.imported.push(t);
                }
                Err(e) => rep.bad.push((src, e)),
            }
        }
        let had_same = !plan.same_path.is_empty();
        for d in plan.same_path {
            match plan.same_choice {
                DupAction::Skip => rep.skipped.push(d.src),
                DupAction::Replace => {
                    reread.push(d.existing.to_string_lossy().to_string());
                    activate.push(d.existing.clone());
                    rep.reread.push(d.existing);
                }
                _ => {
                    activate.push(d.existing.clone());
                    rep.used.push(d.existing);
                }
            }
        }
        let had_dups = !plan.dups.is_empty();
        for d in plan.dups {
            match plan.dup_choice {
                DupAction::Skip => rep.skipped.push(d.src),
                DupAction::UseExisting => {
                    activate.push(d.existing.clone());
                    rep.used.push(d.existing);
                }
                DupAction::ImportDuplicate => match copy_into(&d.src, root.join(&d.rel), true) {
                    Ok(t) => {
                        activate.push(t.clone());
                        rep.duplicates.push(t);
                    }
                    Err(e) => rep.bad.push((d.src, e)),
                },
                DupAction::Replace => {
                    if d.existing.starts_with(&root) {
                        // FontCraft's own copy in the import folder: the new file takes its place
                        let was_active = self.cfg.active.contains(&d.existing.to_string_lossy().to_string());
                        activation::deactivate(std::slice::from_ref(&d.existing));
                        let _ = std::fs::remove_file(&d.existing);
                        forget.push(d.existing.to_string_lossy().to_string());
                        match copy_into(&d.src, root.join(&d.rel), true) {
                            Ok(t) => {
                                // the new file takes over from an active old one in any case
                                if was_active {
                                    keep_active.push(t.to_string_lossy().to_string());
                                }
                                activate.push(t.clone());
                                rep.replaced.push(t);
                            }
                            Err(e) => rep.bad.push((d.src, e)),
                        }
                    } else {
                        // the person's own file elsewhere is never deleted: the new one is added next to it
                        match copy_into(&d.src, root.join(&d.rel), true) {
                            Ok(t) => {
                                activate.push(t.clone());
                                rep.kept_original.push(t);
                            }
                            Err(e) => rep.bad.push((d.src, e)),
                        }
                    }
                }
            }
        }
        if plan.remember {
            if had_same {
                self.cfg.dup_same_path_quiet = true;
                self.cfg.dup_same_path = plan.same_choice;
            }
            if had_dups {
                self.cfg.dup_quiet = true;
                self.cfg.dup_action = plan.dup_choice;
            }
        }
        for k in &forget {
            self.cfg.active.remove(k);
            self.cfg.added.remove(k);
        }
        if !reread.is_empty() {
            let mut cache = config::load_cache();
            for k in &reread {
                cache.remove(k);
            }
            config::save_cache(&cache);
        }
        let copied = rep.imported.len() + rep.duplicates.len() + rep.replaced.len() + rep.kept_original.len();
        if copied > 0 && !self.cfg.folders.iter().any(|f| root.starts_with(f)) {
            self.cfg.folders.push(root.clone());
        }
        if self.cfg.import_activate && !activate.is_empty() {
            keep_active.extend(activate.iter().map(|p| p.to_string_lossy().to_string()));
        }
        if !keep_active.is_empty() {
            self.set_paths(keep_active, true);
        }
        if self.cfg.import_collection && (copied > 0 || !rep.used.is_empty()) && !plan.families.is_empty() {
            let name = format!("Imported {}", today());
            match self.cfg.collections.iter_mut().find(|c| c.name == name) {
                Some(c) => {
                    c.families.extend(plan.families.iter().cloned());
                    c.modified = now();
                }
                None => {
                    let id = activation::fnv(&format!("{:?}{name}", Instant::now())) ^ now();
                    let n = self.cfg.collections.len();
                    self.cfg.collections.push(Collection {
                        id,
                        name: name.clone(),
                        color: COLLECTION_COLORS[n % COLLECTION_COLORS.len()],
                        families: plan.families.clone(),
                        sync_dir: None,
                        modified: now(),
                    });
                }
            }
            rep.collection = Some(name);
        }
        self.dirty = true;
        let mut parts = vec![];
        if copied > 0 {
            parts.push(format!("Imported {copied} font file{}", if copied == 1 { "" } else { "s" }));
        }
        if !rep.used.is_empty() {
            parts.push(format!("{} already in your library", rep.used.len()));
        }
        if !rep.skipped.is_empty() {
            parts.push(format!("{} skipped", rep.skipped.len()));
        }
        if !rep.bad.is_empty() {
            parts.push(format!("{} could not be imported", rep.bad.len()));
        }
        self.set_status(if parts.is_empty() { "Nothing was imported".to_string() } else { parts.join(" · ") });
        if copied > 0 {
            self.view = View::Folder(root);
        }
        self.rescan();
        if self.cfg.import_report && self.shot.is_none() {
            self.import_done = Some(rep);
        }
    }

    /// The question when fonts being imported are already in the library.
    pub(super) fn import_ask_window(&mut self, ctx: &egui::Context) {
        let Some(mut plan) = self.import_ask.take() else { return };
        let p = self.p();
        let mut open = true;
        let mut go: Option<bool> = None;
        egui::Window::new(RichText::new("Fonts already in your library").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(560.0);
                let total = plan.new.len() + plan.same_path.len() + plan.dups.len();
                ui.label(format!(
                    "{} of the {total} fonts you are importing {} already in FontCraft.",
                    plan.same_path.len() + plan.dups.len(),
                    if plan.same_path.len() + plan.dups.len() == 1 { "is" } else { "are" }
                ));
                if !plan.same_path.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("FROM THE SAME PLACE ({})", plan.same_path.len())).size(11.0).strong().color(p.weak));
                    Self::dup_list(ui, &p, "imp-same", &plan.same_path, true);
                    for a in SAME_PATH_CHOICES {
                        ui.radio_value(&mut plan.same_choice, a, dup_label(a, true));
                    }
                }
                if !plan.dups.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("ANOTHER COPY IS IN THE LIBRARY ({})", plan.dups.len())).size(11.0).strong().color(p.weak));
                    Self::dup_list(ui, &p, "imp-dup", &plan.dups, false);
                    for a in DUP_CHOICES {
                        ui.radio_value(&mut plan.dup_choice, a, dup_label(a, false));
                    }
                    if plan.dup_choice == DupAction::Replace {
                        ui.label(
                            RichText::new("Only copies in FontCraft's import folder are replaced; your own font files elsewhere are never deleted.")
                                .size(11.5)
                                .color(p.weak),
                        );
                    }
                }
                ui.add_space(8.0);
                ui.checkbox(&mut plan.remember, "Remember my choice and don't ask again (Settings › Import)");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let b = egui::Button::new(RichText::new("Import").color(p.on_accent).strong()).fill(p.accent);
                        if ui.add(b).clicked() {
                            go = Some(true);
                        }
                        if ui.button("Cancel import").clicked() {
                            go = Some(false);
                        }
                    });
                });
            });
        match go {
            Some(true) => self.import_run(plan),
            Some(false) => self.set_status("Import cancelled"),
            None if open => self.import_ask = Some(plan),
            None => self.set_status("Import cancelled"),
        }
    }

    fn dup_list(ui: &mut Ui, p: &Pal, id: &str, list: &[Dup], same: bool) {
        egui::ScrollArea::vertical().id_salt(id).max_height(110.0).auto_shrink([false, true]).show(ui, |ui| {
            for d in list {
                let name = d.src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let text = if same { name } else { format!("{name}   —   in the library: {}", d.existing.display()) };
                ui.add(egui::Label::new(RichText::new(text).size(12.0).color(p.text)).truncate());
            }
        });
    }

    /// What happened in the last import (Settings › Import › Show a report).
    pub(super) fn import_report_window(&mut self, ctx: &egui::Context) {
        let Some(rep) = &self.import_done else { return };
        let p = self.p();
        let mut open = true;
        let mut close = false;
        egui::Window::new(RichText::new("Import finished").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let groups: [(&str, Vec<String>); 8] = [
                    ("Imported", rep.imported.iter().map(|p| p.display().to_string()).collect()),
                    ("Imported as duplicates", rep.duplicates.iter().map(|p| p.display().to_string()).collect()),
                    ("Replaced the copy in the import folder", rep.replaced.iter().map(|p| p.display().to_string()).collect()),
                    ("Added next to your own copy (that file was kept)", rep.kept_original.iter().map(|p| p.display().to_string()).collect()),
                    ("Already in the library – used that one", rep.used.iter().map(|p| p.display().to_string()).collect()),
                    ("Read again", rep.reread.iter().map(|p| p.display().to_string()).collect()),
                    ("Skipped", rep.skipped.iter().map(|p| p.display().to_string()).collect()),
                    ("Could not be imported", rep.bad.iter().map(|(p, e)| format!("{} – {e}", p.display())).collect()),
                ];
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for (title, list) in groups {
                        if list.is_empty() {
                            continue;
                        }
                        egui::CollapsingHeader::new(RichText::new(format!("{title}: {}", list.len())).strong())
                            .id_salt(title)
                            .default_open(list.len() <= 6)
                            .show(ui, |ui| {
                                for l in list {
                                    ui.add(egui::Label::new(RichText::new(l).size(12.0)).truncate());
                                }
                            });
                    }
                    if let Some(c) = &rep.collection {
                        ui.label(RichText::new(format!("The fonts were put into the collection “{c}”.")).color(p.accent));
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("You can switch this report off in Settings › Import.").size(11.5).color(p.weak));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("  OK  ").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if !open || close {
            self.import_done = None;
        }
    }

    /// Settings › Import.
    pub(super) fn settings_import(&mut self, ui: &mut Ui, p: &Pal) {
        let heading = |ui: &mut Ui, s: &str| {
            ui.add_space(14.0);
            ui.label(RichText::new(s).size(11.0).strong().color(p.weak));
            ui.add_space(2.0);
        };
        ui.add(
            egui::Label::new(
                RichText::new(format!(
                    "Imported fonts are copied into your import folder ({}). Folders you add with “Add a folder” stay where they are.",
                    self.cfg.import_dir().display()
                ))
                .size(12.0)
                .color(p.weak),
            )
            .wrap(),
        );
        heading(ui, "AFTER IMPORTING");
        let mut ch = false;
        ch |= ui.checkbox(&mut self.cfg.import_activate, "Activate imported fonts").changed();
        ch |= ui.checkbox(&mut self.cfg.import_report, "Show a report after importing").changed();
        ch |= ui.checkbox(&mut self.cfg.import_collection, "Put imported fonts into a new collection (“Imported” with the date)").changed();
        heading(ui, "WHEN IMPORTING FOLDERS");
        for (m, l) in [
            (FolderImport::AddInPlace, "Add the folder to the library where it is (nothing is copied)"),
            (FolderImport::CopyTogether, "Copy all its fonts into the import folder, together"),
            (FolderImport::CopyKeepStructure, "Copy its fonts into the import folder and keep its folder structure"),
        ] {
            ch |= ui.radio_value(&mut self.cfg.import_folders, m, l).changed();
        }
        heading(ui, "FONTS ALREADY IN THE LIBRARY");
        ch |= ui.checkbox(&mut self.cfg.dup_same_path_quiet, "Don't ask if a font is already in the library from the same place").changed();
        ui.indent("dup-same", |ui| {
            for a in SAME_PATH_CHOICES {
                ch |= ui.radio_value(&mut self.cfg.dup_same_path, a, dup_label(a, true)).changed();
            }
        });
        ui.add_space(6.0);
        ch |= ui.checkbox(&mut self.cfg.dup_quiet, "Don't ask if a font is already in the library").changed();
        ui.indent("dup-other", |ui| {
            for a in DUP_CHOICES {
                ch |= ui.radio_value(&mut self.cfg.dup_action, a, dup_label(a, false)).changed();
            }
        });
        ui.add(
            egui::Label::new(
                RichText::new(
                    "A font counts as already in the library when a font with the same PostScript name is there. When FontCraft \
                     asks, the choice above is ready in the question. “Replace” only replaces copies in the import folder – your own \
                     font files elsewhere are never deleted.",
                )
                .size(12.0)
                .color(p.weak),
            )
            .wrap(),
        );
        if ch {
            self.dirty = true;
        }
        heading(ui, "IMPORT NOW");
        ui.horizontal(|ui| {
            if ui.button("Import fonts…").clicked() {
                if let Some(files) = rfd::FileDialog::new().set_title("Import fonts").add_filter("Fonts", library::FONT_EXTS).pick_files() {
                    self.import_files(files);
                }
            }
            if ui.button("Import a folder…").clicked() {
                if let Some(d) = rfd::FileDialog::new().set_title("Import a folder of fonts").pick_folder() {
                    self.import_folders(vec![d]);
                }
            }
        });
    }
}
