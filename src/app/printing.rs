//! "Print…" and "Save as PDF…" for a collection, tag or smart filter (the PDF itself is made in
//! `pdfprint.rs`, in the background).

use super::*;
use crate::pdfprint::{self, HAlign, Item, Spec};

impl FontCraftApp {
    /// Make a PDF of these families with the current preview text, size, colours and alignment.
    /// `save`: ask where to keep it; otherwise it opens in the PDF app to print or save from there.
    pub(super) fn print_pdf(&mut self, fams: Vec<usize>, title: String, save: bool) {
        if fams.is_empty() {
            self.set_status(format!("“{title}” has no fonts to print"));
            return;
        }
        if self.print_rx.is_some() {
            self.set_status("A PDF is still being made – one moment…");
            return;
        }
        let safe: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let safe = if safe.trim().is_empty() { "FontCraft".to_string() } else { safe.trim().to_string() };
        let dest = if save {
            rfd::FileDialog::new().set_title("Save as PDF").add_filter("PDF", &["pdf"]).set_file_name(format!("{safe}.pdf")).save_file()
        } else {
            Some(config::data_dir().join("print").join(format!("{safe}.pdf")))
        };
        let Some(dest) = dest else { return };
        let p = self.p();
        let (bg, fg) = self.preview_colors();
        let bg = bg.unwrap_or(p.content);
        let rgb = |c: Color32| [c.r(), c.g(), c.b()];
        let items: Vec<Item> = fams
            .iter()
            .filter_map(|fi| self.families.get(*fi))
            .map(|f| {
                let face = &self.faces[f.primary].meta;
                let n = f.faces.len();
                Item {
                    name: f.name.clone(),
                    info: format!("{} · {n} style{}", f.ext, if n == 1 { "" } else { "s" }),
                    path: face.path.clone(),
                    index: face.index,
                }
            })
            .collect();
        let spec = Spec {
            title: title.clone(),
            items,
            text: if self.cfg.preview_text.trim().is_empty() { "The quick brown fox jumps over the lazy dog.".into() } else { self.cfg.preview_text.clone() },
            size_px: self.cfg.preview_size,
            fg: rgb(fg),
            bg: rgb(bg),
            align: match self.cfg.align.halign() {
                egui::Align::Min => HAlign::Left,
                egui::Align::Center => HAlign::Center,
                egui::Align::Max => HAlign::Right,
            },
            label_font: crate::uifont::interface_font_file(),
        };
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            // a PDF still open in a viewer can't be overwritten: then the next free name is used
            let mut result = Err(String::new());
            for k in 1..10 {
                let target = if k == 1 {
                    dest.clone()
                } else {
                    let stem = dest.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    dest.with_file_name(format!("{stem} ({k}).pdf"))
                };
                match pdfprint::make_pdf(&spec, &target) {
                    Ok(n) => {
                        result = Ok((target, n));
                        break;
                    }
                    Err(e) => result = Err(e),
                }
            }
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.print_rx = Some(rx);
        self.set_status(format!("Making a PDF of “{title}”…"));
    }

    pub(super) fn poll_print(&mut self) {
        let Some(rx) = &self.print_rx else { return };
        let Ok(r) = rx.try_recv() else { return };
        self.print_rx = None;
        match r {
            Ok((path, n)) => {
                if self.shot.is_none() {
                    extras::open_file(&path);
                }
                self.set_status(format!(
                    "PDF with {n} page{} opened – print it or save it from there ({})",
                    if n == 1 { "" } else { "s" },
                    path.display()
                ));
            }
            Err(e) => self.set_status(format!("The PDF could not be made: {e}")),
        }
    }
}
