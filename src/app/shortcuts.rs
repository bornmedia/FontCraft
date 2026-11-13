//! Keyboard shortcuts: one list that both runs the shortcuts and is shown in Settings ›
//! Keyboard shortcuts, with the key names of this computer (⌘ ⇧ on a Mac, Strg/Umschalt on a
//! German Windows keyboard, Ctrl/Shift elsewhere).

use super::*;
use egui::{vec2, Align2, FontId, Key, Rect, RichText, Sense, Ui};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    Activate,
    Deactivate,
    Toggle,
    Delete,
    Favorite,
    Rename,
    CopyName,
    Reveal,
    Print,
    Up,
    Down,
    Left,
    Right,
    First,
    Last,
    SelectAll,
    Escape,
    ListView,
    TileView,
    Bigger,
    Smaller,
    ResetSize,
    Sidebar,
    Theme,
    Invert,
    Search,
    Finder,
    AddFolder,
    Import,
    NewCollection,
    NewTag,
    Rescan,
    Settings,
    Help,
}

/// Which modifier keys go with the key. "Cmd" is ⌘ on a Mac and Ctrl everywhere else.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum M {
    None,
    Cmd,
    CmdShift,
}

pub struct Shortcut {
    pub cmd: Cmd,
    pub mods: M,
    pub key: Key,
    pub group: &'static str,
    pub what: &'static str,
}

const fn sc(cmd: Cmd, mods: M, key: Key, group: &'static str, what: &'static str) -> Shortcut {
    Shortcut { cmd, mods, key, group, what }
}

const FONTS: &str = "Fonts";
const SELECT: &str = "Moving and selecting";
const VIEW: &str = "View";
const LIB: &str = "Library";
const APP: &str = "FontCraft";

/// Every shortcut. Several keys for the same command are shown together ("or").
/// The ones with Shift come before the same letter without it, so they win.
pub const ALL: &[Shortcut] = &[
    sc(Cmd::Activate, M::CmdShift, Key::A, FONTS, "Activate the selected fonts"),
    sc(Cmd::Deactivate, M::CmdShift, Key::D, FONTS, "Deactivate the selected fonts"),
    sc(Cmd::Toggle, M::None, Key::Space, FONTS, "Switch the selected fonts on or off"),
    sc(Cmd::Delete, M::None, Key::Delete, FONTS, "Delete the selected fonts… (asks first)"),
    sc(Cmd::Delete, M::Cmd, Key::Backspace, FONTS, "Delete the selected fonts… (asks first)"),
    sc(Cmd::Favorite, M::Cmd, Key::D, FONTS, "Add to favorites / remove from favorites"),
    sc(Cmd::Rename, M::None, Key::F2, FONTS, "Change family name…"),
    sc(Cmd::CopyName, M::CmdShift, Key::C, FONTS, "Copy the family name"),
    sc(Cmd::Reveal, M::CmdShift, Key::R, FONTS, "Show the font file in its folder"),
    sc(Cmd::Print, M::Cmd, Key::P, FONTS, "Print a specimen…"),
    sc(Cmd::Up, M::None, Key::ArrowUp, SELECT, "Previous font (tiles: the row above)"),
    sc(Cmd::Down, M::None, Key::ArrowDown, SELECT, "Next font (tiles: the row below)"),
    sc(Cmd::Left, M::None, Key::ArrowLeft, SELECT, "Previous tile"),
    sc(Cmd::Right, M::None, Key::ArrowRight, SELECT, "Next tile"),
    sc(Cmd::First, M::None, Key::Home, SELECT, "First font"),
    sc(Cmd::Last, M::None, Key::End, SELECT, "Last font"),
    sc(Cmd::SelectAll, M::Cmd, Key::A, SELECT, "Select all fonts in the list"),
    sc(Cmd::Escape, M::None, Key::Escape, SELECT, "Clear the selection and close the details"),
    sc(Cmd::ListView, M::Cmd, Key::Num1, VIEW, "List view"),
    sc(Cmd::TileView, M::Cmd, Key::Num2, VIEW, "Tile view"),
    sc(Cmd::Bigger, M::Cmd, Key::Plus, VIEW, "Bigger previews"),
    sc(Cmd::Bigger, M::Cmd, Key::Equals, VIEW, "Bigger previews"),
    sc(Cmd::Smaller, M::Cmd, Key::Minus, VIEW, "Smaller previews"),
    sc(Cmd::ResetSize, M::Cmd, Key::Num0, VIEW, "Preview size and text back to normal"),
    sc(Cmd::Sidebar, M::Cmd, Key::B, VIEW, "Show / hide the sidebar"),
    sc(Cmd::Invert, M::CmdShift, Key::I, VIEW, "Swap the preview colours (light on dark)"),
    sc(Cmd::Theme, M::CmdShift, Key::T, VIEW, "Next colour theme"),
    sc(Cmd::Search, M::Cmd, Key::F, LIB, "Search"),
    sc(Cmd::Finder, M::CmdShift, Key::F, LIB, "Font finder (which font is in this picture?)"),
    sc(Cmd::AddFolder, M::Cmd, Key::O, LIB, "Add a font folder…"),
    sc(Cmd::Import, M::Cmd, Key::I, LIB, "Import fonts…"),
    sc(Cmd::NewCollection, M::Cmd, Key::N, LIB, "New collection (with the selected fonts)"),
    sc(Cmd::NewTag, M::Cmd, Key::T, LIB, "New tag (for the selected fonts)"),
    sc(Cmd::Rescan, M::Cmd, Key::R, LIB, "Look through all folders again"),
    sc(Cmd::Rescan, M::None, Key::F5, LIB, "Look through all folders again"),
    sc(Cmd::Settings, M::Cmd, Key::Comma, APP, "Settings"),
    sc(Cmd::Help, M::None, Key::F1, APP, "This list of keyboard shortcuts"),
];

/// Mouse actions, shown with the shortcuts.
fn mouse_hints() -> Vec<(String, &'static str)> {
    vec![
        (format!("{}+{}", key_name(KeyPart::Cmd), click_word()), "Add a font to the selection (or take it out)"),
        (format!("{}+{}", key_name(KeyPart::Shift), click_word()), "Select every font up to this one"),
        ("Double-click".into(), "Switch a font on or off"),
        ("Right-click".into(), "More actions for a font, folder, collection or tag"),
        ("Drag".into(), "Drag fonts onto a collection, tag or Favorites"),
    ]
}

fn click_word() -> &'static str {
    "click"
}

/// The parts of a key combination, named the way this computer's keyboard names them.
#[derive(Clone, Copy)]
pub enum KeyPart {
    Cmd,
    Shift,
    Key(Key),
}

pub fn key_name(k: KeyPart) -> String {
    if cfg!(target_os = "macos") {
        return match k {
            KeyPart::Cmd => "⌘".into(),
            KeyPart::Shift => "⇧".into(),
            KeyPart::Key(Key::Backspace) => "⌫".into(),
            KeyPart::Key(Key::Delete) => "⌦".into(),
            KeyPart::Key(Key::Escape) => "esc".into(),
            KeyPart::Key(Key::Space) => "Space".into(),
            KeyPart::Key(key) => plain(key),
        };
    }
    #[cfg(windows)]
    {
        let vk = match k {
            KeyPart::Cmd => Some((0x11, false)),
            KeyPart::Shift => Some((0x10, false)),
            KeyPart::Key(Key::Delete) => Some((0x2E, true)),
            KeyPart::Key(Key::Backspace) => Some((0x08, false)),
            KeyPart::Key(Key::Space) => Some((0x20, false)),
            KeyPart::Key(Key::Escape) => Some((0x1B, false)),
            KeyPart::Key(Key::Home) => Some((0x24, true)),
            KeyPart::Key(Key::End) => Some((0x23, true)),
            _ => None,
        };
        if let Some(n) = vk.and_then(|(v, ext)| windows_key_name(v, ext)) {
            return n;
        }
    }
    match k {
        KeyPart::Cmd => "Ctrl".into(),
        KeyPart::Shift => "Shift".into(),
        KeyPart::Key(key) => plain(key),
    }
}

fn plain(key: Key) -> String {
    match key {
        Key::ArrowUp => "↑".into(),
        Key::ArrowDown => "↓".into(),
        Key::ArrowLeft => "←".into(),
        Key::ArrowRight => "→".into(),
        Key::Plus => "+".into(),
        Key::Equals => "=".into(),
        Key::Minus => "−".into(),
        Key::Comma => ",".into(),
        Key::Num0 => "0".into(),
        Key::Num1 => "1".into(),
        Key::Num2 => "2".into(),
        Key::Delete => "Delete".into(),
        Key::Backspace => "Backspace".into(),
        Key::Escape => "Esc".into(),
        Key::Space => "Space".into(),
        k => k.name().to_string(),
    }
}

/// The name Windows gives a key on the keyboard layout in use ("Strg", "Umschalt", "Entf"…).
#[cfg(windows)]
fn windows_key_name(vk: u32, extended: bool) -> Option<String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyNameTextW, MapVirtualKeyW, MAPVK_VK_TO_VSC};
    let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) };
    if scan == 0 {
        return None;
    }
    let lparam = ((scan << 16) | if extended { 1 << 24 } else { 0 }) as i32;
    let mut buf = [0u16; 64];
    let n = unsafe { GetKeyNameTextW(lparam, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..n as usize]);
    // Windows writes names in capitals ("STRG"); show them like on the key cap
    let mut c = s.chars();
    let pretty = match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => return None,
    };
    Some(pretty)
}

/// The parts of a shortcut, for drawing as key caps.
pub fn parts(s: &Shortcut) -> Vec<String> {
    let mut v = vec![];
    match s.mods {
        M::None => {}
        M::Cmd => v.push(key_name(KeyPart::Cmd)),
        M::CmdShift => {
            v.push(key_name(KeyPart::Cmd));
            v.push(key_name(KeyPart::Shift));
        }
    }
    v.push(key_name(KeyPart::Key(s.key)));
    v
}

/// The shortcut for a command as text, e.g. "Ctrl+Shift+A" (for menus and tooltips).
pub fn label(cmd: Cmd) -> String {
    let Some(s) = ALL.iter().find(|s| s.cmd == cmd) else { return String::new() };
    let sep = if cfg!(target_os = "macos") { "" } else { "+" };
    parts(s).join(sep)
}

fn modifiers(m: M) -> egui::Modifiers {
    match m {
        M::None => egui::Modifiers::NONE,
        M::Cmd => egui::Modifiers::COMMAND,
        M::CmdShift => egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    }
}

/// The commands whose keys were pressed this frame (each key press is used up, so a shortcut
/// with Shift isn't also taken as the one without).
pub fn pressed(ctx: &egui::Context, typing: bool) -> Vec<Cmd> {
    let mut out = vec![];
    ctx.input_mut(|i| {
        for s in ALL {
            // while typing in a text field only the shortcuts that can't mean typing work
            let ok_while_typing = matches!(s.cmd, Cmd::Search | Cmd::Settings | Cmd::Help | Cmd::Finder);
            if typing && !ok_while_typing {
                continue;
            }
            // plain keys mustn't fire together with Ctrl/⌘ (Ctrl+Space, Ctrl+Delete…)
            if s.mods == M::None && (i.modifiers.command || i.modifiers.alt) {
                continue;
            }
            if s.mods == M::Cmd && i.modifiers.shift {
                continue;
            }
            if i.consume_key(modifiers(s.mods), s.key) && !out.contains(&s.cmd) {
                out.push(s.cmd);
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No two commands share the same keys, and every command has a description and key names.
    #[test]
    fn shortcuts_are_unique_and_named() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert!(!(a.mods == b.mods && a.key == b.key), "{:?} and {:?} use the same keys", a.cmd, b.cmd);
            }
            assert!(!a.what.is_empty());
            assert!(parts(a).iter().all(|p| !p.is_empty()), "{:?}", a.cmd);
        }
        assert!(label(Cmd::Activate).ends_with('A'));
        assert!(label(Cmd::Deactivate).ends_with('D'));
    }
}

/// One key cap.
fn keycap(ui: &mut Ui, p: &Pal, text: &str) {
    let font = FontId::proportional(12.0);
    let g = ui.painter().layout_no_wrap(text.to_string(), font, p.text);
    let w = (g.size().x + 14.0).max(24.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    ui.painter().rect(r, CornerRadius::same(5), p.input, Stroke::new(1.0_f32, p.line), egui::StrokeKind::Inside);
    ui.painter().hline(r.x_range().shrink(3.0), r.bottom() - 1.0, Stroke::new(1.5_f32, p.line));
    ui.painter().galley(r.center() - g.size() / 2.0 - vec2(0.0, 1.0), g, p.text);
}

impl FontCraftApp {
    /// Settings › Keyboard shortcuts.
    pub(super) fn settings_shortcuts(&mut self, ui: &mut Ui, p: &Pal) {
        let os = if cfg!(target_os = "macos") {
            "Mac"
        } else if cfg!(windows) {
            "Windows"
        } else {
            "Linux"
        };
        ui.label(
            RichText::new(format!(
                "The keys shown are the ones of your {os} keyboard. Shortcuts for fonts work on the selected fonts \
                 (click a font, or select several). While you type in a text field only search, the Font finder, \
                 settings and this list react."
            ))
            .size(12.0)
            .color(p.weak),
        );
        let mut groups: Vec<&str> = vec![];
        for s in ALL {
            if !groups.contains(&s.group) {
                groups.push(s.group);
            }
        }
        let w = ui.available_width();
        for g in groups {
            ui.add_space(14.0);
            ui.label(RichText::new(g.to_uppercase()).size(11.0).strong().color(p.weak));
            ui.add_space(4.0);
            let mut done: Vec<Cmd> = vec![];
            for s in ALL.iter().filter(|s| s.group == g) {
                if done.contains(&s.cmd) {
                    continue;
                }
                done.push(s.cmd);
                let alts: Vec<&Shortcut> = ALL.iter().filter(|x| x.cmd == s.cmd).collect();
                self.shortcut_row(ui, p, w, s.what, alts.iter().map(|a| parts(a)).collect());
            }
        }
        ui.add_space(14.0);
        ui.label(RichText::new("MOUSE").size(11.0).strong().color(p.weak));
        ui.add_space(4.0);
        for (k, what) in mouse_hints() {
            self.shortcut_row(ui, p, w, what, vec![vec![k]]);
        }
    }

    fn shortcut_row(&self, ui: &mut Ui, p: &Pal, w: f32, what: &str, keys: Vec<Vec<String>>) {
        let (r, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(5), p.hover);
        }
        ui.painter().text(egui::pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, what, FontId::proportional(13.0), p.text);
        // key caps on the right
        let right = Rect::from_min_max(egui::pos2(r.left() + w * 0.55, r.top()), egui::pos2(r.right() - 6.0, r.bottom()));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(right).layout(egui::Layout::right_to_left(egui::Align::Center)));
        child.spacing_mut().item_spacing.x = 4.0;
        for (k, combo) in keys.iter().enumerate().rev() {
            for (j, part) in combo.iter().enumerate().rev() {
                keycap(&mut child, p, part);
                if j > 0 && !cfg!(target_os = "macos") {
                    child.label(RichText::new("+").size(11.0).color(p.weak));
                }
            }
            if k > 0 {
                child.label(RichText::new("or").size(11.5).color(p.weak));
            }
        }
    }
}
