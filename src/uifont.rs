//! The interface font.
//!
//! FontCraft doesn't ship any font files. The window uses the computer's own interface
//! font (Segoe UI on Windows, San Francisco / Helvetica on macOS, the desktop's sans-serif
//! on Linux) plus a symbol font for the few icons drawn as characters.

use egui::{FontData, FontDefinitions, FontFamily};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn linux_match(pattern: &str) -> Option<PathBuf> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{file}", pattern]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

/// (interface fonts, symbol fonts, monospace fonts) to try, best first.
fn candidates() -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let mut ui = vec![];
    let mut sym = vec![];
    let mut mono = vec![];
    #[cfg(windows)]
    {
        let fonts = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("Fonts");
        ui.extend(["segoeui.ttf", "tahoma.ttf", "arial.ttf"].iter().map(|f| fonts.join(f)));
        sym.extend(["seguisym.ttf", "segoeui.ttf", "arial.ttf"].iter().map(|f| fonts.join(f)));
        mono.extend(["consola.ttf", "cour.ttf", "lucon.ttf"].iter().map(|f| fonts.join(f)));
    }
    #[cfg(target_os = "macos")]
    {
        let s = PathBuf::from("/System/Library/Fonts");
        ui.extend([s.join("SFNS.ttf"), s.join("Helvetica.ttc"), s.join("Supplemental/Arial.ttf"), s.join("Geneva.ttf")]);
        sym.extend([s.join("Apple Symbols.ttf"), s.join("ZapfDingbats.ttf"), s.join("Supplemental/Arial Unicode.ttf")]);
        mono.extend([s.join("SFNSMono.ttf"), s.join("Menlo.ttc"), s.join("Monaco.ttf")]);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ui.extend(linux_match("sans-serif"));
        mono.extend(linux_match("monospace"));
        sym.extend(linux_match("sans-serif:charset=2764 2716 2714"));
        for d in ["/usr/share/fonts/truetype/dejavu", "/usr/share/fonts/TTF", "/usr/share/fonts/dejavu", "/usr/share/fonts/dejavu-sans-fonts"] {
            ui.push(Path::new(d).join("DejaVuSans.ttf"));
            sym.push(Path::new(d).join("DejaVuSans.ttf"));
            mono.push(Path::new(d).join("DejaVuSansMono.ttf"));
        }
        ui.push(PathBuf::from("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"));
        ui.push(PathBuf::from("/usr/share/fonts/noto/NotoSans-Regular.ttf"));
    }
    let _ = linux_match;
    (ui, sym, mono)
}

fn load(path: &Path) -> Option<FontData> {
    let bytes = std::fs::read(path).ok()?;
    ab_glyph::FontRef::try_from_slice_and_index(&bytes, 0).ok()?;
    Some(FontData::from_owned(bytes))
}

/// Any working font from the system font folders (last resort).
fn any_system_font() -> Option<FontData> {
    for dir in crate::library::system_font_dirs() {
        for e in walkdir::WalkDir::new(dir).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            let p = e.path();
            if p.extension().map(|x| x.eq_ignore_ascii_case("ttf")).unwrap_or(false) {
                if let Some(f) = load(p) {
                    return Some(f);
                }
            }
        }
    }
    None
}

/// Font set for the window, built from the computer's own fonts.
pub fn definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::empty();
    let (ui, sym, mono) = candidates();
    let first = |list: &[PathBuf]| list.iter().find_map(|p| load(p).map(|f| (p.clone(), f)));
    let main = first(&ui).map(|(_, f)| f).or_else(any_system_font);
    let mut prop: Vec<String> = vec![];
    if let Some(f) = main {
        defs.font_data.insert("ui".into(), Arc::new(f));
        prop.push("ui".into());
    }
    // every available symbol font, for icons the main font lacks
    for (i, p) in sym.iter().enumerate() {
        if let Some(f) = load(p) {
            let k = format!("sym{i}");
            defs.font_data.insert(k.clone(), Arc::new(f));
            prop.push(k);
        }
    }
    let mut monos: Vec<String> = vec![];
    if let Some((_, f)) = first(&mono) {
        defs.font_data.insert("mono".into(), Arc::new(f));
        monos.push("mono".into());
    }
    monos.extend(prop.iter().cloned());
    defs.families.insert(FontFamily::Proportional, prop);
    defs.families.insert(FontFamily::Monospace, monos);
    defs
}

#[cfg(test)]
mod tests {
    #[test]
    fn interface_font_covers_the_symbols_we_use() {
        let defs = super::definitions();
        let fams = &defs.families[&egui::FontFamily::Proportional];
        assert!(!fams.is_empty(), "no interface font found");
        let faces: Vec<ab_glyph::FontRef> = fams
            .iter()
            .filter_map(|k| ab_glyph::FontRef::try_from_slice(defs.font_data[k].font.as_ref()).ok())
            .collect();
        let missing: Vec<char> = "❤✖✔➕▼▲⏵…–“”·›•€äöüß⚠—"
            .chars()
            .filter(|c| !faces.iter().any(|f| ab_glyph::Font::glyph_id(f, *c).0 != 0))
            .collect();
        assert!(missing.is_empty(), "missing symbols: {missing:?}");
    }
}
