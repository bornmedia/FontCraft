//! FontCraft: state, library logic and the frame loop. Drawing lives in `ui` and `details`.

mod details;
mod finder;
mod gview;
mod list;
mod misc;
mod playground;
mod removal;
mod settings;
mod sidebar;
mod ui;

pub use finder::is_image;

use crate::activation;
use crate::google::{self, GFamily, GMsg, Google, Provider};
use crate::watcher;
use crate::{autoact, sync};
use crate::config::{self, Collection, Config, SortBy, Theme};
use crate::extras;
use crate::fontload::FontLoader;
use crate::library::{self, FaceMeta, GlyphInfo, Origin, ScanCache, ScanMsg};
use egui::{Color32, CornerRadius, Stroke};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ------------------------------------------------------------------ data

pub struct Face {
    pub meta: FaceMeta,
    pub id: u64,
    pub system: bool,
    pub folder: Option<usize>,
    pub path_key: String,
}

pub struct Family {
    pub name: String,
    pub lower: String,
    pub faces: Vec<usize>,
    /// every face of the family, including copies of the same style in other files
    pub files: Vec<usize>,
    pub primary: usize,
    pub system: bool,
    pub added: u64,
    pub variable: bool,
    pub mono: bool,
    pub italic: bool,
    pub ext: String,
    pub search_extra: String,
    pub shapes: Vec<crate::library::Shape>,
    pub auto_tags: Vec<&'static str>,
}

pub struct FolderNode {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub root: usize,
    pub has_children: bool,
    pub ancestors: Vec<usize>,
    pub faces: Vec<usize>,
    pub fams: Vec<usize>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum View {
    All,
    Recent,
    Favorites,
    Active,
    Inactive,
    Installed,
    Online(Provider),
    Folder(PathBuf),
    Collection(u64),
    Tag(String),
    Finder,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Styles,
    Glyphs,
    Waterfall,
    Tester,
    Variable,
    Playground,
    Info,
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Variable,
    Mono,
    Italic,
}

#[derive(Clone)]
struct DragFams(Vec<usize>);

enum Action {
    SetFamilies(Vec<usize>, bool),
    SetPaths(Vec<String>, bool),
    ToggleFavorite(Vec<usize>),
    AddToCollection(Vec<usize>, u64),
    NewCollection(Vec<usize>),
    RemoveFromCollection(Vec<usize>, u64),
    DeleteCollection(u64),
    ExportCollection(u64),
    SyncCollection(u64, bool),
    Specimen(Vec<usize>, String),
    ExportCss(Vec<usize>, String),
    AddTag(Vec<usize>, String),
    RemoveTag(Vec<usize>, String),
    NewTag(Vec<usize>),
    DeleteTag(String),
    RemoveFolder(usize),
    AddFolderDialog,
    ImportDialog,
    DocumentDialog,
    Rescan,
    SetView(View),
    Click(usize, bool, bool),
    ShowInFolder(PathBuf),
    Copy(String),
    RenameFamily(Vec<usize>),
    /// open the "Delete fonts" window for these families
    Delete(Vec<usize>),
    NetworkFolderDialog,
}

struct Shot {
    path: PathBuf,
    frames: u32,
    requested: bool,
    started: Instant,
}

#[derive(Clone, Copy)]
pub struct Pal {
    top: Color32,
    side: Color32,
    content: Color32,
    detail: Color32,
    card: Color32,
    text: Color32,
    weak: Color32,
    line: Color32,
    hover: Color32,
    sel: Color32,
    accent: Color32,
    accent_soft: Color32,
    toggle_off: Color32,
    input: Color32,
    skeleton: Color32,
    heart: Color32,
    /// text on a button filled with the accent colour
    on_accent: Color32,
}

fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn pal(t: Theme) -> Pal {
    let c = |a: [u32; 15]| Pal {
        top: hex(a[0]),
        side: hex(a[1]),
        content: hex(a[2]),
        detail: hex(a[3]),
        card: hex(a[4]),
        text: hex(a[5]),
        weak: hex(a[6]),
        line: hex(a[7]),
        hover: hex(a[8]),
        sel: hex(a[9]),
        accent: hex(a[10]),
        accent_soft: hex(a[11]),
        toggle_off: hex(a[12]),
        input: hex(a[13]),
        skeleton: hex(a[14]),
        heart: hex(0xff2d55),
        on_accent: Color32::WHITE,
    };
    match t {
        // the sand of the app icon on warm, dark greys
        Theme::Sand => Pal {
            on_accent: hex(0x1d1912),
            ..c([
                0x141310, 0x1a1916, 0x1e1c19, 0x211f1b, 0x26231f, 0xeee8dd, 0x9b9283, 0x2e2b25, 0x27241f, 0x3a342a,
                0xd6b278, 0x3b3224, 0x4d473e, 0x2a2722, 0x2a2722,
            ])
        },
        Theme::Dark => c([
            0x141414, 0x1a1a1a, 0x1e1e1e, 0x212121, 0x252525, 0xececec, 0x8f8f8f, 0x2c2c2c, 0x252525, 0x333333,
            0x2fd04b, 0x1f3a25, 0x4a4a4a, 0x2a2a2a, 0x292929,
        ]),
        Theme::Light => c([
            0xf1f1f1, 0xf6f6f6, 0xffffff, 0xfafafa, 0xffffff, 0x1b1b1b, 0x7b7b7b, 0xe4e4e4, 0xf2f2f2, 0xe4e4e4,
            0x18a83a, 0xe3f6e7, 0xc8c8c8, 0xebebeb, 0xefefef,
        ]),
        Theme::Midnight => c([
            0x0f1424, 0x121829, 0x161d31, 0x1a2238, 0x1d2640, 0xe6e9f5, 0x8a93b2, 0x263052, 0x1f2844, 0x2a3560,
            0x8b7cff, 0x2c2a5a, 0x3d4770, 0x222b48, 0x222b48,
        ]),
        Theme::Sepia => c([
            0xebe1cf, 0xf2e9da, 0xfbf6ec, 0xf6efe2, 0xfffaf0, 0x3b2f22, 0x8c7a63, 0xe2d5bf, 0xf0e6d4, 0xe6d9c2,
            0xc2410c, 0xf6dfcf, 0xcdbfa8, 0xece2cf, 0xece2cf,
        ]),
        Theme::Classic => c([
            0xdfe1e5, 0xe9ebee, 0xf7f8fa, 0xf1f2f4, 0xffffff, 0x22252b, 0x6b7180, 0xd3d6dc, 0xe1e4e9, 0xd2ddf2,
            0x2f6fdb, 0xdde7fa, 0xb9bec8, 0xffffff, 0xe6e8ec,
        ]),
    }
}

const COLLECTION_COLORS: [[u8; 3]; 8] = [
    [0x2f, 0xd0, 0x4b],
    [0xf2, 0x5f, 0x5c],
    [0x2e, 0xc4, 0xb6],
    [0xff, 0xb4, 0x00],
    [0x3a, 0x86, 0xff],
    [0xe0, 0x56, 0xfd],
    [0x8b, 0x7c, 0xff],
    [0xfb, 0x85, 0x00],
];

const PRESETS: &[&str] = &[
    "The quick brown fox jumps over the lazy dog.",
    "Pack my box with five dozen liquor jugs.",
    "Sphinx of black quartz, judge my vow.",
    "Zwölf Boxkämpfer jagen Viktor quer über den großen Sylter Deich.",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "abcdefghijklmnopqrstuvwxyz",
    "0123456789 !?&@#$%*()",
    "Handgloves",
];

/// Tags FontCraft works out by itself from the font's classification and measured shape.
fn auto_tags(class: &str, shape: Option<crate::library::Shape>, mono: bool, variable: bool, italic: bool) -> Vec<&'static str> {
    let mut t = vec![];
    match class {
        "serif" => t.push("serif"),
        "sans" => t.push("sans"),
        "script" => t.push("script"),
        "display" => t.push("display"),
        "symbol" => t.push("symbol"),
        _ => {}
    }
    if mono {
        t.push("mono");
    }
    if variable {
        t.push("variable");
    }
    if italic {
        t.push("italic");
    }
    if let Some(s) = shape {
        if s.weight > 0.6 {
            t.push("heavy");
        } else if s.weight < 0.18 {
            t.push("light");
        }
        if s.proportion < 0.25 {
            t.push("condensed");
        } else if s.proportion > 0.7 {
            t.push("wide");
        }
        if s.contrast > 0.6 {
            t.push("high-contrast");
        } else if s.contrast < 0.15 {
            t.push("monoline");
        }
        if s.xheight > 0.7 {
            t.push("big x-height");
        } else if s.xheight < 0.25 {
            t.push("small x-height");
        }
    }
    t
}

pub(crate) fn scroll_speed(level: u8) -> f32 {
    match level {
        0 => 20.0,
        2 => 80.0,
        _ => 40.0,
    }
}

fn in_range(v: f32, r: (f32, f32)) -> bool {
    v >= r.0 - 1e-4 && v <= r.1 + 1e-4
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ------------------------------------------------------------------ app

pub struct FontCraftApp {
    cfg: Config,
    dirty: bool,
    last_save: Instant,
    faces: Vec<Face>,
    act: Vec<bool>,
    families: Vec<Family>,
    face_fam: Vec<usize>,
    nodes: Vec<FolderNode>,
    node_index: HashMap<PathBuf, usize>,
    lib_names: HashSet<String>,
    face_by_id: HashMap<u64, usize>,
    finder: finder::Finder,
    scan_rx: Option<Receiver<ScanMsg>>,
    scan_progress: (usize, usize),
    scans_done: u32,
    loader: FontLoader,
    view: View,
    search: String,
    filter: Filter,
    selected: BTreeSet<usize>,
    primary: Option<usize>,
    anchor: Option<usize>,
    visible: Vec<usize>,
    tab: Tab,
    detail_face: usize,
    glyphs: Option<(u64, Vec<GlyphInfo>)>,
    glyph_sel: Option<usize>,
    glyph_query: String,
    glyph_block: Option<String>,
    pg: playground::Playground,
    family_rename: Option<(Vec<String>, String)>,
    var_values: HashMap<String, f32>,
    var_tex: Option<(u64, egui::TextureHandle)>,
    renaming: Option<(String, String)>,
    show_settings: bool,
    sidebar_open: bool,
    status: Option<(String, Instant)>,
    act_rx: Option<Receiver<String>>,
    shot: Option<Shot>,
    first_frame: bool,
    start_minimized: bool,
    themed: Option<Theme>,
    tester_size: f32,
    search_focus: bool,
    ctx: egui::Context,
    // Google Fonts
    google: Google,
    gcat: Vec<GFamily>,
    /// the catalogue of each source (indexed by Provider::idx)
    gstates: [GState; 3],
    gvisible: Vec<usize>,
    gsel: Option<usize>,
    gjobs: HashSet<String>,
    gcategory: Option<&'static str>,
    /// downloaded Google family folder name -> its faces
    gdl: HashMap<String, Vec<usize>>,
    // folder watching
    watch_list: watcher::WatchList,
    watch_rx: Receiver<()>,
    rescan_pending: bool,
    // newer version on GitHub
    update_rx: Option<Receiver<Option<crate::updates::Update>>>,
    update: Option<crate::updates::Update>,
    shape_range: [(f32, f32); 4],
    autoact: autoact::AutoAct,
    sync_check: Instant,
    sync_mtime: Option<SystemTime>,
    coll_hash: HashMap<u64, u64>,
    /// the native window, for showing and hiding it from the tray
    #[cfg_attr(not(windows), allow(dead_code))]
    hwnd: std::sync::Arc<std::sync::atomic::AtomicIsize>,
    #[cfg(windows)]
    tray: Option<crate::tray::Tray>,
    #[cfg_attr(not(windows), allow(dead_code))]
    quitting: bool,
    // deleting fonts
    delete: Option<removal::DeleteDialog>,
    del_rx: Option<Receiver<removal::DelResult>>,
    del_report: Option<String>,
    // network folders
    netdlg: Option<removal::NetDialog>,
    /// library folders that couldn't be reached in the last scan
    offline: HashSet<PathBuf>,
    /// library folders on another computer
    net_folders: Vec<PathBuf>,
    /// the app icon, for the About part of the settings
    logo: Option<egui::TextureHandle>,
}

#[derive(Clone, PartialEq)]
enum GState {
    Loading,
    Ready,
    Failed(String),
}

impl FontCraftApp {
    fn scan_folders(cfg: &Config) -> Vec<PathBuf> {
        let mut v = cfg.folders.clone();
        for p in Provider::ALL {
            v.push(p.fonts_dir());
        }
        v
    }

    pub fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, minimized: bool, picture: Option<PathBuf>) -> Self {
        let mut cfg = Config::load();
        if cfg.machine_id.is_empty() {
            cfg.machine_id = format!("{:016x}", activation::fnv(&format!("{:?}{}", SystemTime::now(), std::process::id())));
        }
        cc.egui_ctx.options_mut(|o| o.line_scroll_speed = scroll_speed(cfg.scroll_speed));
        let cache: ScanCache = config::load_cache();
        let rx = library::spawn_scan(Self::scan_folders(&cfg), cache, cc.egui_ctx.clone());
        let view = if cfg.folders.is_empty() { View::Installed } else { View::All };
        let google = Google::new(cc.egui_ctx.clone());
        google.load_catalogs(shot.is_none());
        let watch_list: watcher::WatchList = std::sync::Arc::new(std::sync::Mutex::new(cfg.folders.clone()));
        let watch_rx = watcher::spawn(watch_list.clone(), cc.egui_ctx.clone());
        let update_rx = if shot.is_none() && cfg.check_updates { crate::updates::check(cc.egui_ctx.clone()) } else { None };
        let mut app = Self {
            update_rx,
            update: None,
            shape_range: [(0.0, 1.0); 4],
            autoact: autoact::AutoAct::new(cfg.auto_folders.clone(), cc.egui_ctx.clone(), shot.is_none()),
            sync_check: Instant::now(),
            sync_mtime: None,
            coll_hash: HashMap::new(),
            hwnd: Default::default(),
            #[cfg(windows)]
            tray: None,
            quitting: false,
            delete: None,
            del_rx: None,
            del_report: None,
            netdlg: None,
            offline: HashSet::new(),
            net_folders: vec![],
            logo: None,
            google,
            gcat: vec![],
            gstates: [GState::Loading, GState::Loading, GState::Loading],
            gvisible: vec![],
            gsel: None,
            gjobs: HashSet::new(),
            gcategory: None,
            gdl: HashMap::new(),
            watch_list,
            watch_rx,
            rescan_pending: false,
            loader: {
                let defs = crate::uifont::definitions();
                cc.egui_ctx.set_fonts(defs.clone());
                FontLoader::new(cc.egui_ctx.clone(), defs)
            },
            ctx: cc.egui_ctx.clone(),
            dirty: false,
            last_save: Instant::now(),
            faces: vec![],
            act: vec![],
            families: vec![],
            face_fam: vec![],
            nodes: vec![],
            node_index: HashMap::new(),
            lib_names: HashSet::new(),
            face_by_id: HashMap::new(),
            finder: Default::default(),
            scan_rx: Some(rx),
            scan_progress: (0, 0),
            scans_done: 0,
            view,
            search: String::new(),
            filter: Filter::All,
            selected: BTreeSet::new(),
            primary: None,
            anchor: None,
            visible: vec![],
            tab: Tab::Styles,
            detail_face: 0,
            glyphs: None,
            glyph_sel: None,
            glyph_query: String::new(),
            glyph_block: None,
            pg: Default::default(),
            family_rename: None,
            var_values: HashMap::new(),
            var_tex: None,
            renaming: None,
            show_settings: false,
            sidebar_open: true,
            status: None,
            act_rx: None,
            shot: shot.map(|path| Shot { path, frames: 0, requested: false, started: Instant::now() }),
            first_frame: true,
            start_minimized: minimized,
            themed: None,
            tester_size: 28.0,
            search_focus: false,
            cfg,
        };
        match picture {
            Some(p) if p.as_os_str().is_empty() => app.view = View::Finder,
            Some(p) => app.finder_load_path(p),
            None => {}
        }
        // for pictures of a check: FONTCRAFT_SHOW=sand,settings (nothing is saved in that mode)
        if let (Some(_), Ok(s)) = (&app.shot, std::env::var("FONTCRAFT_SHOW")) {
            if s.contains("sand") {
                app.cfg.theme = Theme::Sand;
            }
            if s.contains("settings") {
                app.show_settings = true;
            }
        }
        if let Ok(s) = std::env::var("FONTCRAFT_SOURCE") {
            if let Some(p) = Provider::ALL.iter().find(|p| p.tag() == s.to_lowercase().replace(' ', "")) {
                app.view = View::Online(*p);
            }
        }
        app
    }

    fn p(&self) -> Pal {
        pal(self.cfg.theme)
    }

    fn set_status(&mut self, s: impl Into<String>) {
        self.status = Some((s.into(), Instant::now()));
    }

    // -------------------------------------------------------------- scanning

    fn rescan(&mut self) {
        if let Ok(mut l) = self.watch_list.lock() {
            *l = self.cfg.folders.clone();
        }
        if self.scan_rx.is_some() {
            self.rescan_pending = true;
            return;
        }
        self.rescan_pending = false;
        let cache = config::load_cache();
        self.scan_rx = Some(library::spawn_scan(Self::scan_folders(&self.cfg), cache, self.ctx.clone()));
    }

    /// Something changed in a font folder on disk: pick it up.
    fn poll_watch(&mut self) {
        let mut changed = false;
        while self.watch_rx.try_recv().is_ok() {
            changed = true;
        }
        if changed && self.shot.is_none() {
            self.rescan();
        }
        while let Ok((path, fonts, auto)) = self.autoact.rx.try_recv() {
            self.apply_doc_fonts(path, fonts, auto);
        }
    }

    // -------------------------------------------------------------- sync between computers

    /// Give collections that changed since the last save a new "modified" time.
    fn stamp_collections(&mut self) {
        let t = now();
        for c in &mut self.cfg.collections {
            let h = activation::fnv(&format!("{}{:?}{:?}", c.name, c.color, c.families));
            if let Some(old) = self.coll_hash.insert(c.id, h) {
                if old != h {
                    c.modified = t;
                }
            } else if c.modified == 0 {
                c.modified = t;
            }
        }
    }

    pub(super) fn sync_push(&mut self) {
        let Some(dir) = self.cfg.sync_dir.clone() else { return };
        if self.shot.is_some() {
            return;
        }
        if let Err(e) = sync::write(&dir, &self.cfg, now()) {
            self.set_status(format!("Could not write to the sync folder: {e}"));
            return;
        }
        self.sync_mtime = sync::mtime(&dir);
        // copy the font files of every collection, so the other computers have them too
        let groups: Vec<(String, Vec<PathBuf>)> =
            self.cfg.collections.iter().map(|c| (c.name.clone(), self.collection_files(c.id))).collect();
        std::thread::spawn(move || {
            sync::copy_fonts(&dir, &groups);
        });
    }

    /// Merge what other computers wrote into the sync folder.
    pub(super) fn sync_pull(&mut self, force: bool) {
        let Some(dir) = self.cfg.sync_dir.clone() else { return };
        let m = sync::mtime(&dir);
        if m.is_none() || (!force && m == self.sync_mtime) {
            return;
        }
        self.sync_mtime = m;
        let Some(other) = sync::read(&dir) else { return };
        if other.machine == self.cfg.machine_id && !force {
            return;
        }
        self.cfg.sync_seen = other.saved_at;
        if sync::merge(&mut self.cfg, &other) {
            for c in &self.cfg.collections {
                self.coll_hash.insert(c.id, activation::fnv(&format!("{}{:?}{:?}", c.name, c.color, c.families)));
            }
            self.dirty = true;
            self.set_status("Collections, tags and favorites updated from your other computer");
        }
    }

    pub(super) fn enable_sync(&mut self, dir: PathBuf) {
        let fonts = sync::fonts_dir(&dir);
        let _ = std::fs::create_dir_all(&fonts);
        self.cfg.sync_dir = Some(dir);
        if !self.cfg.folders.contains(&fonts) {
            self.cfg.folders.push(fonts);
        }
        self.sync_pull(true);
        self.dirty = true;
        self.sync_push();
        self.rescan();
        self.set_status("Sync is on – collections, tags and favorites are kept in the chosen folder");
    }

    pub(super) fn disable_sync(&mut self) {
        self.cfg.sync_dir = None;
        self.dirty = true;
        self.set_status("Sync is off (the synced font folder stays in your library)");
    }

    fn poll_scan(&mut self) {
        let Some(rx) = &self.scan_rx else { return };
        let mut done = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                ScanMsg::Progress { done, total } => self.scan_progress = (done, total),
                ScanMsg::Done { faces, cache, offline, network } => done = Some((faces, cache, offline, network)),
            }
        }
        if let Some((faces, cache, offline, network)) = done {
            self.scan_rx = None;
            // Network folders that are away keep their active fonts; when they're back, the fonts
            // are switched on again.
            let offline: HashSet<PathBuf> = offline.into_iter().collect();
            let back: Vec<PathBuf> = self.offline.difference(&offline).cloned().collect();
            let went: Vec<PathBuf> = offline.difference(&self.offline).cloned().collect();
            self.offline = offline;
            self.net_folders = network;
            if self.scans_done > 0 && self.shot.is_none() {
                if !back.is_empty() {
                    let paths: Vec<PathBuf> = self
                        .cfg
                        .active
                        .iter()
                        .filter(|p| back.iter().any(|b| std::path::Path::new(p.as_str()).starts_with(b)))
                        .map(PathBuf::from)
                        .filter(|p| p.exists())
                        .collect();
                    if !paths.is_empty() {
                        self.spawn_activation(paths, true, true);
                    }
                }
                if let Some(w) = went.first() {
                    self.set_status(format!("{} can't be reached right now – its active fonts come back when it's online again", w.display()));
                }
            }
            if self.shot.is_none() {
                config::save_cache(&cache);
            }
            let before: HashSet<String> = self.faces.iter().filter(|f| !f.system).map(|f| f.path_key.clone()).collect();
            self.rebuild(faces);
            if self.scans_done > 0 {
                let after: HashSet<&String> = self.faces.iter().filter(|f| !f.system).map(|f| &f.path_key).collect();
                let added = after.iter().filter(|p| !before.contains(**p)).count();
                let removed = before
                    .iter()
                    .filter(|p| !after.contains(p) && !self.offline.iter().any(|o| std::path::Path::new(p.as_str()).starts_with(o)))
                    .count();
                let mut parts = vec![];
                if added > 0 {
                    parts.push(format!("{added} new font file{} found", if added == 1 { "" } else { "s" }));
                }
                if removed > 0 {
                    parts.push(format!("{removed} font file{} removed", if removed == 1 { "" } else { "s" }));
                }
                if !parts.is_empty() {
                    self.set_status(parts.join(" · "));
                }
            }
            // Fonts that were deleted on disk can't stay active.
            let away = |p: &str| self.offline.iter().any(|o| std::path::Path::new(p).starts_with(o));
            let gone: Vec<String> =
                self.cfg.active.iter().filter(|p| !away(p) && !std::path::Path::new(p.as_str()).exists()).cloned().collect();
            if !gone.is_empty() {
                for g in &gone {
                    self.cfg.active.remove(g);
                }
                self.dirty = true;
                self.refresh_active();
                if self.shot.is_none() {
                    let paths: Vec<PathBuf> = gone.iter().map(PathBuf::from).collect();
                    std::thread::spawn(move || {
                        activation::deactivate(&paths);
                    });
                }
            }
            self.scans_done += 1;
            // for pictures of the windows: FONTCRAFT_SHOW=delete|network
            if self.scans_done == 1 {
                match std::env::var("FONTCRAFT_SHOW").as_deref() {
                    Ok("delete") => {
                        let mut pick: Vec<usize> = vec![];
                        let os = self.families.iter().position(|f| f.system && crate::remove::is_os_font(&self.faces[f.primary].meta.path, &f.name));
                        let other = self.families.iter().position(|f| f.system && !crate::remove::is_os_font(&self.faces[f.primary].meta.path, &f.name));
                        let mine = self.families.iter().position(|f| !f.system);
                        pick.extend([os, other, mine].into_iter().flatten());
                        self.open_delete(pick);
                    }
                    Ok("search") => self.search = "Arial".into(),
                    Ok("network") => {
                        self.netdlg = Some(removal::NetDialog { address: r"\\nas\Design\Fonts".into(), error: None, busy: None })
                    }
                    _ => {}
                }
            }
            if self.rescan_pending {
                self.rescan();
            }
            if self.scans_done == 1 && self.shot.is_none() {
                // Bring back the fonts that were active last time.
                let paths: Vec<PathBuf> = self
                    .cfg
                    .active
                    .iter()
                    .map(PathBuf::from)
                    .filter(|p| !self.offline.iter().any(|o| p.starts_with(o)) && p.exists())
                    .collect();
                if !paths.is_empty() {
                    self.spawn_activation(paths, true, true);
                }
            }
            if self.shot.is_none() {
                self.sync_all_collections();
            }
        }
    }

    fn rebuild(&mut self, list: Vec<(FaceMeta, Origin)>) {
        let sel_names: Vec<(String, bool)> =
            self.selected.iter().filter_map(|i| self.families.get(*i)).map(|f| (f.name.clone(), f.system)).collect();
        let prim_name = self.primary.and_then(|i| self.families.get(i)).map(|f| (f.name.clone(), f.system));
        let t = now();
        self.faces.clear();
        self.families.clear();
        let mut index: HashMap<(bool, String), usize> = HashMap::new();
        for (meta, origin) in list {
            let path_key = meta.path.to_string_lossy().to_string();
            let (system, folder) = match &origin {
                Origin::System => (true, None),
                Origin::Folder(f) => (false, self.cfg.folders.iter().position(|x| x == f)),
            };
            if !system && !self.cfg.added.contains_key(&path_key) {
                self.cfg.added.insert(path_key.clone(), t);
                self.dirty = true;
            }
            let id = activation::fnv(&format!("{}#{}", path_key, meta.index));
            let fi = self.faces.len();
            let okey = format!("{}#{}", path_key, meta.index);
            let fam_name = if let Some(o) = self.cfg.family_overrides.get(&okey) {
                o.clone()
            } else if self.cfg.group_families {
                meta.family.clone()
            } else {
                meta.full_name.clone()
            };
            self.faces.push(Face { meta, id, system, folder, path_key });
            let idx = *index.entry((system, fam_name.clone())).or_insert_with(|| {
                self.families.push(Family {
                    lower: fam_name.to_lowercase(),
                    name: fam_name.clone(),
                    faces: vec![],
                    files: vec![],
                    primary: 0,
                    system,
                    added: 0,
                    variable: false,
                    mono: false,
                    italic: false,
                    ext: String::new(),
                    search_extra: String::new(),
                    shapes: vec![],
                    auto_tags: vec![],
                });
                self.families.len() - 1
            });
            self.families[idx].faces.push(fi);
        }
        for fam in &mut self.families {
            let faces = &self.faces;
            fam.faces.sort_by_key(|i| {
                let m = &faces[*i].meta;
                (m.italic, m.width, m.weight, m.style.clone())
            });
            fam.files = fam.faces.clone();
            fam.faces.dedup_by(|a, b| {
                let (x, y) = (&faces[*a].meta, &faces[*b].meta);
                x.style == y.style && x.file_size == y.file_size && x.version == y.version
            });
            fam.primary = *fam
                .faces
                .iter()
                .min_by_key(|i| {
                    let m = &faces[**i].meta;
                    (m.weight as i32 - 400).abs() + if m.italic { 1000 } else { 0 } + (m.width as i32 - 5).abs() * 50
                })
                .unwrap_or(&fam.faces[0]);
            fam.added = fam.faces.iter().filter_map(|i| self.cfg.added.get(&faces[*i].path_key)).copied().max().unwrap_or(0);
            fam.variable = fam.faces.iter().any(|i| !faces[*i].meta.axes.is_empty());
            fam.mono = fam.faces.iter().any(|i| faces[*i].meta.monospace);
            fam.italic = fam.faces.iter().any(|i| faces[*i].meta.italic);
            let pm = &faces[fam.primary].meta;
            fam.ext = pm.path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
            let mut extra = String::new();
            for i in &fam.faces {
                let m = &faces[*i].meta;
                extra.push_str(&format!(" {} {} {} {} {}", m.style, m.designer, m.manufacturer, m.postscript, m.format));
            }
            extra.push_str(&format!(" {}", pm.copyright));
            fam.shapes = fam.faces.iter().filter_map(|i| faces[*i].meta.shape).collect();
            fam.auto_tags = auto_tags(&pm.class, pm.shape, fam.mono, fam.variable, fam.italic);
            for t in &fam.auto_tags {
                extra.push(' ');
                extra.push_str(t);
            }
            fam.search_extra = extra.to_lowercase();
        }
        // Which online families are in the library (downloaded into <source dir>/<Family>/).
        self.gdl.clear();
        let gdirs: Vec<(Provider, PathBuf)> = Provider::ALL.iter().map(|p| (*p, p.fonts_dir())).collect();
        for (fi, f) in self.faces.iter().enumerate() {
            for (p, gdir) in &gdirs {
                if let Ok(rest) = f.meta.path.strip_prefix(gdir) {
                    if let Some(first) = rest.components().next() {
                        self.gdl.entry(format!("{}/{}", p.tag(), first.as_os_str().to_string_lossy())).or_default().push(fi);
                    }
                }
            }
        }
        // Sort families by name and remap face indices.
        let mut order: Vec<usize> = (0..self.families.len()).collect();
        order.sort_by(|a, b| {
            let (x, y) = (&self.families[*a], &self.families[*b]);
            x.lower.cmp(&y.lower).then(x.system.cmp(&y.system))
        });
        let mut old: Vec<Option<Family>> = std::mem::take(&mut self.families).into_iter().map(Some).collect();
        self.families = order.iter().map(|i| old[*i].take().unwrap()).collect();
        self.face_fam = vec![0; self.faces.len()];
        for (fi, f) in self.families.iter().enumerate() {
            for x in &f.faces {
                self.face_fam[*x] = fi;
            }
        }
        self.lib_names = self.families.iter().filter(|f| !f.system).map(|f| f.name.clone()).collect();
        self.face_by_id = self.faces.iter().enumerate().map(|(i, f)| (f.id, i)).collect();
        self.refresh_active();
        self.build_tree();
        let find = |n: &(String, bool), fams: &Vec<Family>| fams.iter().position(|f| f.name == n.0 && f.system == n.1);
        self.selected = sel_names.iter().filter_map(|n| find(n, &self.families)).collect();
        self.primary = prim_name.and_then(|n| find(&n, &self.families));
        if let View::Folder(p) = &self.view {
            if !self.node_index.contains_key(p) {
                self.view = View::All;
            }
        }
    }

    fn build_tree(&mut self) {
        self.nodes.clear();
        self.node_index.clear();
        for (ri, root) in self.cfg.folders.clone().iter().enumerate() {
            let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
            dirs.insert(root.clone());
            for f in self.faces.iter().filter(|f| f.folder == Some(ri)) {
                let mut d = f.meta.path.parent();
                while let Some(x) = d {
                    if !x.starts_with(root) || !dirs.insert(x.to_path_buf()) || x == root {
                        break;
                    }
                    d = x.parent();
                }
            }
            fn dfs(
                p: &PathBuf,
                depth: usize,
                anc: Vec<usize>,
                ri: usize,
                dirs: &BTreeSet<PathBuf>,
                out: &mut Vec<FolderNode>,
                name: String,
            ) {
                let mut kids: Vec<&PathBuf> = dirs.iter().filter(|d| d.parent() == Some(p.as_path())).collect();
                kids.sort_by_key(|d| d.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default());
                let me = out.len();
                out.push(FolderNode {
                    path: p.clone(),
                    name,
                    depth,
                    root: ri,
                    has_children: !kids.is_empty(),
                    ancestors: anc.clone(),
                    faces: vec![],
                    fams: vec![],
                });
                let mut a2 = anc;
                a2.push(me);
                for k in kids {
                    let n = k.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    dfs(k, depth + 1, a2.clone(), ri, dirs, out, n);
                }
            }
            let synced = self.cfg.sync_dir.as_ref().map(|d| sync::fonts_dir(d) == *root).unwrap_or(false);
            let name = if *root == self.cfg.import_dir() {
                "Imported fonts".to_string()
            } else if synced {
                "Synced fonts".to_string()
            } else {
                root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.display().to_string())
            };
            dfs(root, 0, vec![], ri, &dirs, &mut self.nodes, name);
        }
        for (i, n) in self.nodes.iter().enumerate() {
            self.node_index.insert(n.path.clone(), i);
        }
        for (fi, f) in self.faces.iter().enumerate() {
            if f.folder.is_none() {
                continue;
            }
            let mut d = f.meta.path.parent();
            while let Some(x) = d {
                match self.node_index.get(x) {
                    Some(ni) => {
                        let ni = *ni;
                        self.nodes[ni].faces.push(fi);
                        if self.nodes[ni].depth == 0 {
                            break;
                        }
                    }
                    None => break,
                }
                d = x.parent();
            }
        }
        for n in &mut self.nodes {
            let set: BTreeSet<usize> = n.faces.iter().map(|f| self.face_fam[*f]).collect();
            n.fams = set.into_iter().collect();
        }
    }

    fn refresh_active(&mut self) {
        self.act = self.faces.iter().map(|f| f.system || self.cfg.active.contains(&f.path_key)).collect();
    }

    // -------------------------------------------------------------- state helpers

    fn fam_state(&self, fi: usize) -> (usize, usize) {
        let f = &self.families[fi];
        let a = f.faces.iter().filter(|i| self.act[**i]).count();
        (a, f.faces.len())
    }

    fn faces_state(&self, faces: &[usize]) -> (usize, usize) {
        (faces.iter().filter(|i| self.act[**i]).count(), faces.len())
    }

    fn fams_state(&self, fams: &[usize]) -> (usize, usize) {
        fams.iter().fold((0, 0), |(a, n), f| {
            let (x, y) = self.fam_state(*f);
            (a + x, n + y)
        })
    }

    fn in_named_set(&self, f: &Family, names: &BTreeSet<String>) -> bool {
        names.contains(&f.name) && (!f.system || !self.lib_names.contains(&f.name))
    }

    fn families_in_view(&self, view: &View) -> Vec<usize> {
        if let View::Folder(p) = view {
            return self.node_index.get(p).map(|i| self.nodes[*i].fams.clone()).unwrap_or_default();
        }
        let empty = BTreeSet::new();
        let coll = match view {
            View::Collection(id) => self.cfg.collections.iter().find(|c| c.id == *id).map(|c| &c.families).unwrap_or(&empty),
            View::Tag(t) => self.cfg.tags.get(t).unwrap_or(&empty),
            _ => &empty,
        };
        let mut v: Vec<usize> = (0..self.families.len())
            .filter(|i| {
                let f = &self.families[*i];
                match view {
                    View::All | View::Recent => !f.system,
                    View::Active => !f.system && self.fam_state(*i).0 > 0,
                    View::Inactive => !f.system && self.fam_state(*i).0 == 0,
                    View::Favorites => self.in_named_set(f, &self.cfg.favorites),
                    View::Installed => f.system,
                    View::Collection(_) | View::Tag(_) => self.in_named_set(f, coll),
                    View::Folder(_) | View::Online(_) | View::Finder => false,
                }
            })
            .collect();
        if *view == View::Recent {
            v.sort_by_key(|i| std::cmp::Reverse(self.families[*i].added));
            v.truncate(100);
        }
        v
    }

    fn compute_visible(&mut self) {
        let q = self.search.trim().to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        let shape_on = self.shape_active();
        let ranges = self.shape_range;
        let mut v: Vec<usize> = self
            .families_in_view(&self.view)
            .into_iter()
            .filter(|i| {
                let f = &self.families[*i];
                let tags_hit = |w: &str| self.cfg.tags.iter().any(|(t, s)| t.to_lowercase().contains(w) && s.contains(&f.name));
                (words.is_empty()
                    || f.lower.contains(&q)
                    || words.iter().all(|w| f.lower.contains(w) || f.search_extra.contains(w) || tags_hit(w)))
                    && match self.filter {
                        Filter::All => true,
                        Filter::Variable => f.variable,
                        Filter::Mono => f.mono,
                        Filter::Italic => f.italic,
                    }
                    && (!shape_on || f.shapes.iter().any(|s| (0..4).all(|k| in_range(s.get(k), ranges[k]))))
            })
            .collect();
        if self.view != View::Recent {
            match self.cfg.sort {
                SortBy::NameAz => {}
                SortBy::NameZa => v.reverse(),
                SortBy::Recent => v.sort_by_key(|i| std::cmp::Reverse(self.families[*i].added)),
                SortBy::Styles => v.sort_by_key(|i| std::cmp::Reverse(self.families[*i].faces.len())),
                SortBy::Popular => {}
            }
        }
        self.visible = v;
        if matches!(self.view, View::Online(_)) {
            self.compute_gvisible(&words);
        }
    }

    pub(super) fn shape_active(&self) -> bool {
        self.shape_range.iter().any(|r| r.0 > 0.001 || r.1 < 0.999)
    }

    fn compute_gvisible(&mut self, words: &[&str]) {
        let View::Online(prov) = self.view else { return };
        let mut v: Vec<usize> = (0..self.gcat.len())
            .filter(|i| {
                let g = &self.gcat[*i];
                let hit = |w: &str| {
                    g.lower.contains(w)
                        || g.category.to_lowercase().contains(w)
                        || g.kind.to_lowercase().contains(w)
                        || g.designers.iter().any(|d| d.to_lowercase().contains(w))
                        || (w == "variable" && !g.axes.is_empty())
                };
                g.provider == prov
                    && words.iter().all(|w| hit(w))
                    && self.gcategory.map(|c| g.category == c).unwrap_or(true)
                    && match self.filter {
                        Filter::All => true,
                        Filter::Variable => !g.axes.is_empty(),
                        Filter::Mono => g.category == "Monospace",
                        Filter::Italic => g.italic,
                    }
                    // Google publishes stroke thickness and width (not contrast or x-height)
                    && g.thickness.map(|t| in_range(t, self.shape_range[3])).unwrap_or(true)
                    && g.width.map(|w| in_range(w, self.shape_range[1])).unwrap_or(true)
            })
            .collect();
        match self.cfg.sort {
            SortBy::NameAz => {}
            SortBy::NameZa => v.reverse(),
            SortBy::Recent => v.sort_by(|a, b| self.gcat[*b].added.cmp(&self.gcat[*a].added)),
            SortBy::Styles => v.sort_by_key(|i| std::cmp::Reverse(self.gcat[*i].nstyles)),
            SortBy::Popular => v.sort_by_key(|i| self.gcat[*i].popularity),
        }
        self.gvisible = v;
    }

    /// Faces of a Google family that is in the library (empty when not downloaded).
    fn gfaces(&self, name: &str) -> &[usize] {
        self.pfaces(Provider::Google, name)
    }

    /// Faces of an online family that is in the library (empty when not downloaded).
    fn pfaces(&self, p: Provider, name: &str) -> &[usize] {
        self.gdl.get(&google::dl_key(p, name)).map(|v| v.as_slice()).unwrap_or(&[])
    }

    fn gfam_faces(&self, gi: usize) -> &[usize] {
        let g = &self.gcat[gi];
        self.pfaces(g.provider, &g.name)
    }

    fn gstate(&self, p: Provider) -> &GState {
        &self.gstates[p.idx()]
    }

    fn poll_google(&mut self) {
        while let Ok(m) = self.google.rx.try_recv() {
            match m {
                GMsg::Catalog(p, Ok(list)) => {
                    // the details card points into the list: close it
                    self.gsel = None;
                    self.gcat.retain(|g| g.provider != p);
                    self.gcat.extend(list);
                    self.gstates[p.idx()] = GState::Ready;
                }
                GMsg::Catalog(p, Err(e)) => self.gstates[p.idx()] = GState::Failed(e),
                GMsg::OpenInBrowser { name, url } => {
                    self.ctx.open_url(egui::OpenUrl::new_tab(url));
                    self.set_status(format!(
                        "Font Squirrel asks for a quick check in your browser – {name} is added and activated as soon as the download is in your Downloads folder"
                    ));
                }
                GMsg::Preview(key, Some(path)) => {
                    self.google.previews.insert(key, Some(path));
                }
                GMsg::Preview(key, None) => {
                    self.google.previews.remove(&key);
                    // Font Squirrel asked us to wait: try again later instead of giving up
                    if !(key.starts_with("fontsquirrel:") && crate::online::squirrel_blocked()) {
                        self.google.failed.insert(key);
                    }
                }
                GMsg::Family { key, name, activate, result } => {
                    self.gjobs.remove(&key);
                    match result {
                        Ok(paths) => {
                            if activate {
                                let keys: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
                                self.set_paths(keys, true);
                                self.set_status(format!("{name} downloaded and activated in all your apps"));
                            } else {
                                self.set_status(format!("{name} added to your library"));
                            }
                            self.rescan();
                        }
                        Err(e) => self.set_status(format!("Could not download {name}: {e}")),
                    }
                }
            }
        }
    }

    /// Download a family of the online catalogue (by its index in it).
    fn google_download(&mut self, gi: usize, activate: bool) {
        let Some(g) = self.gcat.get(gi).cloned() else { return };
        let key = g.key();
        if self.shot.is_some() || self.gjobs.contains(&key) {
            return;
        }
        self.gjobs.insert(key);
        self.set_status(format!("Downloading {} from {}…", g.name, g.provider.label()));
        self.google.download(g, activate);
    }

    /// "Delete…" on a family of an online source: asks first (only deactivate, or delete the download).
    fn google_remove(&mut self, gi: usize) {
        self.open_delete_online(gi);
    }

    fn view_title(&self, view: &View) -> String {
        match view {
            View::All => "All".into(),
            View::Active => "Active".into(),
            View::Inactive => "Inactive".into(),
            View::Favorites => "Favorites".into(),
            View::Recent => "Recent".into(),
            View::Installed => "Installed".into(),
            View::Folder(p) => self.node_index.get(p).map(|i| self.nodes[*i].name.clone()).unwrap_or_default(),
            View::Collection(id) => self.cfg.collections.iter().find(|c| c.id == *id).map(|c| c.name.clone()).unwrap_or_default(),
            View::Tag(t) => format!("#{t}"),
            View::Online(p) => p.label().into(),
            View::Finder => "Font finder".into(),
        }
    }

    // -------------------------------------------------------------- activation

    fn spawn_activation(&mut self, paths: Vec<PathBuf>, on: bool, quiet: bool) {
        if self.shot.is_some() || paths.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let n = paths.len();
            let errs = if on { activation::activate(&paths) } else { activation::deactivate(&paths) };
            let msg = if errs.is_empty() {
                if quiet {
                    format!("{n} active font files are ready in all your apps")
                } else {
                    format!("{} {} font file{}", if on { "Activated" } else { "Deactivated" }, n, if n == 1 { "" } else { "s" })
                }
            } else {
                format!("{} of {} files could not be {}: {}", errs.len(), n, if on { "activated" } else { "deactivated" }, errs[0].1)
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
        self.act_rx = Some(rx);
    }

    fn set_paths(&mut self, paths: Vec<String>, on: bool) {
        let mut changed = vec![];
        for p in paths {
            let had = self.cfg.active.contains(&p);
            if on && !had {
                self.cfg.active.insert(p.clone());
                changed.push(PathBuf::from(p));
            } else if !on && had {
                self.cfg.active.remove(&p);
                changed.push(PathBuf::from(p));
            }
        }
        if !changed.is_empty() {
            self.dirty = true;
            self.refresh_active();
            self.spawn_activation(changed, on, false);
        }
    }

    fn family_paths(&self, fams: &[usize]) -> Vec<String> {
        fams.iter()
            .filter(|f| !self.families[**f].system)
            .flat_map(|f| self.families[*f].faces.iter().map(|i| self.faces[*i].path_key.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn set_families(&mut self, fams: &[usize], on: bool) {
        let paths = self.family_paths(fams);
        self.set_paths(paths, on);
    }

    fn targets(&self, fi: usize) -> Vec<usize> {
        if self.selected.contains(&fi) {
            self.selected.iter().copied().collect()
        } else {
            vec![fi]
        }
    }

    fn family_faces_meta(&self, fams: &[usize]) -> Vec<(String, Vec<&FaceMeta>)> {
        fams.iter()
            .map(|f| {
                let fam = &self.families[*f];
                (fam.name.clone(), fam.faces.iter().map(|i| &self.faces[*i].meta).collect())
            })
            .collect()
    }

    fn collection_files(&self, id: u64) -> Vec<PathBuf> {
        let fams = self.families_in_view(&View::Collection(id));
        let set: BTreeSet<PathBuf> =
            fams.iter().flat_map(|f| self.families[*f].faces.iter().map(|i| self.faces[*i].meta.path.clone())).collect();
        set.into_iter().collect()
    }

    fn sync_all_collections(&mut self) {
        let jobs: Vec<(u64, PathBuf)> =
            self.cfg.collections.iter().filter_map(|c| c.sync_dir.clone().map(|d| (c.id, d))).collect();
        for (id, dir) in jobs {
            let files = self.collection_files(id);
            let n = extras::copy_files(&files, &dir);
            if n > 0 {
                self.set_status(format!("Synced {n} new font file{} to {}", if n == 1 { "" } else { "s" }, dir.display()));
            }
        }
    }

    // -------------------------------------------------------------- actions

    fn apply(&mut self, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::SetFamilies(f, on) => self.set_families(&f, on),
                Action::SetPaths(p, on) => self.set_paths(p, on),
                Action::ToggleFavorite(f) => {
                    let all_fav = f.iter().all(|i| self.cfg.favorites.contains(&self.families[*i].name));
                    for i in f {
                        let n = self.families[i].name.clone();
                        if all_fav {
                            self.cfg.favorites.remove(&n);
                        } else {
                            self.cfg.favorites.insert(n);
                        }
                    }
                    self.dirty = true;
                }
                Action::AddToCollection(f, id) => {
                    let names: Vec<String> = f.iter().map(|i| self.families[*i].name.clone()).collect();
                    if let Some(c) = self.cfg.collections.iter_mut().find(|c| c.id == id) {
                        let n = names.len();
                        c.families.extend(names);
                        let cname = c.name.clone();
                        let synced = c.sync_dir.is_some();
                        self.set_status(format!("Added {n} famil{} to “{cname}”", if n == 1 { "y" } else { "ies" }));
                        if synced && self.shot.is_none() {
                            self.sync_all_collections();
                        }
                    }
                    self.dirty = true;
                }
                Action::NewCollection(f) => {
                    let id = activation::fnv(&format!("{:?}{}", Instant::now(), self.cfg.collections.len())) ^ now();
                    let n = self.cfg.collections.len();
                    let name = format!("Collection {}", n + 1);
                    self.cfg.collections.push(Collection {
                        id,
                        name: name.clone(),
                        color: COLLECTION_COLORS[n % COLLECTION_COLORS.len()],
                        families: f.iter().map(|i| self.families[*i].name.clone()).collect(),
                        sync_dir: None,
                        modified: now(),
                    });
                    self.cfg.collapsed.remove("sec:collections");
                    self.renaming = Some((format!("c:{id}"), name));
                    self.dirty = true;
                }
                Action::RemoveFromCollection(f, id) => {
                    let names: Vec<String> = f.iter().map(|i| self.families[*i].name.clone()).collect();
                    if let Some(c) = self.cfg.collections.iter_mut().find(|c| c.id == id) {
                        for n in names {
                            c.families.remove(&n);
                        }
                    }
                    self.dirty = true;
                }
                Action::DeleteCollection(id) => {
                    self.cfg.collections.retain(|c| c.id != id);
                    self.cfg.deleted_collections.insert(id);
                    if self.view == View::Collection(id) {
                        self.view = View::All;
                    }
                    self.dirty = true;
                }
                Action::ExportCollection(id) => {
                    if let Some(dir) = rfd::FileDialog::new().set_title("Export collection to folder").pick_folder() {
                        let files = self.collection_files(id);
                        let n = extras::copy_files(&files, &dir);
                        self.set_status(format!("Exported {n} font file{} to {}", if n == 1 { "" } else { "s" }, dir.display()));
                    }
                }
                Action::SyncCollection(id, on) => {
                    let dir = if on { rfd::FileDialog::new().set_title("Keep this collection synced to a folder").pick_folder() } else { None };
                    if on && dir.is_none() {
                        continue;
                    }
                    if let Some(c) = self.cfg.collections.iter_mut().find(|c| c.id == id) {
                        c.sync_dir = dir;
                    }
                    self.dirty = true;
                    if on {
                        self.sync_all_collections();
                    } else {
                        self.set_status("Folder sync stopped");
                    }
                }
                Action::Specimen(f, title) => {
                    let fams = self.family_faces_meta(&f);
                    match extras::write_specimen(&title, &fams, &self.cfg.preview_text) {
                        Ok(p) => {
                            extras::open_file(&p);
                            self.set_status("Specimen opened in your browser — use Print to print or save as PDF");
                        }
                        Err(e) => self.set_status(format!("Could not create specimen: {e}")),
                    }
                }
                Action::ExportCss(f, title) => {
                    let fams = self.family_faces_meta(&f);
                    let metas: Vec<&FaceMeta> = fams.iter().flat_map(|(_, v)| v.iter().copied()).collect();
                    let css = extras::css_for(&metas, true);
                    if f.len() == 1 {
                        self.ctx.copy_text(css);
                        self.set_status("CSS @font-face rules copied to the clipboard");
                    } else if let Some(path) = rfd::FileDialog::new().set_title("Save CSS").set_file_name(format!("{title}.css")).save_file() {
                        let local = extras::css_for(&metas, false);
                        let files: Vec<PathBuf> = metas.iter().map(|m| m.path.clone()).collect();
                        if let Some(dir) = path.parent() {
                            extras::copy_files(&files, dir);
                        }
                        let _ = std::fs::write(&path, local);
                        self.set_status(format!("Saved CSS and font files to {}", path.display()));
                    }
                }
                Action::AddTag(f, tag) => {
                    let names: Vec<String> = f.iter().map(|i| self.families[*i].name.clone()).collect();
                    self.cfg.tags.entry(tag).or_default().extend(names);
                    self.dirty = true;
                }
                Action::RemoveTag(f, tag) => {
                    if let Some(s) = self.cfg.tags.get_mut(&tag) {
                        for i in f {
                            s.remove(&self.families[i].name);
                        }
                    }
                    self.dirty = true;
                }
                Action::NewTag(f) => {
                    let mut k = self.cfg.tags.len() + 1;
                    let mut name = format!("tag{k}");
                    while self.cfg.tags.contains_key(&name) {
                        k += 1;
                        name = format!("tag{k}");
                    }
                    let names: BTreeSet<String> = f.iter().map(|i| self.families[*i].name.clone()).collect();
                    self.cfg.tags.insert(name.clone(), names);
                    self.cfg.collapsed.remove("sec:tags");
                    self.renaming = Some((format!("t:{name}"), name));
                    self.dirty = true;
                }
                Action::DeleteTag(t) => {
                    self.cfg.tags.remove(&t);
                    if self.view == View::Tag(t) {
                        self.view = View::All;
                    }
                    self.dirty = true;
                }
                Action::RemoveFolder(i) => {
                    let paths: Vec<String> =
                        self.faces.iter().filter(|f| f.folder == Some(i)).map(|f| f.path_key.clone()).collect();
                    self.set_paths(paths, false);
                    let name = self.cfg.folders[i].display().to_string();
                    self.cfg.folders.remove(i);
                    self.view = View::All;
                    self.dirty = true;
                    self.set_status(format!("Removed {name} from FontCraft (the files were not deleted)"));
                    self.rescan();
                }
                Action::AddFolderDialog => {
                    if let Some(dir) = rfd::FileDialog::new().set_title("Add a font folder").pick_folder() {
                        self.add_folder(dir);
                    }
                }
                Action::ImportDialog => {
                    if let Some(files) =
                        rfd::FileDialog::new().set_title("Import fonts").add_filter("Fonts", library::FONT_EXTS).pick_files()
                    {
                        self.import_files(files);
                    }
                }
                Action::DocumentDialog => {
                    if let Some(doc) = rfd::FileDialog::new().set_title("Activate the fonts used in a document").pick_file() {
                        self.activate_for_document(doc);
                    }
                }
                Action::Rescan => self.rescan(),
                Action::SetView(v) => self.view = v,
                Action::Click(fi, ctrl, shift) => self.click_family(fi, ctrl, shift),
                Action::ShowInFolder(p) => activation::show_in_folder(&p),
                Action::RenameFamily(fams) => {
                    let keys: Vec<String> = fams
                        .iter()
                        .flat_map(|f| self.families[*f].faces.iter())
                        .map(|x| format!("{}#{}", self.faces[*x].path_key, self.faces[*x].meta.index))
                        .collect();
                    let name = fams.first().map(|f| self.families[*f].name.clone()).unwrap_or_default();
                    self.family_rename = Some((keys, name));
                }
                Action::Copy(s) => {
                    self.ctx.copy_text(s.clone());
                    self.set_status(format!("Copied “{s}”"));
                }
                Action::Delete(fams) => self.open_delete(fams),
                Action::NetworkFolderDialog => {
                    if self.netdlg.is_none() {
                        self.netdlg = Some(removal::NetDialog { address: String::new(), error: None, busy: None });
                    }
                }
            }
        }
    }

    fn activate_for_document(&mut self, doc: PathBuf) {
        self.autoact.check(doc, false);
    }

    /// A document was read: switch on the fonts it uses.
    fn apply_doc_fonts(&mut self, doc: PathBuf, fonts: autoact::DocFonts, auto: bool) {
        let mut paths = BTreeSet::new();
        let mut fams = BTreeSet::new();
        let mut already = 0;
        for (i, f) in self.faces.iter().enumerate() {
            if !f.system && autoact::face_used(&fonts, &f.meta.family, &f.meta.postscript, &f.meta.full_name) {
                if self.act[i] {
                    already += 1;
                } else {
                    paths.insert(f.path_key.clone());
                }
                fams.insert(self.families[self.face_fam[i]].name.clone());
            }
        }
        // Fonts the document needs that aren't on this computer: fetch them from the free
        // sources (Google Fonts first, then Fontshare, then Font Squirrel).
        let mut fetched = vec![];
        if self.cfg.auto_google && !self.gcat.is_empty() {
            let ns = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
            let have: HashSet<String> = self.families.iter().map(|f| ns(&f.name)).collect();
            let mut picks = vec![];
            for want in autoact::wanted_families(&fonts) {
                if have.contains(&want) || fetched.len() >= 10 {
                    continue;
                }
                let best = (0..self.gcat.len()).filter(|i| ns(&self.gcat[*i].name) == want).min_by_key(|i| self.gcat[*i].provider.idx());
                if let Some(gi) = best {
                    let downloaded = Provider::ALL.iter().any(|p| !self.pfaces(*p, &self.gcat[gi].name).is_empty());
                    if !downloaded {
                        fetched.push(self.gcat[gi].name.clone());
                        picks.push(gi);
                    }
                }
            }
            for gi in picks {
                self.google_download(gi, true);
            }
        }
        let name = doc.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let n = paths.len();
        let mut msg = if n > 0 {
            let list: Vec<String> = fams.into_iter().take(4).collect();
            self.set_paths(paths.into_iter().collect(), true);
            format!("Activated {n} font file{} for “{name}”: {}", if n == 1 { "" } else { "s" }, list.join(", "))
        } else if already > 0 {
            format!("The fonts of “{name}” are already active")
        } else if fetched.is_empty() {
            if auto {
                return; // nothing to report for a document saved in a watched folder
            }
            format!("No fonts from your library were found in “{name}”")
        } else {
            String::new()
        };
        if !fetched.is_empty() {
            if !msg.is_empty() {
                msg.push_str(" · ");
            }
            msg.push_str(&format!("downloading {} from the free font sources", fetched.join(", ")));
        }
        self.set_status(msg);
    }

    fn add_folder(&mut self, dir: PathBuf) {
        if !self.cfg.folders.contains(&dir) {
            self.cfg.folders.push(dir.clone());
            self.dirty = true;
            self.view = View::Folder(dir.clone());
            self.set_status(format!("Added folder {}", dir.display()));
            self.rescan();
        }
    }

    fn import_files(&mut self, files: Vec<PathBuf>) {
        let dest = self.cfg.import_dir();
        let n = extras::copy_files(&files, &dest);
        if !self.cfg.folders.contains(&dest) {
            self.cfg.folders.push(dest.clone());
        }
        self.dirty = true;
        self.set_status(format!("Imported {n} font file{}", if n == 1 { "" } else { "s" }));
        self.view = View::Folder(dest);
        self.rescan();
    }

    fn click_family(&mut self, fi: usize, ctrl: bool, shift: bool) {
        if shift {
            if let Some(a) = self.anchor {
                let pa = self.visible.iter().position(|x| *x == a);
                let pb = self.visible.iter().position(|x| *x == fi);
                if let (Some(pa), Some(pb)) = (pa, pb) {
                    let (lo, hi) = (pa.min(pb), pa.max(pb));
                    self.selected = self.visible[lo..=hi].iter().copied().collect();
                    self.primary = Some(fi);
                    return;
                }
            }
        }
        if ctrl {
            if !self.selected.insert(fi) {
                self.selected.remove(&fi);
            }
        } else {
            self.selected.clear();
            self.selected.insert(fi);
        }
        self.select_primary(fi);
    }

    fn select_primary(&mut self, fi: usize) {
        if self.primary != Some(fi) {
            self.detail_face = 0;
            self.glyph_sel = None;
            self.var_values.clear();
        }
        self.primary = Some(fi);
        self.anchor = Some(fi);
    }

    // -------------------------------------------------------------- theme

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.themed == Some(self.cfg.theme) {
            return;
        }
        self.themed = Some(self.cfg.theme);
        let p = self.p();
        let dark = matches!(self.cfg.theme, Theme::Dark | Theme::Midnight | Theme::Sand);
        let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = p.content;
        v.window_fill = p.detail;
        v.extreme_bg_color = p.input;
        v.faint_bg_color = p.hover;
        v.code_bg_color = p.input;
        v.selection.bg_fill = p.accent.gamma_multiply(0.6);
        v.selection.stroke = Stroke::new(1.0_f32, p.text);
        v.hyperlink_color = p.accent;
        v.window_stroke = Stroke::new(1.0_f32, p.line);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.line);
        v.widgets.noninteractive.fg_stroke.color = p.text;
        v.widgets.noninteractive.bg_fill = p.detail;
        v.widgets.inactive.bg_fill = p.input;
        v.widgets.inactive.weak_bg_fill = p.input;
        v.widgets.inactive.fg_stroke.color = p.text;
        v.widgets.inactive.bg_stroke = Stroke::NONE;
        v.widgets.hovered.weak_bg_fill = p.sel;
        v.widgets.hovered.bg_fill = p.sel;
        v.widgets.hovered.bg_stroke = Stroke::NONE;
        v.widgets.hovered.fg_stroke.color = p.text;
        v.widgets.active.weak_bg_fill = p.sel;
        v.widgets.active.bg_fill = p.accent;
        v.widgets.active.fg_stroke.color = p.text;
        v.widgets.open.weak_bg_fill = p.sel;
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(6);
            w.expansion = 0.0;
        }
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.slider_trailing_fill = true;
        ctx.set_visuals(v);
        ctx.style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
            s.spacing.button_padding = egui::vec2(9.0, 4.0);
            s.spacing.interact_size.y = 24.0;
            s.spacing.slider_width = 130.0;
            s.spacing.menu_margin = egui::Margin::same(8);
        });
    }

    fn hide_window(&self) {
        #[cfg(windows)]
        crate::tray::hide(self.hwnd.load(std::sync::atomic::Ordering::Relaxed));
    }

    /// Keeps the tray icon in step with the setting, handles its menu and turns
    /// "close" into "hide" while it is on. Returns true when the tray is active.
    #[allow(unused_variables)]
    fn tray_step(&mut self, ctx: &egui::Context, frame: &eframe::Frame) -> bool {
        #[cfg(windows)]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            use std::sync::atomic::Ordering;
            if self.hwnd.load(Ordering::Relaxed) == 0 {
                if let Ok(h) = frame.window_handle() {
                    if let RawWindowHandle::Win32(w) = h.as_raw() {
                        self.hwnd.store(w.hwnd.get(), Ordering::Relaxed);
                    }
                }
            }
            let want = self.cfg.hide_to_tray && self.shot.is_none();
            if want && self.tray.is_none() {
                match crate::tray::create(self.hwnd.clone(), ctx.clone()) {
                    Ok(t) => self.tray = Some(t),
                    Err(e) => {
                        self.cfg.hide_to_tray = false;
                        self.set_status(format!("The tray icon could not be created: {e}"));
                    }
                }
            } else if !want && self.tray.is_some() {
                self.tray = None;
            }
            let Some(t) = &self.tray else { return false };
            let (off, quit) = (t.all_off.swap(false, Ordering::Relaxed), t.quit.swap(false, Ordering::Relaxed));
            if off {
                let all: Vec<String> = self.cfg.active.iter().cloned().collect();
                self.set_paths(all, false);
            }
            if quit {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if self.dirty {
                    self.cfg.save();
                    self.dirty = false;
                }
                self.hide_window();
            }
            return true;
        }
        #[allow(unreachable_code)]
        false
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(shot) = &mut self.shot else { return };
        ctx.request_repaint();
        let ready = self.scan_rx.is_none() && self.loader.pending() == 0 && self.finder.job.is_none() && self.finder.waiting.is_none();
        if ready || shot.started.elapsed().as_secs() > 45 {
            shot.frames += 1;
        }
        if shot.frames >= 120 && !shot.requested {
            shot.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let mut done = false;
        ctx.input(|i| {
            for e in &i.raw.events {
                if let egui::Event::Screenshot { image, .. } = e {
                    let [w, h] = image.size;
                    if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, image.as_raw().to_vec()) {
                        if let Some(dir) = shot.path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        match img.save(&shot.path) {
                            Ok(()) => println!("Saved screenshot to {}", shot.path.display()),
                            Err(e) => eprintln!("Could not save screenshot: {e}"),
                        }
                    }
                    done = true;
                }
            }
        });
        if done || shot.started.elapsed().as_secs() > 150 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for FontCraftApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let in_tray = self.tray_step(ctx, frame);
        if self.first_frame && self.start_minimized && self.shot.is_none() {
            if in_tray {
                self.hide_window();
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }
        self.first_frame = false;
        self.loader.begin_frame();
        self.apply_theme(ctx);
        self.poll_scan();
        self.poll_watch();
        self.poll_google();
        self.poll_finder();
        if let Some(rx) = &self.update_rx {
            if let Ok(u) = rx.try_recv() {
                self.update = u;
                self.update_rx = None;
            }
        }
        if let Some(rx) = &self.act_rx {
            if let Ok(msg) = rx.try_recv() {
                self.set_status(msg);
                self.act_rx = None;
            }
        }
        self.handle_drops(ctx);
        self.compute_visible();

        let mut acts = vec![];
        self.keyboard(ctx, &mut acts);
        self.top_bar(ctx, &mut acts);
        self.bottom_bar(ctx);
        self.rail(ctx);
        if self.sidebar_open {
            self.sidebar(ctx, &mut acts);
        }
        if self.primary.is_none() && self.gsel.is_some() && matches!(self.view, View::Online(_) | View::Finder) {
            self.google_details(ctx, &mut acts);
        } else {
            self.details(ctx, &mut acts);
        }
        if matches!(self.view, View::Online(_)) {
            self.google_central(ctx, &mut acts);
        } else if self.view == View::Finder {
            self.finder_central(ctx, &mut acts);
        } else {
            self.central(ctx, &mut acts);
        }
        if self.show_settings {
            self.settings_window(ctx);
        }
        if self.family_rename.is_some() {
            self.family_rename_window(ctx);
        }
        self.poll_delete();
        self.delete_window(ctx);
        self.delete_report_window(ctx);
        self.network_window(ctx);
        self.overlays(ctx);
        let changed_view = acts.iter().any(|a| matches!(a, Action::SetView(_)));
        self.apply(acts);
        if changed_view {
            self.selected.clear();
            self.primary = None;
            self.gsel = None;
            self.compute_visible();
        }

        self.loader.end_frame(ctx);
        if self.status.as_ref().map(|s| s.1.elapsed().as_secs() < 7).unwrap_or(false) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        if self.dirty && self.last_save.elapsed().as_millis() > 800 && self.shot.is_none() {
            self.stamp_collections();
            self.cfg.save();
            self.dirty = false;
            self.last_save = Instant::now();
            self.sync_push();
        }
        if self.shot.is_none() && self.sync_check.elapsed().as_secs() >= 3 {
            self.sync_check = Instant::now();
            self.sync_pull(false);
        }
        self.handle_screenshot(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.shot.is_some() {
            return;
        }
        self.cfg.save();
        if self.cfg.deactivate_on_exit {
            let all: Vec<PathBuf> = self.cfg.active.iter().map(PathBuf::from).collect();
            activation::deactivate(&all);
        }
    }
}
