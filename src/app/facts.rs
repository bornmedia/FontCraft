//! A family's facts for the font panel (category, source, designer, languages, date added…) and
//! the collections, tags and folders a search also finds.

use super::*;
use egui::{RichText, Ui};

/// A day (seconds since 1970) as year-month-day.
pub(super) fn date_of(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    // days since 1970-01-01 to a calendar date (Howard Hinnant's method)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// "Version 2.013;PS 2.013;hotconv 1.0" → "2.013"
fn short_version(v: &str) -> String {
    let v = v.trim();
    let v = v.strip_prefix("Version").or_else(|| v.strip_prefix("version")).unwrap_or(v).trim();
    v.split([';', ' ']).next().unwrap_or("").to_string()
}

impl FontCraftApp {
    /// The facts about a family shown at the top of the font panel: (label, value), only the known ones.
    pub(super) fn family_facts(&self, fi: usize) -> Vec<(&'static str, String)> {
        let fam = &self.families[fi];
        let m = &self.faces[fam.primary].meta;
        let mut out: Vec<(&'static str, String)> = vec![];
        let n = fam.faces.len();
        out.push(("Styles", format!("{n} style{}", if n == 1 { "" } else { "s" })));
        out.push(("Format", fam.ext.clone()));
        let names = [("serif", "Serif"), ("sans", "Sans Serif"), ("script", "Script"), ("display", "Display"), ("symbol", "Symbol")];
        let category = if fam.mono {
            Some("Monospace".to_string())
        } else {
            names.iter().find(|(t, _)| fam.auto_tags.contains(t)).map(|(_, n)| n.to_string())
        };
        if let Some(c) = category {
            out.push(("Category", c));
        }
        let source = if fam.system {
            "Installed on this computer".to_string()
        } else if let Some(p) = Provider::ALL.iter().find(|p| m.path.starts_with(p.fonts_dir())) {
            p.label().to_string()
        } else {
            let folder = m.path.parent().and_then(|d| self.node_index.get(d)).map(|i| self.nodes[*i].name.clone());
            match self.faces[fam.primary].folder.and_then(|i| self.cfg.folders.get(i)) {
                Some(f) if *f == self.cfg.import_dir() => "Imported fonts".into(),
                _ => folder.unwrap_or_default(),
            }
        };
        if !source.is_empty() {
            out.push(("Source", source));
        }
        if !m.designer.trim().is_empty() {
            out.push(("Designer", m.designer.trim().to_string()));
        }
        if !m.manufacturer.trim().is_empty() {
            out.push(("Foundry", m.manufacturer.trim().to_string()));
        }
        let langs: Vec<&str> = crate::library::LANGUAGES
            .iter()
            .enumerate()
            .filter(|(k, _)| fam.scripts & (1 << k) != 0)
            .map(|(_, (n, _))| n.split(" (").next().unwrap_or(n))
            .collect();
        if !langs.is_empty() {
            out.push(("Languages", langs.join(", ")));
        }
        let v = short_version(&m.version);
        if !v.is_empty() {
            out.push(("Version", v));
        }
        if fam.added > 0 {
            out.push(("Added", date_of(fam.added)));
        }
        if !fam.auto_tags.is_empty() {
            out.push(("Looks", fam.auto_tags.join(", ")));
        }
        out
    }

    /// While searching: the one or two best matching collections, tags and folders, above the
    /// fonts (which keep their order). Nothing when none of them matches.
    pub(super) fn search_groups(&mut self, ui: &mut Ui, p: &Pal, acts: &mut Vec<Action>) {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() || !self.cfg.search_groups {
            return;
        }
        // the best first: a name that starts with the search, then the shorter name
        let rank = |name: &str| -> Option<(bool, usize)> {
            let l = name.to_lowercase();
            l.contains(&q).then(|| (!l.starts_with(&q), l.len()))
        };
        let mut colls: Vec<((bool, usize), View, String, usize)> = self
            .cfg
            .collections
            .iter()
            .filter_map(|c| rank(&c.name).map(|r| (r, View::Collection(c.id), c.name.clone(), c.families.len())))
            .collect();
        let mut tags: Vec<((bool, usize), View, String, usize)> = self
            .cfg
            .tags
            .iter()
            .filter_map(|(t, s)| rank(t).map(|r| (r, View::Tag(t.clone()), format!("#{t}"), s.len())))
            .collect();
        let mut folders: Vec<((bool, usize), View, String, usize)> =
            self.nodes.iter().filter_map(|n| rank(&n.name).map(|r| (r, View::Folder(n.path.clone()), n.name.clone(), n.fams.len()))).collect();
        for v in [&mut colls, &mut tags, &mut folders] {
            v.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.to_lowercase().cmp(&b.2.to_lowercase())));
            v.truncate(2);
        }
        if colls.is_empty() && tags.is_empty() && folders.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.add_space(14.0);
            for (title, list) in [("COLLECTIONS", colls), ("TAGS", tags), ("FOLDERS", folders)] {
                if list.is_empty() {
                    continue;
                }
                ui.label(RichText::new(title).size(10.5).strong().color(p.weak));
                for (_, view, name, count) in list {
                    let chip = egui::Button::new(RichText::new(format!("{name}  {count}")).size(12.0).color(p.text))
                        .fill(p.input)
                        .corner_radius(CornerRadius::same(10));
                    if ui.add(chip).on_hover_text("Show it").clicked() {
                        acts.push(Action::SetView(view));
                        self.search.clear();
                    }
                }
                ui.add_space(12.0);
            }
        });
        ui.add_space(4.0);
    }
}
