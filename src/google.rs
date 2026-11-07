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

#[derive(Clone, Debug)]
pub struct GFamily {
    pub name: String,
    pub lower: String,
    pub category: String,
    pub designers: Vec<String>,
    pub styles: Vec<String>,
    pub axes: Vec<(String, f32, f32)>,
    pub popularity: u32,
    pub added: String,
    /// Google's own 1–10 ratings of the regular style, scaled to 0..1.
    pub thickness: Option<f32>,
    pub width: Option<f32>,
}

impl GFamily {
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
        format!("https://fonts.google.com/specimen/{}", self.name.replace(' ', "+"))
    }
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

fn curl() -> std::process::Command {
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
fn download(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("part");
    let out = curl()
        .args(["-sSL", "--fail", "--max-time", "300", "-o"])
        .arg(&tmp)
        .arg(url)
        .output()
        .map_err(|_| "could not start curl (needed to download from Google Fonts)".to_string())?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "download failed – are you online?".into() } else { err });
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

fn get_text(url: &str) -> Result<String, String> {
    let out = curl()
        .args(["-sSL", "--fail", "--max-time", "60"])
        .arg(url)
        .output()
        .map_err(|_| "could not start curl (needed to download from Google Fonts)".to_string())?;
    if !out.status.success() {
        return Err("Google Fonts could not be reached – are you online?".into());
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
            Some(GFamily {
                thickness: rating("thickness"),
                width: rating("width"),
                lower: name.to_lowercase(),
                category: f.get("category").and_then(|c| c.as_str()).unwrap_or("").to_string(),
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
    if files.iter().any(|(f, _)| f.starts_with("static/")) {
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


fn fetch_preview(name: &str) -> Result<PathBuf, String> {
    let dest = preview_path(name);
    if dest.exists() {
        return Ok(dest);
    }
    let css = get_text(&format!("https://fonts.googleapis.com/css2?family={}", name.replace(' ', "+")))?;
    let url = css
        .split("url(")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .ok_or_else(|| "no preview file".to_string())?;
    download(url, &dest)?;
    Ok(dest)
}

// ------------------------------------------------------------------ background workers

pub enum GMsg {
    Catalog(Result<Vec<GFamily>, String>),
    Preview(String, Option<PathBuf>),
    Family { name: String, activate: bool, result: Result<Vec<PathBuf>, String> },
    Removed,
}

pub struct Google {
    pub tx: Sender<GMsg>,
    pub rx: Receiver<GMsg>,
    queue: Arc<Mutex<VecDeque<String>>>,
    pub previews: HashMap<String, Option<PathBuf>>,
    pub failed: HashSet<String>,
    ctx: egui::Context,
}

impl Google {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        let queue: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        for _ in 0..4 {
            let q = queue.clone();
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || loop {
                let next = q.lock().ok().and_then(|mut q| q.pop_front());
                match next {
                    Some(name) => {
                        let r = fetch_preview(&name).ok();
                        if tx.send(GMsg::Preview(name, r)).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                    None => std::thread::sleep(Duration::from_millis(60)),
                }
            });
        }
        Self { tx, rx, queue, previews: HashMap::new(), failed: HashSet::new(), ctx }
    }

    pub fn load_catalog(&self, allow_network: bool) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(GMsg::Catalog(load_catalog(allow_network)));
            ctx.request_repaint();
        });
    }

    /// The preview file of a family, or None while it is being fetched.
    pub fn preview(&mut self, name: &str, allow_network: bool) -> Option<PathBuf> {
        if let Some(st) = self.previews.get(name) {
            return st.clone();
        }
        if self.failed.contains(name) {
            return None;
        }
        let p = preview_path(name);
        if p.exists() {
            self.previews.insert(name.to_string(), Some(p.clone()));
            return Some(p);
        }
        if allow_network {
            self.previews.insert(name.to_string(), None);
            if let Ok(mut q) = self.queue.lock() {
                // newest requests first: what's on screen now matters most
                q.push_front(name.to_string());
                while q.len() > 80 {
                    if let Some(old) = q.pop_back() {
                        self.previews.remove(&old);
                    }
                }
            }
        }
        None
    }


    pub fn download(&self, name: String, activate: bool) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let result = download_family(&name);
            let _ = tx.send(GMsg::Family { name, activate, result });
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
        let p = fetch_preview("ABeeZee").expect("preview");
        assert!(std::fs::metadata(&p).unwrap().len() > 1000);
        let files = download_family("ABeeZee").expect("download");
        assert_eq!(files.len(), 2, "{files:?}");
        let _ = std::fs::remove_dir_all(family_dir("ABeeZee"));
    }
}
