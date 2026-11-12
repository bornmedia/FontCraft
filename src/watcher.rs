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
        // a network folder that is away (or back) counts as a change too
        if crate::netfs::is_network_path(root) {
            let up = crate::netfs::reachable(root, Duration::from_secs(4));
            up.hash(&mut h);
            if !up {
                continue;
            }
        }
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
        // Folders on this computer are looked at every 2 seconds; network folders every 30
        // seconds, so a big shared folder doesn't keep the network busy.
        let mut last: Option<(Vec<PathBuf>, u64)> = None;
        let mut last_net: Option<(Vec<PathBuf>, u64)> = None;
        let mut tick: u32 = 0;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let folders = list.lock().map(|l| l.clone()).unwrap_or_default();
            let (net, local): (Vec<PathBuf>, Vec<PathBuf>) = folders.into_iter().partition(|f| crate::netfs::is_network_path(f));
            let mut changed = step(&mut last, local);
            if tick % 15 == 0 || last_net.as_ref().map(|l| l.0 != net).unwrap_or(true) {
                changed |= step(&mut last_net, net);
            }
            tick = tick.wrapping_add(1);
            if changed {
                if tx.send(()).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        }
    });
    rx
}

/// Looks at the folders again; true when something changed since the last look.
fn step(last: &mut Option<(Vec<PathBuf>, u64)>, folders: Vec<PathBuf>) -> bool {
    let sig = signature(&folders);
    let changed = matches!(last, Some((f, s)) if *f == folders && *s != sig);
    *last = Some((folders, sig));
    changed
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
