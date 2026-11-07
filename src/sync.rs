//! Sync between computers through any cloud-drive folder (Dropbox, Google Drive,
//! OneDrive, iCloud Drive, a network share…).
//!
//! The folder holds:
//!   fontcraft-sync.json   collections, tags and favourites
//!   Fonts/<Collection>/   copies of the font files used by collections
//! Every computer that points FontCraft at the same folder merges the file and adds
//! the Fonts folder to its library, so the collections work everywhere.

use crate::config::{Collection, Config};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncFile {
    pub saved_at: u64,
    pub machine: String,
    pub collections: Vec<Collection>,
    pub deleted: BTreeSet<u64>,
    pub tags: BTreeMap<String, BTreeSet<String>>,
    pub favorites: BTreeSet<String>,
}

pub fn state_file(dir: &Path) -> PathBuf {
    dir.join("fontcraft-sync.json")
}

pub fn fonts_dir(dir: &Path) -> PathBuf {
    dir.join("Fonts")
}

pub fn mtime(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(state_file(dir)).and_then(|m| m.modified()).ok()
}

pub fn read(dir: &Path) -> Option<SyncFile> {
    let s = std::fs::read_to_string(state_file(dir)).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn write(dir: &Path, cfg: &Config, now: u64) -> Result<(), String> {
    let f = SyncFile {
        saved_at: now,
        machine: cfg.machine_id.clone(),
        collections: cfg
            .collections
            .iter()
            .map(|c| Collection { sync_dir: None, ..c.clone() })
            .collect(),
        deleted: cfg.deleted_collections.clone(),
        tags: cfg.tags.clone(),
        favorites: cfg.favorites.clone(),
    };
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("fontcraft-sync.json.tmp");
    let s = serde_json::to_string_pretty(&f).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, s).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, state_file(dir)).map_err(|e| e.to_string())
}

/// Merge another computer's state into ours. Returns true when anything changed.
pub fn merge(cfg: &mut Config, other: &SyncFile) -> bool {
    let before = serde_json::to_string(&(&cfg.collections, &cfg.tags, &cfg.favorites, &cfg.deleted_collections)).unwrap_or_default();
    cfg.deleted_collections.extend(other.deleted.iter().copied());
    let deleted = cfg.deleted_collections.clone();
    cfg.collections.retain(|c| !deleted.contains(&c.id));
    for oc in &other.collections {
        if deleted.contains(&oc.id) {
            continue;
        }
        match cfg.collections.iter_mut().find(|c| c.id == oc.id) {
            Some(mine) => {
                if oc.modified > mine.modified {
                    let keep = mine.sync_dir.clone();
                    *mine = Collection { sync_dir: keep, ..oc.clone() };
                }
            }
            None => cfg.collections.push(Collection { sync_dir: None, ..oc.clone() }),
        }
    }
    for (t, set) in &other.tags {
        cfg.tags.entry(t.clone()).or_default().extend(set.iter().cloned());
    }
    cfg.favorites.extend(other.favorites.iter().cloned());
    let after = serde_json::to_string(&(&cfg.collections, &cfg.tags, &cfg.favorites, &cfg.deleted_collections)).unwrap_or_default();
    before != after
}

/// Copy font files into the sync folder (skips files that are already there).
pub fn copy_fonts(dir: &Path, groups: &[(String, Vec<PathBuf>)]) -> usize {
    let root = fonts_dir(dir);
    let mut n = 0;
    for (name, files) in groups {
        let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let files: Vec<PathBuf> = files.iter().filter(|f| !f.starts_with(&root)).cloned().collect();
        n += crate::extras::copy_files(&files, &root.join(safe.trim()));
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_collection_wins_and_deletions_spread() {
        let mut a = Config::default();
        a.collections.push(Collection { id: 1, name: "Old".into(), color: [0; 3], families: ["Inter".to_string()].into(), sync_dir: None, modified: 10 });
        a.collections.push(Collection { id: 2, name: "Gone".into(), color: [0; 3], families: BTreeSet::new(), sync_dir: None, modified: 10 });
        let other = SyncFile {
            collections: vec![
                Collection { id: 1, name: "New".into(), color: [0; 3], families: ["Lato".to_string()].into(), sync_dir: None, modified: 20 },
                Collection { id: 3, name: "Added".into(), color: [0; 3], families: BTreeSet::new(), sync_dir: None, modified: 5 },
            ],
            deleted: [2].into(),
            favorites: ["Lato".to_string()].into(),
            ..Default::default()
        };
        assert!(merge(&mut a, &other));
        assert_eq!(a.collections.len(), 2);
        assert_eq!(a.collections[0].name, "New");
        assert!(a.favorites.contains("Lato"));
        assert!(!merge(&mut a, &other), "merging twice must change nothing");
    }
}
