//! "Clean Application Font Caches…": find and delete the font information other programs keep
//! for themselves (Adobe apps, Microsoft Office, QuarkXPress, and the user's own system font
//! cache on macOS and Linux). Only caches are touched – never font files, and never Adobe Fonts'
//! own activated fonts. The programs rebuild their caches the next time they start.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum CacheApp {
    Adobe,
    Office,
    Quark,
    /// macOS: the user's font database (atsutil); Linux: fontconfig's cache
    System,
}

impl CacheApp {
    pub fn all() -> Vec<CacheApp> {
        let mut v = vec![CacheApp::Adobe, CacheApp::Office, CacheApp::Quark];
        if cfg!(any(target_os = "macos", all(unix, not(target_os = "macos")))) {
            v.push(CacheApp::System);
        }
        v
    }
    pub fn label(self) -> &'static str {
        match self {
            CacheApp::Adobe => "Adobe apps",
            CacheApp::Office => "Microsoft Office",
            CacheApp::Quark => "QuarkXPress",
            CacheApp::System => {
                if cfg!(target_os = "macos") {
                    "macOS font cache (your user)"
                } else {
                    "Fontconfig cache (your user)"
                }
            }
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            CacheApp::Adobe => "Photoshop, Illustrator, InDesign, Acrobat… – their font lists and font caches. Adobe Fonts, documents and recovery data stay as they are.",
            CacheApp::Office => "Word, Excel, PowerPoint, Outlook: the Office font cache, with the cloud fonts Office downloaded (fetched again when used)",
            CacheApp::Quark => "QuarkXPress's font cache",
            CacheApp::System => {
                if cfg!(target_os = "macos") {
                    "The font database macOS keeps for you; rebuilt after you log out and in again"
                } else {
                    "~/.cache/fontconfig; rebuilt right away"
                }
            }
        }
    }
    /// Program names (as in the process list) that should be closed before cleaning.
    fn processes(self) -> &'static [(&'static str, &'static str)] {
        match self {
            CacheApp::Adobe => &[
                ("photoshop", "Photoshop"),
                ("illustrator", "Illustrator"),
                ("indesign", "InDesign"),
                ("acrobat", "Acrobat"),
                ("acrord32", "Acrobat Reader"),
                ("afterfx", "After Effects"),
                ("adobe premiere pro", "Premiere Pro"),
                ("incopy", "InCopy"),
            ],
            CacheApp::Office => &[("winword", "Word"), ("excel", "Excel"), ("powerpnt", "PowerPoint"), ("outlook", "Outlook"), ("microsoft word", "Word"), ("microsoft excel", "Excel"), ("microsoft powerpoint", "PowerPoint"), ("microsoft outlook", "Outlook")],
            CacheApp::Quark => &[("quarkxpress", "QuarkXPress")],
            CacheApp::System => &[],
        }
    }
}

/// What was found for one program: the cache files and folders and their size together.
#[derive(Clone, Debug)]
pub struct Found {
    pub app: CacheApp,
    pub paths: Vec<PathBuf>,
    pub bytes: u64,
    /// programs of this kind that are open now (their caches are in use)
    pub running: Vec<String>,
}

fn env_dir(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).map(PathBuf::from).filter(|p| p.is_dir())
}

fn home() -> Option<PathBuf> {
    env_dir("HOME").or_else(|| env_dir("USERPROFILE"))
}

/// Everything below `root` (at most `depth` folders deep) for which `pick` says yes; a picked
/// folder is taken whole and not looked into.
fn walk(root: &Path, depth: usize, pick: &dyn Fn(&Path, bool) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        // Adobe Fonts (activated through Creative Cloud) live in CoreSync: never touched
        let s = p.to_string_lossy().to_lowercase();
        if s.contains("coresync") || s.contains("livetype") {
            continue;
        }
        if pick(&p, ft.is_dir()) {
            out.push(p);
        } else if ft.is_dir() && depth > 0 {
            walk(&p, depth - 1, pick, out);
        }
    }
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// The cache files and folders of a program on this computer.
pub fn cache_paths(app: CacheApp) -> Vec<PathBuf> {
    let mut out = vec![];
    let local = env_dir("LOCALAPPDATA");
    let roaming = env_dir("APPDATA");
    let lib = home().map(|h| h.join("Library")).filter(|p| p.is_dir() && cfg!(target_os = "macos"));
    match app {
        CacheApp::Adobe => {
            // AdobeFnt*.lst font lists (all Adobe apps), Photoshop's "CT Font Cache", InDesign's caches
            let pick = |p: &Path, dir: bool| {
                let n = name_of(p);
                (!dir && n.starts_with("adobefnt") && n.ends_with(".lst")) || (dir && n == "ct font cache")
            };
            let mut roots: Vec<PathBuf> = vec![];
            roots.extend(roaming.iter().map(|r| r.join("Adobe")));
            roots.extend(local.iter().map(|r| r.join("Adobe")));
            if let Some(l) = &lib {
                roots.push(l.join("Application Support").join("Adobe"));
                roots.push(l.join("Preferences").join("Adobe"));
                roots.push(l.join("Caches").join("Adobe"));
            }
            for r in roots.iter().filter(|r| r.is_dir()) {
                walk(r, 7, &pick, &mut out);
            }
            // InDesign keeps its font caches among other things in …\InDesign\Version X\<language>\Caches
            // (Windows) and ~/Library/Caches/Adobe InDesign/Version X/<language> (Mac). Only the font
            // parts are taken: never "InDesign Recovery" (unsaved documents) or saved data.
            let font_part = |p: &Path, _dir: bool| {
                let n = name_of(p);
                matches!(n.as_str(), "typequestcache" | "fontmaskcache" | "typesupport" | "fontclassificationcache.txt" | "adobefontlist.lst")
                    || n.starts_with("df_sim_")
            };
            let mut id_caches: Vec<PathBuf> = vec![];
            if let Some(l) = &local {
                walk(&l.join("Adobe").join("InDesign"), 3, &|p: &Path, dir: bool| dir && name_of(p) == "caches", &mut id_caches);
            }
            if let Some(l) = &lib {
                let c = l.join("Caches").join("Adobe InDesign");
                if c.is_dir() {
                    id_caches.push(c);
                }
                let ts = l.join("Caches").join("Adobe").join("TypeSupport");
                if ts.is_dir() {
                    out.push(ts);
                }
            }
            for c in id_caches {
                walk(&c, 3, &|p: &Path, dir: bool| font_part(p, dir) && !p.to_string_lossy().to_lowercase().contains("recovery"), &mut out);
            }
        }        CacheApp::Office => {
            if let Some(l) = &local {
                out.push(l.join("Microsoft").join("FontCache"));
            }
            if let Some(h) = home().filter(|_| cfg!(target_os = "macos")) {
                out.push(h.join("Library").join("Group Containers").join("UBF8T346G9.Office").join("FontCache"));
            }
            out.retain(|p| p.exists());
        }
        CacheApp::Quark => {
            let pick = |p: &Path, _dir: bool| {
                let n = name_of(p).replace([' ', '_', '-', '.'], "");
                n.contains("font") && n.contains("cache")
            };
            let mut roots: Vec<PathBuf> = vec![];
            roots.extend(roaming.iter().map(|r| r.join("Quark")));
            roots.extend(local.iter().map(|r| r.join("Quark")));
            if let Some(l) = &lib {
                roots.push(l.join("Caches").join("Quark"));
                roots.push(l.join("Preferences").join("Quark"));
                roots.push(l.join("Application Support").join("Quark"));
            }
            for r in roots.iter().filter(|r| r.is_dir()) {
                walk(r, 5, &pick, &mut out);
            }
        }
        CacheApp::System => {
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                let c = env_dir("XDG_CACHE_HOME").or_else(|| home().map(|h| h.join(".cache"))).map(|c| c.join("fontconfig"));
                out.extend(c.filter(|c| c.is_dir()));
            }
        }
    }
    out.sort();
    out.dedup();
    // a path inside another one that is deleted anyway isn't listed twice
    let all = out.clone();
    out.retain(|p| !all.iter().any(|o| o != p && p.starts_with(o)));
    out
}

fn size_of(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| size_of(&e.path())).sum()).unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// The names of running programs (lower case).
fn process_names() -> Vec<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("tasklist").args(["/fo", "csv", "/nh"]).creation_flags(0x0800_0000).output();
        return out
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| l.split("\",\"").next().map(|n| n.trim_matches('"').to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();
    }
    #[cfg(not(windows))]
    {
        let out = std::process::Command::new("ps").args(["-axo", "comm="]).output();
        out.map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| Path::new(l.trim()).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
    }
}

/// Look for the caches of every program (also the ones with nothing found, so the window can say so).
pub fn find_all() -> Vec<Found> {
    let procs = process_names();
    CacheApp::all()
        .into_iter()
        .map(|app| {
            let paths = cache_paths(app);
            let bytes = paths.iter().map(|p| size_of(p)).sum();
            let mut running: Vec<String> = app
                .processes()
                .iter()
                .filter(|(exe, _)| procs.iter().any(|p| p.starts_with(exe)))
                .map(|(_, n)| n.to_string())
                .collect();
            running.dedup();
            Found { app, paths, bytes, running }
        })
        .collect()
}

/// Delete one program's caches. Returns how many were removed and the problems (a cache in use…).
pub fn clean(found: &Found) -> (usize, Vec<String>) {
    let mut done = 0;
    let mut errors = vec![];
    for p in &found.paths {
        let r = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
        match r {
            Ok(()) => done += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => done += 1,
            Err(e) => errors.push(format!("{}: {e}", p.display())),
        }
    }
    if found.app == CacheApp::System {
        #[cfg(target_os = "macos")]
        {
            match std::process::Command::new("atsutil").args(["databases", "-removeUser"]).status() {
                Ok(s) if s.success() => done += 1,
                Ok(_) | Err(_) => errors.push("atsutil could not clear the font database".into()),
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            // rebuild it now, so programs started next find their fonts quickly
            let _ = std::process::Command::new("fc-cache").arg("-f").status();
        }
    }
    (done, errors)
}

pub fn human(b: u64) -> String {
    if b >= 1_000_000 {
        format!("{:.1} MB", b as f64 / 1_000_000.0)
    } else if b >= 1000 {
        format!("{:.0} KB", b as f64 / 1000.0)
    } else if b > 0 {
        format!("{b} bytes")
    } else {
        "empty".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lists what would be cleaned on this computer (nothing is deleted): cargo test list_caches -- --ignored --nocapture
    #[test]
    #[ignore]
    fn list_caches() {
        for app in CacheApp::all() {
            for p in cache_paths(app) {
                eprintln!("{:?}\t{}\t{}", app, human(size_of(&p)), p.display());
            }
        }
    }

    /// Font lists and CT Font Cache folders are found; other Adobe files and Adobe Fonts are not.
    #[test]
    fn finds_only_caches() {
        let root = std::env::temp_dir().join(format!("fontcraft-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let ps = root.join("Adobe").join("Adobe Photoshop 2025").join("CT Font Cache");
        std::fs::create_dir_all(&ps).unwrap();
        std::fs::write(ps.join("a.bin"), b"1234").unwrap();
        let ai = root.join("Adobe").join("Adobe Illustrator 28 Settings").join("en_US");
        std::fs::create_dir_all(&ai).unwrap();
        std::fs::write(ai.join("AdobeFnt22.lst"), b"fonts").unwrap();
        std::fs::write(ai.join("Adobe Illustrator Prefs"), b"keep").unwrap();
        let cs = root.join("Adobe").join("CoreSync").join("plugins").join("livetype");
        std::fs::create_dir_all(&cs).unwrap();
        std::fs::write(cs.join("AdobeFnt99.lst"), b"keep").unwrap();
        let mut out = vec![];
        let pick = |p: &Path, dir: bool| {
            let n = name_of(p);
            (!dir && n.starts_with("adobefnt") && n.ends_with(".lst")) || (dir && n == "ct font cache")
        };
        walk(&root.join("Adobe"), 7, &pick, &mut out);
        out.sort();
        let mut want = vec![ps.clone(), ai.join("AdobeFnt22.lst")];
        want.sort();
        assert_eq!(out, want);
        let f = Found { app: CacheApp::Adobe, bytes: out.iter().map(|p| size_of(p)).sum(), paths: out, running: vec![] };
        assert_eq!(f.bytes, 9);
        let (n, errs) = clean(&f);
        assert_eq!((n, errs.len()), (2, 0));
        assert!(!ps.exists() && ai.join("Adobe Illustrator Prefs").exists() && cs.join("AdobeFnt99.lst").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
