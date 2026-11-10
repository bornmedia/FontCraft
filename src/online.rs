//! The other free font sources next to Google Fonts: Fontshare (the Indian Type Foundry's
//! free library) and Font Squirrel (hand-picked fonts that are free for commercial use).
//!
//! Files are kept in FontCraft's data folder:
//!   online/<source>/catalog.json        the list of families (refreshed once a week)
//!   online/<source>/fonts/<Family>/...  complete families that were added to the library
//!   google/previews/<source>_<Family>.ttf  one style per family, for previews
//!
//! Font Squirrel answers only about one automatic request every few minutes (then its
//! site shows a robot check). So FontCraft only fetches its list automatically (once a
//! week); families are downloaded by the person's browser and picked up from the
//! Downloads folder, and there are no previews before a family is in the library.

use crate::google::{curl, download, safe, GFamily, Provider};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

const SQUIRREL_WAIT: Duration = Duration::from_secs(5 * 60);
static SQUIRREL_BLOCKED: Mutex<Option<Instant>> = Mutex::new(None);

/// Whether Font Squirrel recently asked us to slow down (previews wait until then).
pub fn squirrel_blocked() -> bool {
    SQUIRREL_BLOCKED.lock().map(|b| b.map(|t| t.elapsed() < SQUIRREL_WAIT).unwrap_or(false)).unwrap_or(false)
}
fn set_squirrel_blocked() {
    if let Ok(mut b) = SQUIRREL_BLOCKED.lock() {
        *b = Some(Instant::now());
    }
}

fn root(p: Provider) -> PathBuf {
    crate::config::data_dir().join("online").join(p.tag())
}

const BLOCKED_MSG: &str = "Font Squirrel asked FontCraft to slow down. Please try again in a few minutes.";

/// Download into `dest`; tells a robot check (an empty "202" answer) apart from other failures.
fn fetch(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("part");
    let out = curl()
        .args(["-sSL", "--fail", "--max-time", "300", "-w", "%{http_code}", "-o"])
        .arg(&tmp)
        .arg(url)
        .output()
        .map_err(|_| "could not start curl (needed for downloading fonts)".to_string())?;
    let code = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let size = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if !out.status.success() || code != "200" || size == 0 {
        let _ = std::fs::remove_file(&tmp);
        if code == "202" || code == "403" || code == "405" {
            if url.contains("fontsquirrel") {
                set_squirrel_blocked();
                return Err(BLOCKED_MSG.into());
            }
        }
        return Err(if out.status.success() || code.is_empty() { "download failed – are you online?".into() } else { format!("download failed (error {code})") });
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ catalogues

fn catalog_url(p: Provider) -> &'static str {
    match p {
        Provider::Fontshare => "https://api.fontshare.com/v2/fonts?limit=1000",
        _ => "https://www.fontsquirrel.com/api/fontlist/all",
    }
}

/// Load a catalogue from disk; fetch it when missing or older than a week.
pub fn load_catalog(p: Provider, allow_network: bool) -> Result<Vec<GFamily>, String> {
    let path = root(p).join("catalog.json");
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|age| age < Duration::from_secs(7 * 24 * 3600))
        .unwrap_or(false);
    if !fresh && allow_network {
        let new = path.with_extension("new");
        match fetch(catalog_url(p), &new) {
            // only replace the old list with one that can be read
            Ok(()) => {
                let ok = std::fs::read_to_string(&new).map(|t| !parse_catalog(p, &t).is_empty()).unwrap_or(false);
                if ok {
                    let _ = std::fs::rename(&new, &path);
                } else {
                    let _ = std::fs::remove_file(&new);
                }
            }
            Err(e) if !path.exists() => return Err(e),
            Err(_) => {}
        }
    }
    let text = std::fs::read_to_string(&path).map_err(|_| format!("The {} list has not been downloaded yet.", p.label()))?;
    let list = parse_catalog(p, &text);
    if list.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Err(format!("The {} list could not be read. Try again later.", p.label()));
    }
    Ok(list)
}

pub fn parse_catalog(p: Provider, text: &str) -> Vec<GFamily> {
    let mut out = match p {
        Provider::Fontshare => parse_fontshare(text),
        _ => parse_squirrel(text),
    };
    out.sort_by(|a, b| a.lower.cmp(&b.lower));
    out.dedup_by(|a, b| a.lower == b.lower);
    out
}

/// The provider's classification turned into Google's five categories, for the filter chips.
fn category_of(kind: &str) -> &'static str {
    let k = kind.to_lowercase();
    let first = k.split(',').next().unwrap_or("").trim().to_string();
    let has = |w: &str| first.contains(w);
    if has("mono") || has("typewriter") || has("programming") {
        "Monospace"
    } else if has("script") || has("hand") || has("calligraph") || has("comic") {
        "Handwriting"
    } else if has("sans") {
        "Sans Serif"
    } else if has("serif") || has("slab") {
        "Serif"
    } else {
        "Display"
    }
}

fn base_family(p: Provider, name: String, slug: String, kind: String) -> GFamily {
    GFamily {
        provider: p,
        lower: name.to_lowercase(),
        name,
        category: category_of(&kind).to_string(),
        kind,
        designers: vec![],
        styles: vec![],
        style_names: vec![],
        nstyles: 1,
        italic: false,
        axes: vec![],
        popularity: 99_999,
        added: String::new(),
        thickness: None,
        width: None,
        slug,
        preview_src: String::new(),
        license: String::new(),
    }
}

fn parse_fontshare(text: &str) -> Vec<GFamily> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return vec![] };
    let Some(list) = v.get("fonts").and_then(|f| f.as_array()) else { return vec![] };
    let s = |x: &Value, k: &str| x.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut out: Vec<(u64, GFamily)> = list
        .iter()
        .filter_map(|f| {
            let name = f.get("name")?.as_str()?.trim().to_string();
            let slug = f.get("slug")?.as_str()?.to_string();
            let mut g = base_family(Provider::Fontshare, name, slug, tidy_kind(&s(f, "category")));
            g.designers = f
                .get("designers")
                .and_then(|d| d.as_array())
                .map(|d| d.iter().filter_map(|x| x.get("name").and_then(|n| n.as_str()).map(String::from)).collect())
                .unwrap_or_default();
            g.added = s(f, "inserted_at").chars().take(10).collect();
            g.license = match s(f, "license_type").as_str() {
                "sil_ofl" => "SIL Open Font License".into(),
                "itf_ffl" => "ITF Free Font License (free for personal and commercial use)".into(),
                other => other.to_string(),
            };
            let styles = f.get("styles").and_then(|s| s.as_array()).cloned().unwrap_or_default();
            let mut statics: Vec<(u16, bool, String, String)> = vec![];
            let (mut wmin, mut wmax, mut variable) = (1000u16, 0u16, false);
            for st in &styles {
                let w = st.pointer("/weight/weight").or_else(|| st.pointer("/weight/number")).and_then(|w| w.as_u64()).unwrap_or(400) as u16;
                let italic = st.get("is_italic").and_then(|i| i.as_bool()).unwrap_or(false);
                let label = st.pointer("/weight/label").and_then(|l| l.as_str()).unwrap_or("Regular").to_string();
                let file = s(st, "file");
                if st.get("is_variable").and_then(|i| i.as_bool()).unwrap_or(false) {
                    variable = true;
                    continue;
                }
                wmin = wmin.min(w);
                wmax = wmax.max(w);
                statics.push((w, italic, label, file));
            }
            statics.sort_by_key(|(w, i, _, _)| (*i, *w));
            if variable && wmax > wmin {
                g.axes = vec![("wght".into(), wmin as f32, wmax as f32)];
            }
            g.styles = statics.iter().map(|(w, i, _, _)| format!("{w}{}", if *i { "i" } else { "" })).collect();
            g.style_names = statics
                .iter()
                .map(|(_, i, l, _)| if *i && !l.to_lowercase().contains("italic") { format!("{l} Italic") } else { l.clone() })
                .collect();
            g.nstyles = statics.len().max(1);
            g.italic = statics.iter().any(|s| s.1);
            // the regular style (or the one closest to it) for previews
            let pick = statics.iter().min_by_key(|(w, i, _, _)| (*i, (*w as i32 - 400).abs()));
            g.preview_src = pick
                .map(|(_, _, _, f)| f.clone())
                .filter(|f| !f.is_empty())
                .map(|f| format!("{}{f}.ttf", if f.starts_with("//") { "https:" } else { "" }))
                .unwrap_or_default();
            let views = f.get("views").and_then(|v| v.as_u64()).unwrap_or(0);
            Some((views, g))
        })
        .collect();
    // most viewed first = most popular
    out.sort_by_key(|(v, _)| std::cmp::Reverse(*v));
    out.into_iter()
        .enumerate()
        .map(|(i, (_, mut g))| {
            g.popularity = i as u32 + 1;
            g
        })
        .collect()
}

fn parse_squirrel(text: &str) -> Vec<GFamily> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return vec![] };
    let Some(list) = v.as_array() else { return vec![] };
    let s = |x: &Value, k: &str| x.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    list.iter()
        .filter_map(|f| {
            let name = s(f, "family_name");
            let slug = s(f, "family_urlname");
            if name.is_empty() || slug.is_empty() {
                return None;
            }
            let mut g = base_family(Provider::FontSquirrel, name, slug, s(f, "classification"));
            let foundry = s(f, "foundry_name");
            if !foundry.is_empty() {
                g.designers = vec![foundry];
            }
            g.nstyles = s(f, "family_count").parse().unwrap_or(1).max(1);
            g.preview_src = s(f, "font_filename");
            g.license = "Free for commercial use (see the license file that comes with it)".into();
            Some(g)
        })
        .collect()
}

// ------------------------------------------------------------------ files

fn download_url(g: &GFamily) -> String {
    match g.provider {
        Provider::Fontshare => format!("https://api.fontshare.com/v2/fonts/download/{}", g.slug),
        _ => format!("https://www.fontsquirrel.com/fonts/download/{}", g.slug),
    }
}

fn zip_path(g: &GFamily) -> PathBuf {
    root(g.provider).join("zips").join(format!("{}.zip", safe(&g.slug)))
}

/// The font files in a family download worth keeping: desktop files rather than web
/// copies, OpenType (.otf) when both kinds are there, and static styles rather than a
/// variable file (having both would show every style twice in Word).
fn pick_fonts(names: &[String]) -> Vec<String> {
    let low = |s: &String| s.to_lowercase().replace('\\', "/");
    let base = |s: &String| low(s).rsplit('/').next().unwrap_or("").to_string();
    let mut v: Vec<String> = names
        .iter()
        .filter(|n| {
            let l = low(n);
            let b = base(n);
            !l.contains("__macosx") && !b.starts_with('.') && (b.ends_with(".otf") || b.ends_with(".ttf") || b.ends_with(".ttc"))
        })
        .cloned()
        .collect();
    let is_web = |n: &String| low(n).split('/').any(|p| p == "web" || p == "webfonts" || p == "webfont");
    if v.iter().any(|n| !is_web(n)) {
        v.retain(|n| !is_web(n));
    }
    if v.iter().any(|n| base(n).ends_with(".otf")) {
        v.retain(|n| base(n).ends_with(".otf"));
    }
    let is_var = |n: &String| {
        let b = base(n);
        b.contains("variable") || b.contains('[') || low(n).split('/').any(|p| p == "variable")
    };
    if v.iter().any(|n| !is_var(n)) {
        v.retain(|n| !is_var(n));
    }
    // one file per name
    let mut seen = std::collections::HashSet::new();
    v.retain(|n| seen.insert(base(n)));
    v
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<Vec<u8>, String> {
    let mut e = zip.by_name(name).map_err(|e| e.to_string())?;
    let mut data = vec![];
    e.read_to_end(&mut data).map_err(|e| e.to_string())?;
    Ok(data)
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<std::fs::File>, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    zip::ZipArchive::new(f).map_err(|_| "the download was not a font package".to_string())
}

fn entry_names(zip: &zip::ZipArchive<std::fs::File>) -> Vec<String> {
    zip.file_names().map(String::from).collect()
}

/// A preview file for a family. Only Fontshare has single font files online; Font Squirrel
/// is left out (it allows too few automatic requests for browsing).
pub fn fetch_preview(g: &GFamily, dest: &Path) -> Result<PathBuf, String> {
    if g.provider != Provider::Fontshare || g.preview_src.is_empty() {
        return Err("no preview file".into());
    }
    download(&g.preview_src, dest)?;
    Ok(dest.to_path_buf())
}

/// "Serif,  Blackletter, Display" -> "Serif, Blackletter, Display"
fn tidy_kind(s: &str) -> String {
    s.split(',').map(|p| p.trim()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ")
}

/// Download a whole family into the library folder; returns its font files.
pub fn download_family(g: &GFamily) -> Result<Vec<PathBuf>, String> {
    let zp = zip_path(g);
    fetch(&download_url(g), &zp)?;
    let r = extract_family(g, &zp);
    let _ = std::fs::remove_file(&zp);
    r
}

pub fn is_blocked_error(e: &str) -> bool {
    e == BLOCKED_MSG
}

/// The address that downloads the family in a web browser.
pub fn browser_url(g: &GFamily) -> String {
    download_url(g)
}

/// The person's Downloads folder (where Windows, or the Linux desktop, really keeps it).
pub fn downloads_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(p) = windows_downloads_dir() {
        return Some(p);
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)?;
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // a renamed or translated Downloads folder is written down here
        if let Ok(t) = std::fs::read_to_string(home.join(".config/user-dirs.dirs")) {
            for line in t.lines() {
                if let Some(v) = line.strip_prefix("XDG_DOWNLOAD_DIR=") {
                    let v = v.trim().trim_matches('"').replace("$HOME", &home.to_string_lossy());
                    return Some(PathBuf::from(v));
                }
            }
        }
    }
    Some(home.join("Downloads"))
}

/// Windows lets people move Downloads (to another drive, into OneDrive...): ask Windows.
#[cfg(windows)]
fn windows_downloads_dir() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath};
    unsafe {
        let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(&FOLDERID_Downloads, 0, std::ptr::null_mut(), &mut p);
        if p.is_null() {
            return None;
        }
        let path = if hr == 0 {
            let mut n = 0;
            while *p.add(n) != 0 {
                n += 1;
            }
            Some(PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(p, n))))
        } else {
            None
        };
        CoTaskMemFree(p as _);
        path.filter(|d| d.is_dir())
    }
}

/// After the family was opened in the browser: wait (up to 15 minutes) for its package to
/// appear in the Downloads folder, then add it to the library like a direct download.
pub fn wait_for_browser_download(g: &GFamily, since: SystemTime) -> Result<Vec<PathBuf>, String> {
    let dir = downloads_dir().ok_or_else(|| "the Downloads folder was not found".to_string())?;
    let started = Instant::now();
    let mut tried: std::collections::HashMap<PathBuf, SystemTime> = std::collections::HashMap::new();
    while started.elapsed() < Duration::from_secs(15 * 60) {
        std::thread::sleep(Duration::from_millis(1500));
        if let Some(path) = find_family_zip(&dir, g, since, &mut tried) {
            // the package stays in Downloads: it is the person's file
            return extract_family(g, &path);
        }
    }
    Err(format!("the download from the browser did not arrive in {} within 15 minutes", dir.display()))
}

/// One look through `dir` for a finished .zip of family `g` that arrived after `since`.
/// A package still being written (unreadable) is looked at again once it changes.
fn find_family_zip(
    dir: &Path,
    g: &GFamily,
    since: SystemTime,
    tried: &mut std::collections::HashMap<PathBuf, SystemTime>,
) -> Option<PathBuf> {
    let want = g.preview_src.to_lowercase();
    let slug = g.slug.to_lowercase();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let path = e.path();
        if !path.extension().map(|x| x.eq_ignore_ascii_case("zip")).unwrap_or(false) {
            continue;
        }
        let Ok(modified) = e.metadata().and_then(|m| m.modified()) else { continue };
        if modified + Duration::from_secs(2) < since || tried.get(&path) == Some(&modified) {
            continue;
        }
        tried.insert(path.clone(), modified);
        let fname = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let Ok(zip) = open_zip(&path) else { continue };
        let names = entry_names(&zip);
        if fname.starts_with(&slug)
            || (!want.is_empty() && names.iter().any(|n| n.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase() == want))
        {
            return Some(path);
        }
    }
    None
}

fn extract_family(g: &GFamily, zp: &Path) -> Result<Vec<PathBuf>, String> {
    {
        let mut zip = open_zip(zp)?;
        let names = entry_names(&zip);
        let fonts = pick_fonts(&names);
        if fonts.is_empty() {
            return Err(format!("{} sent no font files for this family", g.provider.label()));
        }
        let dir = g.dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut out = vec![];
        for n in &fonts {
            let base = n.rsplit(['/', '\\']).next().unwrap_or(n).to_string();
            let dest = dir.join(&base);
            let data = read_entry(&mut zip, n)?;
            std::fs::write(&dest, data).map_err(|e| e.to_string())?;
            out.push(dest);
        }
        // keep the license that comes with the fonts next to them
        for n in &names {
            let b = n.rsplit(['/', '\\']).next().unwrap_or(n).to_string();
            let l = b.to_lowercase();
            if (l.contains("licen") || l.contains("ofl") || l.contains("ffl") || l.contains("eula")) && (l.ends_with(".txt") || l.ends_with(".md") || l.ends_with(".pdf")) && !n.to_lowercase().contains("__macosx") {
                if let Ok(data) = read_entry(&mut zip, n) {
                    let _ = std::fs::write(dir.join(&b), data);
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_desktop_static_files() {
        let names: Vec<String> = [
            "Kihim_Complete/Fonts/OTF/Kihim-Regular.otf",
            "Kihim_Complete/Fonts/TTF/Kihim-Regular.ttf",
            "Kihim_Complete/Fonts/TTF/Kihim-Variable.ttf",
            "Kihim_Complete/Fonts/WEB/fonts/Kihim-Regular.ttf",
            "__MACOSX/Kihim_Complete/._Kihim-Regular.otf",
            "Kihim_Complete/License/FFL.txt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(pick_fonts(&names), vec!["Kihim_Complete/Fonts/OTF/Kihim-Regular.otf".to_string()]);
        assert_eq!(category_of("Sans, Display"), "Sans Serif");
        assert_eq!(category_of("Slab Serif"), "Serif");
        assert_eq!(category_of("Handdrawn"), "Handwriting");
        assert_eq!(category_of("Monospaced"), "Monospace");
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, data) in entries {
            w.start_file(*n, opt).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }

    /// The browser handoff for Font Squirrel, without a browser: a placeholder that is still
    /// being written, an unrelated older zip and another family's zip are passed over; the
    /// finished package (renamed like browsers do) is found and its fonts unpacked.
    #[test]
    fn picks_up_browser_download() {
        let dl = std::env::temp_dir().join(format!("fc_dl_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dl);
        std::fs::create_dir_all(&dl).unwrap();
        let g = GFamily {
            provider: Provider::FontSquirrel,
            name: "FC Test Squirrel Family".into(),
            lower: "fc test squirrel family".into(),
            category: "Sans Serif".into(),
            kind: "Sans".into(),
            designers: vec![],
            styles: vec![],
            style_names: vec![],
            nstyles: 2,
            italic: false,
            axes: vec![],
            popularity: 0,
            added: String::new(),
            thickness: None,
            width: None,
            slug: "fc-test-squirrel".into(),
            preview_src: "FCTestSquirrel-Regular.otf".into(),
            license: String::new(),
        };
        let since = SystemTime::now();
        let mut tried = std::collections::HashMap::new();
        // nothing there yet
        assert_eq!(find_family_zip(&dl, &g, since, &mut tried), None);
        // another family's package and a half-written placeholder (Firefox makes an empty one)
        write_zip(&dl.join("other-font.zip"), &[("Other-Regular.otf", b"OTTO")]);
        std::fs::write(dl.join("fc-test-squirrel.zip"), b"").unwrap();
        std::fs::write(dl.join("fc-test-squirrel.zip.crdownload"), b"PK").unwrap();
        assert_eq!(find_family_zip(&dl, &g, since, &mut tried), None);
        // the finished download replaces the placeholder (a little later, so its time changes)
        std::thread::sleep(Duration::from_millis(1100));
        let tmp = dl.join("fc-test-squirrel.zip.part");
        write_zip(
            &tmp,
            &[
                ("FCTestSquirrel-Regular.otf", b"OTTO-regular"),
                ("FCTestSquirrel-Bold.otf", b"OTTO-bold"),
                ("SIL Open Font License.txt", b"license"),
            ],
        );
        std::fs::remove_file(dl.join("fc-test-squirrel.zip")).unwrap();
        std::fs::rename(&tmp, dl.join("fc-test-squirrel.zip")).unwrap();
        let found = find_family_zip(&dl, &g, since, &mut tried).expect("the finished package is found");
        assert_eq!(found, dl.join("fc-test-squirrel.zip"));
        // a package renamed by the browser ("(1)") is still recognised by the font inside
        std::fs::rename(&found, dl.join("download (1).zip")).unwrap();
        let mut fresh = std::collections::HashMap::new();
        assert_eq!(find_family_zip(&dl, &g, since, &mut fresh), Some(dl.join("download (1).zip")));
        // unpack into FontCraft's own folder for this provider
        let files = extract_family(&g, &dl.join("download (1).zip")).expect("unpacked");
        assert_eq!(files.len(), 2, "{files:?}");
        assert!(files.iter().all(|f| f.starts_with(g.dir()) && f.exists()));
        assert!(g.dir().join("SIL Open Font License.txt").exists());
        // the person's download stays where it was
        assert!(dl.join("download (1).zip").exists());
        let _ = std::fs::remove_dir_all(g.dir());
        let _ = std::fs::remove_dir_all(&dl);
    }

    #[test]
    fn downloads_folder_exists() {
        let d = downloads_dir().expect("a Downloads folder");
        println!("Downloads: {}", d.display());
        assert!(d.is_absolute());
    }

    #[test]
    #[ignore]
    fn fontshare_catalog_preview_and_download() {
        let list = load_catalog(Provider::Fontshare, true).expect("catalogue");
        assert!(list.len() > 50, "only {}", list.len());
        let g = list.iter().find(|g| g.slug == "satoshi").or(list.first()).unwrap().clone();
        println!("{} {:?} {:?} {}", g.name, g.style_names, g.axes, g.preview_src);
        let dest = std::env::temp_dir().join("fc_fs_preview.ttf");
        fetch_preview(&g, &dest).expect("preview");
        assert!(ttf_parser::Face::parse(&std::fs::read(&dest).unwrap(), 0).is_ok());
        let files = download_family(&g).expect("download");
        println!("{files:?}");
        assert!(!files.is_empty());
        let _ = std::fs::remove_dir_all(g.dir());
    }

    #[test]
    #[ignore]
    fn squirrel_catalog() {
        let list = load_catalog(Provider::FontSquirrel, true).expect("catalogue");
        assert!(list.len() > 500, "only {}", list.len());
        let g = list.iter().find(|g| g.slug == "open-sans").unwrap();
        assert_eq!(g.category, "Sans Serif");
        assert!(g.nstyles >= 5);
    }
}
