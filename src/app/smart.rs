//! Smart filters: saved rules ("Languages contains Arabic", "Category is Sans Serif",
//! "Activation is Activated"…) that show every family matching all (or any) of them.

use super::*;
use crate::config::{SmartField, SmartFilter, SmartRule};
use crate::library::LANGUAGES;
use egui::{vec2, Align2, RichText, Ui};

pub const FIELDS: [SmartField; 13] = [
    SmartField::Languages,
    SmartField::Category,
    SmartField::Activation,
    SmartField::Name,
    SmartField::Designer,
    SmartField::Tag,
    SmartField::Collection,
    SmartField::Format,
    SmartField::Variable,
    SmartField::Favorite,
    SmartField::Source,
    SmartField::Styles,
    SmartField::Added,
];

const CATEGORIES: [&str; 6] = ["Serif", "Sans Serif", "Script", "Display", "Symbol", "Monospace"];
const ACTIVATION: [&str; 3] = ["Activated", "Deactivated", "Partly activated"];
const FORMATS: [&str; 4] = ["TTF", "OTF", "TTC", "OTC"];
const SOURCES: [&str; 6] = ["Installed on this computer", "My folders", "Network folders", "Google Fonts", "Fontshare", "Font Squirrel"];

fn field_label(f: SmartField) -> &'static str {
    match f {
        SmartField::Languages => "Languages",
        SmartField::Category => "Category",
        SmartField::Activation => "Activation",
        SmartField::Name => "Name",
        SmartField::Designer => "Designer or foundry",
        SmartField::Tag => "Tag",
        SmartField::Collection => "Collection",
        SmartField::Format => "Format",
        SmartField::Variable => "Variable font",
        SmartField::Favorite => "Favorite",
        SmartField::Source => "Source",
        SmartField::Styles => "Number of styles",
        SmartField::Added => "Added to FontCraft",
    }
}

/// The two ways a rule can be read: (normal, negated).
fn ops(f: SmartField) -> (&'static str, &'static str) {
    match f {
        SmartField::Languages | SmartField::Name | SmartField::Designer => ("contains", "doesn't contain"),
        SmartField::Tag => ("has", "doesn't have"),
        SmartField::Collection => ("is in", "is not in"),
        SmartField::Variable | SmartField::Favorite => ("yes", "no"),
        SmartField::Styles => ("at least", "fewer than"),
        SmartField::Added => ("in the last", "more than"),
        _ => ("is", "is not"),
    }
}

enum Value {
    Choice(Vec<String>),
    Text,
    Number(&'static str),
    Nothing,
}

impl FontCraftApp {
    fn value_kind(&self, f: SmartField) -> Value {
        let list = |v: &[&str]| Value::Choice(v.iter().map(|s| s.to_string()).collect());
        match f {
            SmartField::Languages => Value::Choice(LANGUAGES.iter().map(|(n, _)| n.to_string()).collect()),
            SmartField::Category => list(&CATEGORIES),
            SmartField::Activation => list(&ACTIVATION),
            SmartField::Format => list(&FORMATS),
            SmartField::Source => list(&SOURCES),
            SmartField::Tag => {
                let mut v: Vec<String> = self.cfg.tags.keys().cloned().collect();
                for t in [
                    "serif", "sans", "script", "display", "symbol", "mono", "variable", "italic", "heavy", "light", "condensed", "wide",
                    "high-contrast", "monoline",
                ] {
                    if !v.iter().any(|x| x == t) {
                        v.push(t.to_string());
                    }
                }
                Value::Choice(v)
            }
            SmartField::Collection => Value::Choice(self.cfg.collections.iter().map(|c| c.name.clone()).collect()),
            SmartField::Name | SmartField::Designer => Value::Text,
            SmartField::Styles => Value::Number("styles"),
            SmartField::Added => Value::Number("days"),
            SmartField::Variable | SmartField::Favorite => Value::Nothing,
        }
    }

    fn default_value(&self, f: SmartField) -> String {
        match self.value_kind(f) {
            Value::Choice(v) => v.first().cloned().unwrap_or_default(),
            Value::Number(_) if f == SmartField::Added => "30".into(),
            Value::Number(_) => "4".into(),
            _ => String::new(),
        }
    }

    fn rule_true(&self, r: &SmartRule, fi: usize) -> bool {
        let f = &self.families[fi];
        let v = r.value.trim();
        let low = v.to_lowercase();
        let hit = match r.field {
            SmartField::Languages => LANGUAGES.iter().position(|(n, _)| *n == v).map(|k| f.scripts & (1 << k) != 0).unwrap_or(false),
            SmartField::Category => match v {
                "Serif" => f.auto_tags.contains(&"serif"),
                "Sans Serif" => f.auto_tags.contains(&"sans"),
                "Script" => f.auto_tags.contains(&"script"),
                "Display" => f.auto_tags.contains(&"display"),
                "Symbol" => f.auto_tags.contains(&"symbol"),
                "Monospace" => f.mono,
                _ => false,
            },
            SmartField::Activation => {
                let (a, n) = self.fam_state(fi);
                match v {
                    "Activated" => n > 0 && a == n,
                    "Deactivated" => a == 0,
                    _ => a > 0 && a < n,
                }
            }
            SmartField::Name => low.is_empty() || f.lower.contains(&low),
            SmartField::Designer => {
                low.is_empty()
                    || f.faces.iter().any(|i| {
                        let m = &self.faces[*i].meta;
                        m.designer.to_lowercase().contains(&low) || m.manufacturer.to_lowercase().contains(&low)
                    })
            }
            SmartField::Tag => self.cfg.tags.get(v).map(|s| s.contains(&f.name)).unwrap_or(false) || f.auto_tags.contains(&v),
            SmartField::Collection => {
                self.cfg.collections.iter().find(|c| c.name == v).map(|c| self.in_named_set(f, &c.families)).unwrap_or(false)
            }
            SmartField::Format => f.ext.eq_ignore_ascii_case(v),
            SmartField::Variable => f.variable,
            SmartField::Favorite => self.cfg.favorites.contains(&f.name),
            SmartField::Source => {
                let path = &self.faces[f.primary].meta.path;
                let online = |p: Provider| path.starts_with(p.fonts_dir());
                let any_online = Provider::ALL.iter().any(|p| online(*p));
                match v {
                    "Installed on this computer" => f.system,
                    "Google Fonts" => online(Provider::Google),
                    "Fontshare" => online(Provider::Fontshare),
                    "Font Squirrel" => online(Provider::FontSquirrel),
                    "Network folders" => !f.system && self.is_net_folder(path),
                    _ => !f.system && !any_online && !self.is_net_folder(path),
                }
            }
            SmartField::Styles => v.parse::<usize>().map(|n| f.faces.len() >= n).unwrap_or(true),
            SmartField::Added => match v.parse::<u64>() {
                Ok(days) => f.added > 0 && f.added + days * 86_400 >= now(),
                Err(_) => true,
            },
        };
        hit != r.negate
    }

    /// Does this family match the smart filter?
    pub(super) fn smart_matches(&self, s: &SmartFilter, fi: usize) -> bool {
        if s.rules.is_empty() {
            return true;
        }
        if s.all {
            s.rules.iter().all(|r| self.rule_true(r, fi))
        } else {
            s.rules.iter().any(|r| self.rule_true(r, fi))
        }
    }

    pub(super) fn smart_new(&mut self) {
        let id = activation::fnv(&format!("{:?}{}", Instant::now(), self.cfg.smart_filters.len())) ^ now();
        let rule = SmartRule { field: SmartField::Activation, negate: false, value: "Activated".into() };
        let n = self.cfg.smart_filters.len() + 1;
        self.smart_edit = Some((SmartFilter { id, name: format!("Smart filter {n}"), all: true, rules: vec![rule] }, true));
    }

    pub(super) fn smart_edit_open(&mut self, id: u64) {
        if let Some(s) = self.cfg.smart_filters.iter().find(|s| s.id == id) {
            self.smart_edit = Some((s.clone(), false));
        }
    }

    pub(super) fn smart_duplicate(&mut self, id: u64) {
        if let Some(s) = self.cfg.smart_filters.iter().find(|s| s.id == id).cloned() {
            let new_id = activation::fnv(&format!("{:?}{id}", Instant::now())) ^ now();
            self.smart_edit = Some((SmartFilter { id: new_id, name: format!("{} copy", s.name), ..s }, true));
        }
    }

    /// The window for making or changing a smart filter.
    pub(super) fn smart_window(&mut self, ctx: &egui::Context) {
        let Some((mut sf, is_new)) = self.smart_edit.take() else { return };
        let p = self.p();
        let mut open = true;
        let mut done: Option<bool> = None;
        // how many families match right now
        let matching = (0..self.families.len())
            .filter(|i| (!self.families[*i].system || self.cfg.show_system) && self.smart_matches(&sf, *i))
            .count();
        egui::Window::new(RichText::new(if is_new { "New smart filter" } else { "Edit smart filter" }).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(640.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(640.0);
                ui.horizontal(|ui| {
                    ui.label("Smart filter name:");
                    ui.add(egui::TextEdit::singleline(&mut sf.name).desired_width(f32::INFINITY));
                });
                ui.add_space(6.0);
                egui::Frame::new().stroke(Stroke::new(1.0_f32, p.line)).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::same(8)).show(
                    ui,
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_min_height(190.0);
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("sf-all").selected_text(if sf.all { "All" } else { "Any" }).width(70.0).show_ui(ui, |ui| {
                                ui.selectable_value(&mut sf.all, true, "All");
                                ui.selectable_value(&mut sf.all, false, "Any");
                            });
                            ui.label("of the following are true");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(" + ").on_hover_text("Add a rule").clicked() {
                                    let f = SmartField::Category;
                                    sf.rules.push(SmartRule { field: f, negate: false, value: self.default_value(f) });
                                }
                            });
                        });
                        ui.separator();
                        let mut remove = None;
                        let mut insert = None;
                        for k in 0..sf.rules.len() {
                            ui.horizontal(|ui| {
                                ui.add_space(24.0);
                                self.rule_row(ui, k, &mut sf.rules[k]);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(" + ").on_hover_text("Add a rule below").clicked() {
                                        insert = Some(k + 1);
                                    }
                                    if ui.button(" − ").on_hover_text("Remove this rule").clicked() {
                                        remove = Some(k);
                                    }
                                });
                            });
                            ui.separator();
                        }
                        if let Some(k) = remove {
                            sf.rules.remove(k);
                        }
                        if let Some(k) = insert {
                            let f = sf.rules.get(k - 1).map(|r| r.field).unwrap_or(SmartField::Category);
                            sf.rules.insert(k, SmartRule { field: f, negate: false, value: self.default_value(f) });
                        }
                        if sf.rules.is_empty() {
                            ui.label(RichText::new("No rules yet – every font is shown. Press + to add one.").color(p.weak));
                        }
                    },
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{matching} famil{} match", if matching == 1 { "y" } else { "ies" })).color(p.weak));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ok = egui::Button::new(RichText::new("  OK  ").color(p.on_accent).strong()).fill(p.accent);
                        if ui.add_enabled(!sf.name.trim().is_empty(), ok).clicked() {
                            done = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            done = Some(false);
                        }
                    });
                });
            });
        match done {
            Some(true) => {
                sf.name = sf.name.trim().to_string();
                let id = sf.id;
                match self.cfg.smart_filters.iter_mut().find(|s| s.id == id) {
                    Some(s) => *s = sf,
                    None => self.cfg.smart_filters.push(sf),
                }
                self.cfg.collapsed.remove("sec:smart");
                self.view = View::Smart(id);
                self.selected.clear();
                self.primary = None;
                self.dirty = true;
            }
            Some(false) => {}
            None if open => self.smart_edit = Some((sf, is_new)),
            None => {}
        }
    }

    /// One rule: what to look at, how, and the value.
    fn rule_row(&self, ui: &mut Ui, k: usize, r: &mut SmartRule) {
        let before = r.field;
        egui::ComboBox::from_id_salt(("sf-field", k)).selected_text(field_label(r.field)).width(170.0).show_ui(ui, |ui| {
            for f in FIELDS {
                ui.selectable_value(&mut r.field, f, field_label(f));
            }
        });
        if r.field != before {
            r.negate = false;
            r.value = self.default_value(r.field);
        }
        let (yes, no) = ops(r.field);
        egui::ComboBox::from_id_salt(("sf-op", k)).selected_text(if r.negate { no } else { yes }).width(120.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut r.negate, false, yes);
            ui.selectable_value(&mut r.negate, true, no);
        });
        match self.value_kind(r.field) {
            Value::Choice(list) => {
                egui::ComboBox::from_id_salt(("sf-value", k)).selected_text(r.value.clone()).width(190.0).show_ui(ui, |ui| {
                    if list.is_empty() {
                        ui.label("(none yet)");
                    }
                    for v in list {
                        let chosen = r.value == v;
                        if ui.selectable_label(chosen, &v).clicked() {
                            r.value = v;
                        }
                    }
                });
            }
            Value::Text => {
                ui.add(egui::TextEdit::singleline(&mut r.value).desired_width(190.0).hint_text("text"));
            }
            Value::Number(unit) => {
                let mut n: u32 = r.value.trim().parse().unwrap_or(1);
                if ui.add(egui::DragValue::new(&mut n).range(1..=9999)).changed() {
                    r.value = n.to_string();
                }
                ui.label(unit);
            }
            Value::Nothing => {}
        }
    }
}
