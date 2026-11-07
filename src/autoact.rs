//! Auto-activation: find out which fonts a document uses and switch them on.
//!
//! Works without plugins by reading the document file itself:
//! * Photoshop, Illustrator, InDesign, PDF, EPS: font (PostScript) names stored as text
//! * SVG: font-family attributes and styles
//! * Word, PowerPoint, Excel, InDesign IDML (zip-based): the font names in their XML
//!
//! Project folders can be watched: whenever a document there is saved or added,
//! its fonts are activated.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub const DOC_EXTS: &[&str] = &[
    "psd", "psb", "ai", "indd", "indt", "idml", "pdf", "eps", "svg", "docx", "dotx", "pptx", "potx", "xlsx",
];

pub fn is_document(p: &Path) -> bool {
    p.extension()
        .map(|e| DOC_EXTS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Font names found in a document.
#[derive(Default, Debug, Clone)]
pub struct DocFonts {
    /// exact names such as PostScript names ("roboto-bold"), lower case
    pub names: HashSet<String>,
    /// family names written out in the document ("open sans"), lower case
    pub families: HashSet<String>,
}

fn is_name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
}

fn scan_names(data: &[u8], out: &mut HashSet<String>) {
    let mut push = |w: &[u8]| {
        if w.len() >= 3 && w.len() <= 80 {
            if let Ok(s) = std::str::from_utf8(w) {
                out.insert(s.to_ascii_lowercase());
            }
        }
    };
    let mut start = None;
    for (i, &b) in data.iter().enumerate() {
        if is_name_byte(b) {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            push(&data[s..i]);
        }
    }
    // UTF-16 text (Photoshop and InDesign store names this way), both byte orders
    for off in 0..2 {
        let mut buf = Vec::new();
        let mut i = off;
        while i + 1 < data.len() {
            let (ch, zero) = if off == 0 { (data[i], data[i + 1]) } else { (data[i + 1], data[i]) };
            if zero == 0 && is_name_byte(ch) {
                buf.push(ch);
            } else if !buf.is_empty() {
                push(&buf);
                buf.clear();
            }
            i += 2;
        }
    }
}

/// Values of XML attributes that hold font names, plus CSS font-family values.
fn scan_xml(text: &str, fonts: &mut DocFonts) {
    const ATTRS: [&str; 9] =
        ["w:name=\"", "w:ascii=\"", "w:hAnsi=\"", "typeface=\"", "FontFamily=\"", "PostScriptName=\"", "font-family=\"", "AppliedFont=\"", "name=\""];
    for a in ATTRS {
        let mut rest = text;
        while let Some(pos) = rest.find(a) {
            rest = &rest[pos + a.len()..];
            let Some(end) = rest.find('"') else { break };
            let v = rest[..end].trim().trim_matches('\'').to_lowercase();
            rest = &rest[end..];
            if v.len() >= 2 && v.len() <= 80 && !v.starts_with('+') {
                if a == "PostScriptName=\"" {
                    fonts.names.insert(v);
                } else if a != "name=\"" || text.contains("<w:font ") {
                    for part in v.split(',') {
                        fonts.families.insert(part.trim().trim_matches('\'').to_string());
                    }
                }
            }
        }
    }
    let mut rest = text;
    while let Some(pos) = rest.find("font-family:") {
        rest = &rest[pos + 12..];
        let end = rest.find([';', '"', '}']).unwrap_or(rest.len());
        for part in rest[..end].split(',') {
            let v = part.trim().trim_matches(|c| c == '\'' || c == '"').to_lowercase();
            if v.len() >= 2 && v.len() <= 80 {
                fonts.families.insert(v);
            }
        }
        rest = &rest[end..];
    }
}

fn u16le(d: &[u8], i: usize) -> usize {
    d.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0)
}
fn u32le(d: &[u8], i: usize) -> usize {
    d.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).unwrap_or(0)
}

/// The XML files inside a zip-based document (docx, pptx, idml…).
fn zip_xml_entries(data: &[u8]) -> Vec<String> {
    let mut out = vec![];
    // find the "end of central directory" record
    let Some(eocd) = (0..data.len().saturating_sub(21)).rev().take(70_000).find(|&i| data[i..].starts_with(b"PK\x05\x06")) else {
        return out;
    };
    let count = u16le(data, eocd + 10);
    let mut p = u32le(data, eocd + 16);
    for _ in 0..count {
        if !data.get(p..).map(|d| d.starts_with(b"PK\x01\x02")).unwrap_or(false) {
            break;
        }
        let method = u16le(data, p + 10);
        let csize = u32le(data, p + 20);
        let usize_ = u32le(data, p + 24);
        let nlen = u16le(data, p + 28);
        let elen = u16le(data, p + 30);
        let clen = u16le(data, p + 32);
        let local = u32le(data, p + 42);
        let name = String::from_utf8_lossy(data.get(p + 46..p + 46 + nlen).unwrap_or(&[])).to_lowercase();
        p += 46 + nlen + elen + clen;
        if !name.ends_with(".xml") || usize_ > 64_000_000 {
            continue;
        }
        let lnlen = u16le(data, local + 26);
        let lelen = u16le(data, local + 28);
        let start = local + 30 + lnlen + lelen;
        let Some(raw) = data.get(start..start + csize) else { continue };
        let bytes = match method {
            0 => raw.to_vec(),
            8 => {
                let mut v = Vec::with_capacity(usize_);
                if flate2::read::DeflateDecoder::new(raw).read_to_end(&mut v).is_err() {
                    continue;
                }
                v
            }
            _ => continue,
        };
        out.push(String::from_utf8_lossy(&bytes).to_string());
    }
    out
}

pub fn document_fonts(path: &Path) -> DocFonts {
    let mut fonts = DocFonts::default();
    let Ok(data) = std::fs::read(path) else { return fonts };
    let data = &data[..data.len().min(400_000_000)];
    if data.starts_with(b"PK\x03\x04") {
        for xml in zip_xml_entries(data) {
            scan_xml(&xml, &mut fonts);
        }
    } else {
        scan_names(data, &mut fonts.names);
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if ext == "svg" {
            scan_xml(&String::from_utf8_lossy(data), &mut fonts);
        }
    }
    fonts
}

fn nospace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase()
}

/// Is this face used by the document?
pub fn face_used(doc: &DocFonts, family: &str, postscript: &str, full_name: &str) -> bool {
    let ps = postscript.to_ascii_lowercase();
    if ps.len() >= 3 && doc.names.contains(&ps) {
        return true;
    }
    let full = nospace(full_name);
    if full.len() >= 5 && doc.names.contains(&full) {
        return true;
    }
    let fam = family.to_lowercase();
    doc.families.contains(&fam) || doc.families.contains(&nospace(family))
}

/// Family names the document asks for (used to look for missing fonts on Google Fonts).
pub fn wanted_families(doc: &DocFonts) -> HashSet<String> {
    let mut out: HashSet<String> = doc.families.iter().map(|f| nospace(f)).collect();
    for n in &doc.names {
        if let Some((base, _)) = n.split_once('-') {
            if base.len() >= 3 {
                out.insert(base.to_string());
            }
        }
    }
    out
}

// ------------------------------------------------------------------ background work

pub struct AutoAct {
    pub folders: Arc<Mutex<Vec<PathBuf>>>,
    jobs: Sender<(PathBuf, bool)>,
    pub rx: Receiver<(PathBuf, DocFonts, bool)>,
}

impl AutoAct {
    pub fn new(folders: Vec<PathBuf>, ctx: egui::Context, watch: bool) -> Self {
        let (res_tx, rx) = channel();
        let (jobs, job_rx) = channel::<(PathBuf, bool)>();
        {
            let ctx = ctx.clone();
            let res_tx = res_tx.clone();
            std::thread::spawn(move || {
                while let Ok((path, auto)) = job_rx.recv() {
                    let fonts = document_fonts(&path);
                    if res_tx.send((path, fonts, auto)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        }
        let folders = Arc::new(Mutex::new(folders));
        if watch {
            let list = folders.clone();
            let jobs = jobs.clone();
            std::thread::spawn(move || {
                let mut seen: HashMap<PathBuf, SystemTime> = HashMap::new();
                let mut known_roots: Vec<PathBuf> = vec![];
                loop {
                    let roots = list.lock().map(|l| l.clone()).unwrap_or_default();
                    let mut now: HashMap<PathBuf, SystemTime> = HashMap::new();
                    for root in &roots {
                        for e in walkdir::WalkDir::new(root).max_depth(8).into_iter().filter_map(|e| e.ok()) {
                            if e.file_type().is_file() && is_document(e.path()) {
                                if let Ok(t) = std::fs::metadata(e.path()).and_then(|m| m.modified()) {
                                    now.insert(e.path().to_path_buf(), t);
                                }
                            }
                        }
                    }
                    for (p, t) in &now {
                        // documents in a folder that was just added only count from now on
                        let new_root = !known_roots.iter().any(|r| p.starts_with(r));
                        let changed = seen.get(p).map(|old| old != t).unwrap_or(true);
                        if changed && !new_root && jobs.send((p.clone(), true)).is_err() {
                            return;
                        }
                    }
                    seen = now;
                    known_roots = roots;
                    std::thread::sleep(Duration::from_secs(3));
                }
            });
        }
        Self { folders, jobs, rx }
    }

    pub fn set_folders(&self, f: Vec<PathBuf>) {
        if let Ok(mut l) = self.folders.lock() {
            *l = f;
        }
    }

    /// Read a document in the background.
    pub fn check(&self, path: PathBuf, auto: bool) {
        let _ = self.jobs.send((path, auto));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_fonts_in_svg_and_binary() {
        let mut d = DocFonts::default();
        scan_xml(r#"<text font-family="Open Sans" style="font-family:'Lato', serif;font-weight:700">Hi</text>"#, &mut d);
        assert!(d.families.contains("open sans") && d.families.contains("lato"));
        assert!(face_used(&d, "Open Sans", "OpenSans-Regular", "Open Sans Regular"));
        let mut names = HashSet::new();
        scan_names(b"/BaseFont /ABCDEF+Roboto-Bold /Type", &mut names);
        let doc = DocFonts { names, families: HashSet::new() };
        assert!(face_used(&doc, "Roboto", "Roboto-Bold", "Roboto Bold"));
        assert!(!face_used(&doc, "Roboto", "Roboto-Regular", "Roboto Regular"));
        assert!(wanted_families(&doc).contains("roboto"));
    }
}
