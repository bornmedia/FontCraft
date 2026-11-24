//! The "Clean Application Font Caches…" window: choose the programs whose font caches are
//! cleared (the finding and deleting itself is in `fontcache.rs`).

use super::*;
use crate::fontcache::{self, CacheApp, Found};
use egui::{vec2, Align2, RichText};
use std::collections::BTreeSet;

pub(super) struct CacheWin {
    found: Option<Vec<Found>>,
    rx: Option<Receiver<Vec<Found>>>,
    chosen: BTreeSet<u8>,
    result: Option<String>,
}

fn key(a: CacheApp) -> u8 {
    a as u8
}

impl FontCraftApp {
    pub(super) fn cache_open(&mut self) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(fontcache::find_all());
            ctx.request_repaint();
        });
        self.cache_win = Some(CacheWin { found: None, rx: Some(rx), chosen: BTreeSet::new(), result: None });
    }

    pub(super) fn cache_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.cache_win.take() else { return };
        if let Some(rx) = &w.rx {
            if let Ok(f) = rx.try_recv() {
                // everything with something to clean is chosen at first
                if w.found.is_none() && w.result.is_none() {
                    w.chosen = f.iter().filter(|x| !x.paths.is_empty() || x.app == CacheApp::System).map(|x| key(x.app)).collect();
                }
                w.found = Some(f);
                w.rx = None;
            }
        }
        let p = self.p();
        let mut open = true;
        let mut close = false;
        let mut go = false;
        egui::Window::new(RichText::new("Clean Application Font Caches").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(540.0);
                ui.label(
                    RichText::new(
                        "Programs keep their own lists of your fonts. When a font is missing from a font menu, shows up twice or looks wrong, \
                         clearing that program's font cache usually helps. Only the caches are deleted – your fonts stay as they are, \
                         and each program builds its cache again the next time it starts.",
                    )
                    .color(p.weak)
                    .size(12.5),
                );
                ui.add_space(10.0);
                match &w.found {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Looking for font caches…").color(p.weak));
                        });
                    }
                    Some(found) => {
                        for f in found {
                            let has = !f.paths.is_empty() || f.app == CacheApp::System;
                            let mut on = w.chosen.contains(&key(f.app));
                            ui.horizontal(|ui| {
                                if ui.add_enabled(has, egui::Checkbox::new(&mut on, RichText::new(f.app.label()).strong())).changed() {
                                    if on {
                                        w.chosen.insert(key(f.app));
                                    } else {
                                        w.chosen.remove(&key(f.app));
                                    }
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let t = if f.paths.is_empty() {
                                        if f.app == CacheApp::System { String::new() } else { "nothing found".into() }
                                    } else {
                                        format!("{} item{} · {}", f.paths.len(), if f.paths.len() == 1 { "" } else { "s" }, fontcache::human(f.bytes))
                                    };
                                    ui.label(RichText::new(t).size(12.0).color(p.weak));
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.add_space(26.0);
                                ui.add(egui::Label::new(RichText::new(f.app.detail()).size(11.5).color(p.weak)).wrap());
                            });
                            if !f.running.is_empty() && has {
                                ui.horizontal(|ui| {
                                    ui.add_space(26.0);
                                    ui.label(RichText::new(format!("Close {} first – an open program keeps using its cache.", f.running.join(", "))).size(11.5).color(p.accent));
                                });
                            }
                            if !f.paths.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.add_space(22.0);
                                    egui::CollapsingHeader::new(RichText::new("What is deleted").size(11.5).color(p.weak)).id_salt(("cachepaths", key(f.app))).show(ui, |ui| {
                                        for path in &f.paths {
                                            ui.add(egui::Label::new(RichText::new(path.display().to_string()).size(11.0).color(p.weak)).truncate());
                                        }
                                    });
                                });
                            }
                            ui.add_space(6.0);
                        }
                    }
                }
                if let Some(r) = &w.result {
                    ui.add_space(4.0);
                    ui.label(RichText::new(r).color(p.accent));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ready = w.found.is_some() && !w.chosen.is_empty();
                    let b = egui::Button::new(RichText::new("Clean selected caches").color(p.on_accent).strong()).fill(p.accent);
                    if ui.add_enabled(ready, b).clicked() {
                        go = true;
                    }
                    if ui.button(if w.result.is_some() { "Done" } else { "Cancel" }).clicked() {
                        close = true;
                    }
                });
            });
        if go {
            if let Some(found) = &w.found {
                let mut cleaned = vec![];
                let mut problems = vec![];
                for f in found.iter().filter(|f| w.chosen.contains(&key(f.app))) {
                    let (n, errs) = fontcache::clean(f);
                    if n > 0 {
                        cleaned.push(f.app.label());
                    }
                    if !errs.is_empty() {
                        let hint = if f.running.is_empty() { String::new() } else { format!(" – close {} and try again", f.running.join(", ")) };
                        problems.push(format!("{}: {} item{} in use{hint}", f.app.label(), errs.len(), if errs.len() == 1 { "" } else { "s" }));
                    }
                }
                let mut msg = if cleaned.is_empty() { "Nothing was cleaned.".to_string() } else { format!("Cleaned: {}.", cleaned.join(", ")) };
                if !problems.is_empty() {
                    msg.push_str(&format!(" Not everything could be deleted ({}).", problems.join("; ")));
                }
                self.set_status(msg.clone());
                w.result = Some(msg);
                w.chosen.clear();
            }
            // look again, so the sizes show what is left
            let (tx, rx) = channel();
            let ctx2 = self.ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(fontcache::find_all());
                ctx2.request_repaint();
            });
            w.rx = Some(rx);
        }
        if open && !close {
            self.cache_win = Some(w);
        }
    }
}
