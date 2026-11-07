//! Loads font files into the UI on demand so previews can be drawn in each font.
//! Files are read on a worker thread; only fonts that are on screen are kept.

use egui::{FontData, FontDefinitions, FontFamily};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

const MAX_LOADED: usize = 260;

pub struct FontLoader {
    defs: FontDefinitions,
    req_tx: Sender<(u64, PathBuf, u32)>,
    res_rx: Receiver<(u64, Option<Vec<u8>>, u32)>,
    requested: HashSet<u64>,
    failed: HashSet<u64>,
    staged: Vec<u64>,
    ready: HashSet<u64>,
    last_used: HashMap<u64, u64>,
    frame: u64,
}

fn key(id: u64) -> String {
    format!("fc{id:016x}")
}

impl FontLoader {
    pub fn new(ctx: egui::Context, defs: FontDefinitions) -> Self {
        let (req_tx, req_rx) = channel::<(u64, PathBuf, u32)>();
        let (res_tx, res_rx) = channel();
        std::thread::spawn(move || {
            while let Ok((id, path, index)) = req_rx.recv() {
                let bytes = std::fs::read(&path).ok().filter(|b| {
                    ab_glyph::FontRef::try_from_slice_and_index(b, index).is_ok()
                });
                if res_tx.send((id, bytes, index)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            defs,
            req_tx,
            res_rx,
            requested: HashSet::new(),
            failed: HashSet::new(),
            staged: vec![],
            ready: HashSet::new(),
            last_used: HashMap::new(),
            frame: 0,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame += 1;
        for id in self.staged.drain(..) {
            self.ready.insert(id);
        }
    }

    /// The font family to draw a face with, or None while it is still loading.
    pub fn family(&mut self, id: u64, path: &PathBuf, index: u32) -> Option<FontFamily> {
        self.last_used.insert(id, self.frame);
        if self.ready.contains(&id) {
            return Some(FontFamily::Name(key(id).into()));
        }
        if !self.requested.contains(&id) && !self.failed.contains(&id) {
            self.requested.insert(id);
            let _ = self.req_tx.send((id, path.clone(), index));
        }
        None
    }

    pub fn is_failed(&self, id: u64) -> bool {
        self.failed.contains(&id)
    }

    pub fn pending(&self) -> usize {
        self.requested.len().saturating_sub(self.ready.len() + self.staged.len())
    }

    pub fn end_frame(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        while let Ok((id, bytes, index)) = self.res_rx.try_recv() {
            match bytes {
                Some(b) => {
                    let mut fd = FontData::from_owned(b);
                    fd.index = index;
                    let k = key(id);
                    self.defs.font_data.insert(k.clone(), Arc::new(fd));
                    let mut fam = vec![k];
                    if let Some(p) = self.defs.families.get(&FontFamily::Proportional) {
                        fam.extend(p.iter().cloned());
                    }
                    self.defs.families.insert(FontFamily::Name(key(id).into()), fam);
                    self.staged.push(id);
                    changed = true;
                }
                None => {
                    self.requested.remove(&id);
                    self.failed.insert(id);
                }
            }
        }
        // Forget fonts that have not been on screen for a while.
        let loaded = self.ready.len() + self.staged.len();
        if loaded > MAX_LOADED {
            let mut old: Vec<(u64, u64)> = self
                .ready
                .iter()
                .map(|id| (*id, *self.last_used.get(id).unwrap_or(&0)))
                .filter(|(_, f)| *f + 2 < self.frame)
                .collect();
            old.sort_by_key(|(_, f)| *f);
            for (id, _) in old.into_iter().take(loaded - MAX_LOADED) {
                self.ready.remove(&id);
                self.requested.remove(&id);
                self.defs.font_data.remove(&key(id));
                self.defs.families.remove(&FontFamily::Name(key(id).into()));
                changed = true;
            }
        }
        if changed {
            ctx.set_fonts(self.defs.clone());
        }
    }
}
