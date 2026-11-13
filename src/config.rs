//! Saved settings: folders, collections, active fonts, favourites, preferences.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: u64,
    pub name: String,
    pub color: [u8; 3],
    pub families: BTreeSet<String>,
    #[serde(default)]
    pub sync_dir: Option<PathBuf>,
    /// When it last changed (seconds), so syncing knows which copy is newer.
    #[serde(default)]
    pub modified: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Theme {
    Classic,
    Dark,
    Light,
    Midnight,
    Sepia,
    /// dark, with the sand tone of the app icon for highlights
    Sand,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    List,
    Grid,
}

/// What to do with a font that is imported although it is already in the library.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum DupAction {
    /// copy it anyway, next to the one already there
    ImportDuplicate,
    /// keep the one already in the library and don't copy the new one
    UseExisting,
    /// put the new one in place of the old one (same place: read it again)
    Replace,
    /// leave this font out of the import
    Skip,
}

/// What happens with a folder that is dropped onto FontCraft or imported.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum FolderImport {
    /// add the folder to the library where it is (nothing is copied)
    AddInPlace,
    /// copy all its fonts into the import folder, together
    CopyTogether,
    /// copy its fonts into the import folder, keeping its sub-folders
    CopyKeepStructure,
}

/// A saved filter in the sidebar: the families for which all (or any) of the rules are true.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SmartFilter {
    pub id: u64,
    pub name: String,
    /// true: all rules must be true; false: any of them
    pub all: bool,
    pub rules: Vec<SmartRule>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SmartRule {
    pub field: SmartField,
    /// the second word of the rule: "is" / "is not", "contains" / "doesn't contain", "at least" / "fewer than"…
    pub negate: bool,
    pub value: String,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum SmartField {
    Languages,
    Category,
    Activation,
    Name,
    Designer,
    Tag,
    Collection,
    Format,
    Variable,
    Favorite,
    Source,
    Styles,
    Added,
}

/// How the preview text sits in its row or tile.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    /// full lines stretched to both edges, the last line on the left
    JustifyLeft,
    /// full lines stretched to both edges, the last line centred
    JustifyCenter,
    /// full lines stretched to both edges, the last line on the right
    JustifyRight,
}

impl TextAlign {
    pub const ALL: [TextAlign; 6] = [
        TextAlign::Left,
        TextAlign::Center,
        TextAlign::Right,
        TextAlign::JustifyLeft,
        TextAlign::JustifyCenter,
        TextAlign::JustifyRight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => "Align left",
            TextAlign::Center => "Centre",
            TextAlign::Right => "Align right",
            TextAlign::JustifyLeft => "Justify, last line left",
            TextAlign::JustifyCenter => "Justify, last line centred",
            TextAlign::JustifyRight => "Justify, last line right",
        }
    }
    /// where each line (and the last line of justified text) goes
    pub fn halign(self) -> egui::Align {
        match self {
            TextAlign::Left | TextAlign::JustifyLeft => egui::Align::Min,
            TextAlign::Center | TextAlign::JustifyCenter => egui::Align::Center,
            TextAlign::Right | TextAlign::JustifyRight => egui::Align::Max,
        }
    }
    pub fn justify(self) -> bool {
        matches!(self, TextAlign::JustifyLeft | TextAlign::JustifyCenter | TextAlign::JustifyRight)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortBy {
    NameAz,
    NameZa,
    Recent,
    Styles,
    Popular,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub folders: Vec<PathBuf>,
    pub collections: Vec<Collection>,
    pub active: BTreeSet<String>,
    pub favorites: BTreeSet<String>,
    pub added: BTreeMap<String, u64>,
    pub preview_text: String,
    pub preview_size: f32,
    pub theme: Theme,
    pub tags: BTreeMap<String, BTreeSet<String>>,
    pub invert_preview: bool,
    /// older versions only knew "centred"; read once and turned into `align`
    pub center_align: bool,
    pub align: TextAlign,
    pub collapsed: BTreeSet<String>,
    pub view_mode: ViewMode,
    pub sort: SortBy,
    pub deactivate_on_exit: bool,
    pub launch_at_login: bool,
    pub show_system: bool,
    pub tester_text: String,
    pub check_updates: bool,
    /// cloud-drive folder for syncing collections, tags and favourites between computers
    pub sync_dir: Option<PathBuf>,
    pub machine_id: String,
    pub sync_seen: u64,
    pub deleted_collections: BTreeSet<u64>,
    /// project folders whose documents get their fonts activated automatically
    pub auto_folders: Vec<PathBuf>,
    pub auto_google: bool,
    /// where "Import fonts" copies files (None = inside FontCraft's data folder)
    pub import_dir: Option<PathBuf>,
    pub scroll_speed: u8,
    pub group_families: bool,
    /// "file#index" -> family name chosen by the user
    pub family_overrides: BTreeMap<String, String>,
    pub start_hidden: bool,
    /// closing the window hides it in the system tray (Windows)
    pub hide_to_tray: bool,
    /// the folder system fonts were last moved into instead of being deleted
    pub moved_dir: Option<PathBuf>,
    /// widths of the sidebar and the font details panel, as the person dragged them
    pub sidebar_w: f32,
    pub details_w: f32,
    pub smart_filters: Vec<SmartFilter>,
    // importing
    pub import_activate: bool,
    pub import_report: bool,
    pub import_collection: bool,
    pub import_folders: FolderImport,
    /// don't ask when a font is imported from the place it is already in the library from
    pub dup_same_path_quiet: bool,
    pub dup_same_path: DupAction,
    /// don't ask when another copy of a font is already in the library
    pub dup_quiet: bool,
    pub dup_action: DupAction,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            folders: vec![],
            collections: vec![],
            active: BTreeSet::new(),
            favorites: BTreeSet::new(),
            added: BTreeMap::new(),
            preview_text: "The quick brown fox jumps over the lazy dog.".into(),
            preview_size: 42.0,
            theme: Theme::Dark,
            tags: BTreeMap::new(),
            invert_preview: false,
            center_align: false,
            align: TextAlign::Left,
            collapsed: BTreeSet::new(),
            view_mode: ViewMode::List,
            sort: SortBy::NameAz,
            deactivate_on_exit: false,
            launch_at_login: false,
            show_system: true,
            check_updates: true,
            sync_dir: None,
            machine_id: String::new(),
            sync_seen: 0,
            deleted_collections: BTreeSet::new(),
            auto_folders: vec![],
            auto_google: true,
            import_dir: None,
            scroll_speed: 1,
            group_families: true,
            family_overrides: BTreeMap::new(),
            start_hidden: true,
            hide_to_tray: false,
            moved_dir: None,
            sidebar_w: 200.0,
            details_w: 420.0,
            smart_filters: vec![],
            import_activate: false,
            import_report: true,
            import_collection: false,
            import_folders: FolderImport::AddInPlace,
            dup_same_path_quiet: true,
            dup_same_path: DupAction::UseExisting,
            dup_quiet: false,
            dup_action: DupAction::ImportDuplicate,
            tester_text: "Typography is the craft of endowing human language with a durable visual form. \
Good type disappears into the reading; great type makes the reading a pleasure.\n\n\
ABCDEFGHIJKLMNOPQRSTUVWXYZ\nabcdefghijklmnopqrstuvwxyz\n0123456789 !?&@#%(){}[]"
                .into(),
        }
    }
}

pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("FontCraft");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(h) = std::env::var_os("HOME") {
            return PathBuf::from(h).join("Library/Application Support/FontCraft");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(x).join("fontcraft");
        }
        if let Some(h) = std::env::var_os("HOME") {
            return PathBuf::from(h).join(".config/fontcraft");
        }
    }
    PathBuf::from(".fontcraft")
}

/// The default folder where "Import fonts" copies files to.
pub fn default_import_dir() -> PathBuf {
    data_dir().join("Imported fonts")
}

impl Config {
    /// Folder where "Import fonts" copies files to.
    pub fn import_dir(&self) -> PathBuf {
        self.import_dir.clone().unwrap_or_else(default_import_dir)
    }
}

impl Config {
    pub fn load() -> Self {
        let mut c: Config = std::fs::read_to_string(data_dir().join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if c.center_align {
            c.align = TextAlign::Center;
            c.center_align = false;
        }
        c
    }
    pub fn save(&self) {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let tmp = dir.join("settings.json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, dir.join("settings.json"));
            }
        }
    }
}

pub fn load_cache() -> crate::library::ScanCache {
    std::fs::read(data_dir().join("scan-cache.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_cache(c: &crate::library::ScanCache) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(b) = serde_json::to_vec(c) {
        let _ = std::fs::write(dir.join("scan-cache.json"), b);
    }
}
