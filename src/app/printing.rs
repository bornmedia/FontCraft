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
                    rtl: f.rtl.clone(),
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
                        let msg = format!("PDF with {n} page{} opened – print it or save it from there", if n == 1 { "" } else { "s" });
                        result = Ok((target, msg));
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

    /// The one-page print sheet of one style of a font (waterfall, sample text, characters, running
    /// text – as chosen in the Print tab). `save`: ask where to keep the PDF; otherwise it opens.
    pub(super) fn print_sheet(&mut self, face: usize, save: bool) {
        if self.print_rx.is_some() {
            self.set_status("A PDF is still being made – one moment…");
            return;
        }
        let Some(fi) = self.families.iter().position(|f| f.faces.contains(&face)) else { return };
        let fam = &self.families[fi];
        let m = &self.faces[face].meta;
        let style = m.style.trim();
        let name = if style.is_empty() || style.eq_ignore_ascii_case("regular") || fam.faces.len() == 1 && style.eq_ignore_ascii_case("normal") {
            fam.name.clone()
        } else {
            format!("{} {}", fam.name, style)
        };
        // the small line under the name: style, styles, format, designer, foundry, version
        let facts = self.family_facts(fi);
        let fact = |k: &str| facts.iter().find(|(l, _)| *l == k).map(|(_, v)| v.clone());
        let mut info: Vec<String> = vec![];
        if !style.is_empty() {
            info.push(style.to_string());
        }
        info.extend(["Styles", "Format", "Designer", "Foundry"].iter().filter_map(|k| fact(k)));
        if let Some(v) = fact("Version") {
            info.push(format!("Version {v}"));
        }
        let c = &self.cfg.print_sheet;
        let sample = if self.cfg.preview_text.trim().is_empty() { "The quick brown fox jumps over the lazy dog.".to_string() } else { self.cfg.preview_text.clone() };
        let sheet = pdfprint::Sheet {
            name: name.clone(),
            info: info.join("  ·  "),
            path: m.path.clone(),
            index: m.index,
            rtl: fam.rtl.clone(),
            sizes: if c.waterfall { c.size_list() } else { vec![] },
            waterfall_text: if c.waterfall_sample { sample.clone() } else { String::new() },
            sample: c.sample.then(|| (sample.clone(), c.sample_pt.clamp(6.0, 72.0))),
            charset: c.charset,
            lorem: c.lorem.then(|| (c.lorem_paras.clamp(1, 5) as usize, c.lorem_pt.clamp(5.0, 24.0))),
            date: super::facts::date_of(now()),
            label_font: crate::uifont::interface_font_file(),
        };
        let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let safe = format!("{} – print sheet", if safe.trim().is_empty() { "Font" } else { safe.trim() });
        let dest = if save {
            rfd::FileDialog::new().set_title("Save as PDF").add_filter("PDF", &["pdf"]).set_file_name(format!("{safe}.pdf")).save_file()
        } else {
            Some(config::data_dir().join("print").join(format!("{safe}.pdf")))
        };
        let Some(dest) = dest else { return };
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let mut result = Err(String::new());
            for k in 1..10 {
                let target = if k == 1 {
                    dest.clone()
                } else {
                    let stem = dest.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    dest.with_file_name(format!("{stem} ({k}).pdf"))
                };
                match pdfprint::make_sheet_pdf(&sheet, &target) {
                    Ok(r) => {
                        let msg = if r.left_out.is_empty() {
                            "Print sheet opened – print it or save it from there".to_string()
                        } else {
                            format!("Print sheet opened. Not enough room on the page for: {}", r.left_out.join(", "))
                        };
                        result = Ok((target, msg));
                        break;
                    }
                    Err(e) => result = Err(e),
                }
            }
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.print_rx = Some(rx);
        self.set_status(format!("Making the print sheet of “{name}”…"));
    }

    /// Every glyph of one style at one size (12 pt unless changed), on as many pages as it takes.
    pub(super) fn print_glyphs(&mut self, face: usize, save: bool) {
        if self.print_rx.is_some() {
            self.set_status("A PDF is still being made – one moment…");
            return;
        }
        let Some(fi) = self.families.iter().position(|f| f.faces.contains(&face)) else { return };
        let fam = &self.families[fi];
        let m = &self.faces[face].meta;
        let style = m.style.trim();
        let name = if style.is_empty() || style.eq_ignore_ascii_case("regular") { fam.name.clone() } else { format!("{} {}", fam.name, style) };
        let mut info: Vec<String> = vec![];
        if !style.is_empty() {
            info.push(style.to_string());
        }
        info.push(fam.ext.clone());
        let sheet = pdfprint::GlyphSheet {
            name: name.clone(),
            info: info.join("  ·  "),
            path: m.path.clone(),
            index: m.index,
            pt: self.cfg.print_sheet.glyph_pt.clamp(4.0, 72.0),
            date: super::facts::date_of(now()),
            label_font: crate::uifont::interface_font_file(),
        };
        let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let safe = format!("{} – glyphs", if safe.trim().is_empty() { "Font" } else { safe.trim() });
        let dest = if save {
            rfd::FileDialog::new().set_title("Save as PDF").add_filter("PDF", &["pdf"]).set_file_name(format!("{safe}.pdf")).save_file()
        } else {
            Some(config::data_dir().join("print").join(format!("{safe}.pdf")))
        };
        let Some(dest) = dest else { return };
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let mut result = Err(String::new());
            for k in 1..10 {
                let target = if k == 1 {
                    dest.clone()
                } else {
                    let stem = dest.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    dest.with_file_name(format!("{stem} ({k}).pdf"))
                };
                match pdfprint::make_glyph_pdf(&sheet, &target) {
                    Ok((pages, n)) => {
                        let msg = format!("Glyph overview opened: {n} glyphs on {pages} page{}", if pages == 1 { "" } else { "s" });
                        result = Ok((target, msg));
                        break;
                    }
                    Err(e) => result = Err(e),
                }
            }
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.print_rx = Some(rx);
        self.set_status(format!("Making the glyph overview of “{name}”…"));
    }

    pub(super) fn poll_print(&mut self) {
        let Some(rx) = &self.print_rx else { return };
        let Ok(r) = rx.try_recv() else { return };
        self.print_rx = None;
        match r {
            Ok((path, msg)) => {
                if self.shot.is_none() {
                    extras::open_file(&path);
                }
                self.set_status(format!("{msg} ({})", path.display()));
            }
            Err(e) => self.set_status(format!("The PDF could not be made: {e}")),
        }
    }
}
