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
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    List,
    Grid,
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
    pub center_align: bool,
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
        std::fs::read_to_string(data_dir().join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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
