//! Keeps FontCraft in step with the font folders on disk.
//!
//! Every two seconds the folders (and their sub-folders) are looked at; a folder's
//! "last changed" time moves whenever a file inside it is added, removed or renamed.
//! When anything changed, FontCraft rescans (only new files are actually read).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type WatchList = Arc<Mutex<Vec<PathBuf>>>;

fn signature(folders: &[PathBuf]) -> u64 {
    let mut h = DefaultHasher::new();
    for root in folders {
        root.hash(&mut h);
        for e in walkdir::WalkDir::new(root).follow_links(true).into_iter().filter_map(|e| e.ok()) {
            let ft = e.file_type();
            if ft.is_dir() {
                e.path().hash(&mut h);
                if let Ok(m) = std::fs::metadata(e.path()).and_then(|m| m.modified()) {
                    m.hash(&mut h);
                }
            } else if ft.is_file() && crate::library::is_font_file(e.path()) {
                // file names also go in, for file systems that don't update folder times
                e.file_name().hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Starts watching; sends a message whenever something in the watched folders changed.
pub fn spawn(list: WatchList, ctx: egui::Context) -> Receiver<()> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut last: Option<(Vec<PathBuf>, u64)> = None;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let folders = list.lock().map(|l| l.clone()).unwrap_or_default();
            let sig = signature(&folders);
            match &last {
                Some((f, s)) if *f == folders && *s != sig => {
                    if tx.send(()).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
                _ => {}
            }
            last = Some((folders, sig));
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    #[test]
    fn signature_changes_when_a_font_is_added() {
        let dir = std::env::temp_dir().join("fontcraft-watch-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let a = super::signature(&[dir.clone()]);
        std::fs::write(dir.join("sub").join("New.ttf"), b"x").unwrap();
        let b = super::signature(&[dir.clone()]);
        assert_ne!(a, b);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
