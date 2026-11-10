//! Google Fonts: the free catalogue, preview files and downloading whole families.
//!
//! Everything is fetched with the system's `curl` (built into Windows 10+, macOS and
//! practically every Linux), so no extra network libraries are needed.
//! Files are kept in FontCraft's data folder:
//!   google/metadata.json        the catalogue (refreshed once a week)
//!   google/previews/<name>.ttf  one regular style per family, for previews
//!   google/fonts/<Family>/...   complete families that were added to the library

use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

const METADATA_URL: &str = "https://fonts.google.com/metadata/fonts";

/// Where a family of the online catalogue comes from. Only free sources: everything
/// in them can be used for personal and commercial work without a subscription.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Provider {
    Google,
    Fontshare,
    FontSquirrel,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Google, Provider::Fontshare, Provider::FontSquirrel];
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn tag(self) -> &'static str {
        match self {
            Provider::Google => "google",
            Provider::Fontshare => "fontshare",
            Provider::FontSquirrel => "fontsquirrel",
        }
    }
    /// The short name in the sidebar.
    pub fn short(self) -> &'static str {
        match self {
            Provider::Google => "Google",
            Provider::Fontshare => "Fontshare",
            Provider::FontSquirrel => "Font Squirrel",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Provider::Google => "Google Fonts",
            Provider::Fontshare => "Fontshare",
            Provider::FontSquirrel => "Font Squirrel",
        }
    }
    pub fn site(self) -> &'static str {
        match self {
            Provider::Google => "fonts.google.com",
            Provider::Fontshare => "fontshare.com",
            Provider::FontSquirrel => "fontsquirrel.com",
        }
    }
    /// What the fonts may be used for, shown next to the count.
    pub fn terms(self) -> &'static str {
        match self {
            Provider::Google => "free and open source",
            Provider::Fontshare => "free for personal and commercial use",
            Provider::FontSquirrel => "hand-picked, free for commercial use",
        }
    }
    pub fn about(self) -> &'static str {
        match self {
            Provider::Google => "Over 1,900 free font families from Google Fonts",
            Provider::Fontshare => "About 100 high-quality families by the Indian Type Foundry – free for personal and commercial use",
            Provider::FontSquirrel => "About 1,000 hand-picked font families that are free for commercial use",
        }
    }
    /// Where downloaded families of this source are kept (one folder per family).
    pub fn fonts_dir(self) -> PathBuf {
        match self {
            Provider::Google => fonts_dir(),
            p => crate::config::data_dir().join("online").join(p.tag()).join("fonts"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct GFamily {
    pub provider: Provider,
    pub name: String,
    pub lower: String,
    /// Google's categories (Sans Serif, Serif, Display, Handwriting, Monospace), for filtering.
    pub category: String,
    /// The provider's own classification, shown in the list.
    pub kind: String,
    pub designers: Vec<String>,
    /// Style codes like "400" or "700i" (empty when the source doesn't list them).
    pub styles: Vec<String>,
    /// Readable style names.
    pub style_names: Vec<String>,
    pub nstyles: usize,
    pub italic: bool,
    pub axes: Vec<(String, f32, f32)>,
    pub popularity: u32,
    pub added: String,
    /// Google's own 1–10 ratings of the regular style, scaled to 0..1.
    pub thickness: Option<f32>,
    pub width: Option<f32>,
    /// The provider's name for the family in its addresses.
    pub slug: String,
    /// Fontshare: the regular style's file; Font Squirrel: the main file inside the download.
    pub preview_src: String,
    pub license: String,
}

impl GFamily {
    /// Unique across sources (Google families keep their plain name, as before).
    pub fn key(&self) -> String {
        key_of(self.provider, &self.name)
    }
    pub fn dir(&self) -> PathBuf {
        self.provider.fonts_dir().join(safe(&self.name))
    }
    pub fn style_label(code: &str) -> String {
        let italic = code.ends_with('i');
        let w: u16 = code.trim_end_matches('i').parse().unwrap_or(400);
        let name = match w {
            100 => "Thin",
            200 => "Extra Light",
            300 => "Light",
            400 => "Regular",
            500 => "Medium",
            600 => "Semi Bold",
            700 => "Bold",
            800 => "Extra Bold",
            900 => "Black",
            _ => "Regular",
        };
        match (italic, w) {
            (true, 400) => "Italic".into(),
            (true, _) => format!("{name} Italic"),
            _ => name.into(),
        }
    }
    pub fn url(&self) -> String {
        match self.provider {
            Provider::Google => format!("https://fonts.google.com/specimen/{}", self.name.replace(' ', "+")),
            Provider::Fontshare => format!("https://www.fontshare.com/fonts/{}", self.slug),
            Provider::FontSquirrel => format!("https://www.fontsquirrel.com/fonts/{}", self.slug),
        }
    }
}

pub fn key_of(p: Provider, name: &str) -> String {
    match p {
        Provider::Google => name.to_string(),
        p => format!("{}:{}", p.tag(), name),
    }
}

/// The key of a downloaded family folder in the library ("google/Roboto", "fontshare/Satoshi").
pub fn dl_key(p: Provider, name: &str) -> String {
    format!("{}/{}", p.tag(), safe(name))
}

pub fn root() -> PathBuf {
    crate::config::data_dir().join("google")
}
pub fn fonts_dir() -> PathBuf {
    root().join("fonts")
}
fn previews_dir() -> PathBuf {
    root().join("previews")
}
pub fn safe(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}
pub fn family_dir(name: &str) -> PathBuf {
    fonts_dir().join(safe(name))
}
pub fn preview_path(name: &str) -> PathBuf {
    previews_dir().join(format!("{}.ttf", safe(name)))
}

pub(crate) fn curl() -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new("curl");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

/// Download `url` into `dest` (atomically, through a temporary file).
pub(crate) fn download(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("part");
    let out = curl()
        .args(["-sSL", "--fail", "--max-time", "300", "-o"])
        .arg(&tmp)
        .arg(url)
        .output()
        .map_err(|_| "could not start curl (needed for downloading fonts)".to_string())?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "download failed – are you online?".into() } else { err });
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

pub(crate) fn get_text(url: &str) -> Result<String, String> {
    let out = curl()
        .args(["-sSL", "--fail", "--max-time", "60"])
        .arg(url)
        .output()
        .map_err(|_| "could not start curl (needed for downloading fonts)".to_string())?;
    if !out.status.success() {
        return Err("the font service could not be reached – are you online?".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn strip_guard(s: &str) -> &str {
    s.trim_start().trim_start_matches(")]}'").trim_start()
}

pub fn parse_catalog(text: &str) -> Vec<GFamily> {
    let Ok(v) = serde_json::from_str::<Value>(strip_guard(text)) else { return vec![] };
    let Some(list) = v.get("familyMetadataList").and_then(|l| l.as_array()) else { return vec![] };
    let mut out: Vec<GFamily> = list
        .iter()
        .filter_map(|f| {
            let name = f.get("family")?.as_str()?.to_string();
            let mut styles: Vec<String> =
                f.get("fonts").and_then(|x| x.as_object()).map(|o| o.keys().cloned().collect()).unwrap_or_default();
            styles.sort_by_key(|s| (s.ends_with('i'), s.trim_end_matches('i').parse::<u16>().unwrap_or(400)));
            let axes = f
                .get("axes")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            Some((
                                x.get("tag")?.as_str()?.to_string(),
                                x.get("min")?.as_f64()? as f32,
                                x.get("max")?.as_f64()? as f32,
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let regular = f.get("fonts").and_then(|o| o.get("400").or_else(|| o.as_object().and_then(|m| m.values().next())));
            let rating = |k: &str| regular.and_then(|r| r.get(k)).and_then(|v| v.as_f64()).map(|v| ((v as f32 - 1.0) / 9.0).clamp(0.0, 1.0));
            let category = f.get("category").and_then(|c| c.as_str()).unwrap_or("").to_string();
            Some(GFamily {
                provider: Provider::Google,
                kind: category.clone(),
                style_names: styles.iter().map(|s| GFamily::style_label(s)).collect(),
                nstyles: styles.len(),
                italic: styles.iter().any(|s| s.ends_with('i')),
                slug: String::new(),
                preview_src: String::new(),
                license: String::new(),
                thickness: rating("thickness"),
                width: rating("width"),
                lower: name.to_lowercase(),
                category,
                designers: f
                    .get("designers")
                    .and_then(|d| d.as_array())
                    .map(|d| d.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                styles,
                axes,
                popularity: f.get("popularity").and_then(|p| p.as_u64()).unwrap_or(99999) as u32,
                added: f.get("dateAdded").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.lower.cmp(&b.lower));
    out
}

/// Load the catalogue from disk; fetch it from Google when missing or older than a week.
pub fn load_catalog(allow_network: bool) -> Result<Vec<GFamily>, String> {
    let path = root().join("metadata.json");
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|age| age < Duration::from_secs(7 * 24 * 3600))
        .unwrap_or(false);
    if (!fresh || !path.exists()) && allow_network {
        match download(METADATA_URL, &path) {
            Ok(()) => {}
            Err(e) if !path.exists() => return Err(e),
            Err(_) => {}
        }
    }
    let text = std::fs::read_to_string(&path).map_err(|_| "The Google Fonts list has not been downloaded yet.".to_string())?;
    let list = parse_catalog(&text);
    if list.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Err("The Google Fonts list could not be read. Try again later.".into());
    }
    Ok(list)
}

/// Download every font file of a family into the library folder; returns the files.
/// Static styles are preferred over variable files because Word and many other apps
/// handle them better (and having both would show every style twice).
pub fn download_family(name: &str) -> Result<Vec<PathBuf>, String> {
    let text = get_text(&format!("https://fonts.google.com/download/list?family={}", name.replace(' ', "+")))?;
    let v: Value = serde_json::from_str(strip_guard(&text)).map_err(|_| "unexpected answer from Google Fonts".to_string())?;
    let refs = v
        .pointer("/manifest/fileRefs")
        .and_then(|r| r.as_array())
        .ok_or_else(|| "Google Fonts sent no files for this family".to_string())?;
    let mut files: Vec<(String, String)> = refs
        .iter()
        .filter_map(|r| Some((r.get("filename")?.as_str()?.to_string(), r.get("url")?.as_str()?.to_string())))
        .filter(|(f, _)| {
            let l = f.to_lowercase();
            l.ends_with(".ttf") || l.ends_with(".otf")
        })
        .collect();
    // Families made for several optical sizes ("Inter_18pt", "Inter_24pt"…) would turn into one
    // family per size; their variable file keeps them together as one family.
    let optical_sizes = files.iter().any(|(f, _)| {
        let stem = f.trim_start_matches("static/").split('-').next().unwrap_or("");
        stem.split('_').any(|part| part.len() > 2 && part.ends_with("pt") && part[..part.len() - 2].chars().all(|c| c.is_ascii_digit()))
    });
    let has_variable = files.iter().any(|(f, _)| !f.starts_with("static/"));
    if optical_sizes && has_variable {
        files.retain(|(f, _)| !f.starts_with("static/"));
    } else if files.iter().any(|(f, _)| f.starts_with("static/")) {
        files.retain(|(f, _)| f.starts_with("static/"));
    }
    if files.is_empty() {
        return Err("Google Fonts sent no font files for this family".into());
    }
    let dir = family_dir(name);
    let mut out = vec![];
    for (f, url) in files {
        let base = f.rsplit('/').next().unwrap_or(&f).to_string();
        let dest = dir.join(base);
        if !dest.exists() {
            download(&url, &dest)?;
        }
        out.push(dest);
    }
    Ok(out)
}


fn fetch_preview(g: &GFamily) -> Result<PathBuf, String> {
    let dest = preview_path(&g.key());
    if dest.exists() {
        return Ok(dest);
    }
    if g.provider != Provider::Google {
        return crate::online::fetch_preview(g, &dest);
    }
    let name = &g.name;
    let css = get_text(&format!("https://fonts.googleapis.com/css2?family={}", name.replace(' ', "+")))?;
    let url = css
        .split("url(")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .ok_or_else(|| "no preview file".to_string())?;
    download(url, &dest)?;
    Ok(dest)
}

// ------------------------------------------------------------------ letter sets for the font finder

/// A small file with just the letters the font finder compares (a few KB per family).
pub fn letter_set_path(name: &str) -> PathBuf {
    root().join("letters").join(format!("{}.ttf", safe(name)))
}
fn letter_set_none(name: &str) -> PathBuf {
    root().join("letters").join(format!("{}.none", safe(name)))
}

/// Whether the letter set is on disk (Some(true)), known to be unavailable (Some(false)) or not fetched yet.
pub fn letter_set_state(name: &str) -> Option<bool> {
    if letter_set_path(name).exists() {
        Some(true)
    } else if letter_set_none(name).exists() {
        Some(false)
    } else {
        None
    }
}

/// How to ask Google for a family's regular style (or its first style when it has none).
pub fn letter_spec(g: &GFamily) -> String {
    let base = g.name.replace(' ', "+");
    if g.styles.is_empty() || g.styles.iter().any(|s| s == "400") {
        return base;
    }
    let s = &g.styles[0];
    let w = s.trim_end_matches('i');
    if s.ends_with('i') {
        format!("{base}:ital,wght@1,{w}")
    } else {
        format!("{base}:wght@{w}")
    }
}

pub fn letter_set_cand(name: &str, path: PathBuf) -> crate::fontmatch::Cand {
    crate::fontmatch::Cand {
        path,
        index: 0,
        family: name.to_string(),
        style: String::new(),
        google: true,
        id: crate::activation::fnv(&format!("gletters:{name}")),
    }
}

fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b == b'.' { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Fetch the letter sets of a batch of (family, spec); returns the ones now on disk.
pub fn fetch_letter_sets(batch: &[(String, String)]) -> Vec<(String, PathBuf)> {
    if batch.is_empty() {
        return vec![];
    }
    let families: Vec<String> = batch.iter().map(|(_, spec)| format!("family={spec}")).collect();
    let url = format!(
        "https://fonts.googleapis.com/css2?{}&text={}",
        families.join("&"),
        percent(&format!("{} .", crate::fontmatch::CHARSET))
    );
    let css = match get_text(&url) {
        Ok(c) => c,
        Err(_) if batch.len() > 1 => {
            // one odd family can spoil a whole request: try the halves
            let (a, b) = batch.split_at(batch.len() / 2);
            let mut v = fetch_letter_sets(a);
            v.extend(fetch_letter_sets(b));
            return v;
        }
        Err(_) => return vec![],
    };
    let mut urls: HashMap<String, String> = HashMap::new();
    for block in css.split("@font-face").skip(1) {
        let fam = block.split("font-family:").nth(1).and_then(|s| s.split(';').next()).map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string());
        let u = block.split("url(").nth(1).and_then(|s| s.split(')').next()).map(|s| s.to_string());
        if let (Some(f), Some(u)) = (fam, u) {
            urls.entry(f).or_insert(u);
        }
    }
    let dir = root().join("letters");
    let _ = std::fs::create_dir_all(&dir);
    let mut cmd = curl();
    cmd.args(["-sSL", "--fail", "--max-time", "120", "-Z", "--parallel-max", "12"]);
    let mut want = vec![];
    for (name, _) in batch {
        match urls.get(name) {
            Some(u) => {
                let tmp = letter_set_path(name).with_extension("part");
                cmd.arg("-o").arg(&tmp).arg(u);
                want.push(name.clone());
            }
            // Google has no letters for it (icon fonts, other scripts): don't ask again
            None => {
                let _ = std::fs::write(letter_set_none(name), b"");
            }
        }
    }
    if want.is_empty() {
        return vec![];
    }
    let _ = cmd.output();
    let mut out = vec![];
    for name in want {
        let tmp = letter_set_path(&name).with_extension("part");
        let ok = std::fs::read(&tmp).map(|d| ttf_parser::Face::parse(&d, 0).is_ok()).unwrap_or(false);
        if ok && std::fs::rename(&tmp, letter_set_path(&name)).is_ok() {
            out.push((name.clone(), letter_set_path(&name)));
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    out
}

// ------------------------------------------------------------------ background workers

pub enum GMsg {
    Catalog(Provider, Result<Vec<GFamily>, String>),
    /// family key, preview file
    Preview(String, Option<PathBuf>),
    /// family key, display name
    Family { key: String, name: String, activate: bool, result: Result<Vec<PathBuf>, String> },
    /// a source that only hands out files to a browser: open this download there
    OpenInBrowser { name: String, url: String },
    Removed,
}

type Queue = Arc<Mutex<VecDeque<GFamily>>>;

pub struct Google {
    pub tx: Sender<GMsg>,
    pub rx: Receiver<GMsg>,
    /// Google Fonts and Fontshare previews (several at once)
    queue: Queue,
    pub previews: HashMap<String, Option<PathBuf>>,
    pub failed: HashSet<String>,
    ctx: egui::Context,
}

fn spawn_worker(q: Queue, tx: Sender<GMsg>, ctx: egui::Context) {
    std::thread::spawn(move || loop {
        let next = q.lock().ok().and_then(|mut q| q.pop_front());
        match next {
            Some(g) => {
                let r = fetch_preview(&g).ok();
                if tx.send(GMsg::Preview(g.key(), r)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
            None => std::thread::sleep(Duration::from_millis(60)),
        }
    });
}

impl Google {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
        for _ in 0..4 {
            spawn_worker(queue.clone(), tx.clone(), ctx.clone());
        }
        Self { tx, rx, queue, previews: HashMap::new(), failed: HashSet::new(), ctx }
    }

    /// Load (or download) the catalogue of every source, each in its own thread.
    pub fn load_catalogs(&self, allow_network: bool) {
        for p in Provider::ALL {
            self.load_catalog(p, allow_network);
        }
    }

    pub fn load_catalog(&self, p: Provider, allow_network: bool) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let r = match p {
                Provider::Google => load_catalog(allow_network),
                p => crate::online::load_catalog(p, allow_network),
            };
            let _ = tx.send(GMsg::Catalog(p, r));
            ctx.request_repaint();
        });
    }

    /// The preview file of a family, or None while it is being fetched.
    pub fn preview(&mut self, g: &GFamily, allow_network: bool) -> Option<PathBuf> {
        let key = g.key();
        if let Some(st) = self.previews.get(&key) {
            return st.clone();
        }
        if self.failed.contains(&key) {
            return None;
        }
        let p = preview_path(&key);
        if p.exists() {
            self.previews.insert(key, Some(p.clone()));
            return Some(p);
        }
        // Font Squirrel allows about one automatic request every few minutes: no previews
        if allow_network && g.provider != Provider::FontSquirrel {
            self.previews.insert(key, None);
            if let Ok(mut q) = self.queue.lock() {
                // newest requests first: what's on screen now matters most
                q.push_front(g.clone());
                while q.len() > 80 {
                    if let Some(old) = q.pop_back() {
                        self.previews.remove(&old.key());
                    }
                }
            }
        }
        None
    }

    pub fn download(&self, g: GFamily, activate: bool) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let since = SystemTime::now();
            let mut result = match g.provider {
                Provider::Google => download_family(&g.name),
                Provider::Fontshare => crate::online::download_family(&g),
                // Font Squirrel only hands its files to people in a browser (see below)
                Provider::FontSquirrel => Err(String::new()),
            };
            // Font Squirrel wants a person behind its downloads: let the browser fetch the
            // package, then pick it up from the Downloads folder.
            if g.provider == Provider::FontSquirrel || matches!(&result, Err(e) if crate::online::is_blocked_error(e)) {
                let _ = tx.send(GMsg::OpenInBrowser { name: g.name.clone(), url: crate::online::browser_url(&g) });
                ctx.request_repaint();
                result = crate::online::wait_for_browser_download(&g, since);
            }
            let _ = tx.send(GMsg::Family { key: g.key(), name: g.name.clone(), activate, result });
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_preview_and_family_download() {
        let list = load_catalog(true).expect("catalogue");
        assert!(list.len() > 1000, "only {} families", list.len());
        let g = list.iter().find(|g| g.name == "ABeeZee").expect("ABeeZee").clone();
        let p = fetch_preview(&g).expect("preview");
        assert!(std::fs::metadata(&p).unwrap().len() > 1000);
        let files = download_family("ABeeZee").expect("download");
        assert_eq!(files.len(), 2, "{files:?}");
        let _ = std::fs::remove_dir_all(family_dir("ABeeZee"));
    }

    /// Fetch the previews of the most popular Fontshare families (for looking at the list offline).
    #[test]
    #[ignore]
    fn warm_fontshare_previews() {
        let mut list = crate::online::load_catalog(Provider::Fontshare, true).expect("catalogue");
        list.sort_by_key(|g| g.popularity);
        for g in list.iter().take(40) {
            let r = fetch_preview(g);
            assert!(r.is_ok(), "{}: {r:?}", g.name);
        }
    }

    #[test]
    #[ignore]
    fn letter_sets() {
        let list = load_catalog(true).expect("catalogue");
        let pick: Vec<(String, String)> = ["Lobster", "Buda", "Roboto", "Material Icons"]
            .iter()
            .filter_map(|n| list.iter().find(|g| g.name == *n))
            .map(|g| (g.name.clone(), letter_spec(g)))
            .collect();
        println!("{pick:?}");
        let got = fetch_letter_sets(&pick);
        println!("{got:?}");
        assert!(got.len() >= 3, "{got:?}");
        for (_, p) in &got {
            let d = std::fs::read(p).unwrap();
            let f = ttf_parser::Face::parse(&d, 0).unwrap();
            assert!(f.glyph_index('Q').is_some() && f.glyph_index('7').is_some());
        }
    }

    /// Fill a demo profile with free Google families for the README screenshots.
    /// Run with APPDATA (Windows) / HOME pointing at a throw-away folder.
    #[test]
    #[ignore]
    fn demo_library() {
        let names = std::env::var("FC_DEMO_FAMILIES").unwrap_or_else(|_| {
            "Abril Fatface,Bebas Neue,Caveat,DM Serif Display,Fraunces,Inter,JetBrains Mono,Lora,Pacifico,Playfair Display,Roboto Slab,Space Grotesk".into()
        });
        load_catalog(true).expect("catalogue");
        for n in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            let files = download_family(n).unwrap_or_else(|e| panic!("{n}: {e}"));
            println!("{n}: {} files", files.len());
        }
    }

    #[test]
    #[ignore]
    fn all_letter_sets_timing() {
        let list = load_catalog(true).expect("catalogue");
        let missing: Vec<(String, String)> = list.iter().filter(|g| letter_set_state(&g.name).is_none()).map(|g| (g.name.clone(), letter_spec(g))).collect();
        let t = std::time::Instant::now();
        let mut n = 0;
        for b in missing.chunks(48) {
            n += fetch_letter_sets(b).len();
        }
        let none = list.iter().filter(|g| letter_set_state(&g.name) == Some(false)).count();
        println!("fetched {n} of {} missing in {:?}; {none} without letters", missing.len(), t.elapsed());
    }
}