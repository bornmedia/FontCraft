//! Making fonts available to every other program (Word, Photoshop, Affinity, ...)
//! without installing them permanently.
//!
//! * Windows: the font is placed in the user's own font folder
//!   (`%LOCALAPPDATA%\Microsoft\Windows\Fonts`) as a hard link to the original file – or, when
//!   it lives on another drive, as a copy that is removed again on deactivation – and listed in
//!   the user's font list, exactly like a font installed "for me only". It is loaded with
//!   `AddFontResourceExW` and all windows are told that the font list changed. Apps that
//!   collect fonts into a package (Illustrator, InDesign, …) only look in the Windows font
//!   folders, which is why the font has to be there.
//! * macOS: a link to the font file is placed in the user's `~/Library/Fonts`.
//! * Linux: a link is placed in `~/.local/share/fonts/FontCraft` and the
//!   fontconfig cache is refreshed.

use std::path::{Path, PathBuf};

/// Prefix used for links FontCraft creates, so the system-font scan can skip them.
pub const LINK_PREFIX: &str = "FontCraft-";

pub fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[allow(dead_code)]
fn link_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        let l = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        return Some(l.join("Microsoft").join("Windows").join("Fonts"));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    if cfg!(target_os = "macos") {
        Some(home.join("Library").join("Fonts"))
    } else {
        Some(home.join(".local").join("share").join("fonts").join("FontCraft"))
    }
}

#[allow(dead_code)]
fn link_path(font: &Path) -> Option<PathBuf> {
    let name = font.file_name()?.to_string_lossy().to_string();
    let h = fnv(&font.to_string_lossy());
    Some(link_dir()?.join(format!("{LINK_PREFIX}{h:016x}-{name}")))
}

/// Activate the given font files. Returns a list of (file, error) for failures.
pub fn activate(paths: &[PathBuf]) -> Vec<(PathBuf, String)> {
    let mut errors = Vec::new();
    for p in paths {
        if let Err(e) = activate_one(p) {
            errors.push((p.clone(), e));
        }
    }
    notify();
    errors
}

/// Deactivate the given font files.
pub fn deactivate(paths: &[PathBuf]) -> Vec<(PathBuf, String)> {
    let mut errors = Vec::new();
    for p in paths {
        if let Err(e) = deactivate_one(p) {
            errors.push((p.clone(), e));
        }
    }
    notify();
    errors
}

/// Tell all programs that the list of fonts changed.
#[allow(dead_code)]
pub fn notify_font_change() {
    notify();
}

#[cfg(windows)]
fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// Unload a font file from this session (it may have been loaded more than once).
#[cfg(windows)]
fn unload(p: &Path) {
    use windows_sys::Win32::Graphics::Gdi::RemoveFontResourceExW;
    let w = wide(p);
    for _ in 0..16 {
        if unsafe { RemoveFontResourceExW(w.as_ptr(), 0, std::ptr::null()) } == 0 {
            break;
        }
    }
}

/// Which files in the user's font folder FontCraft put there, and for which original
/// (`placed-fonts.json` in FontCraft's settings folder). The files keep their own names – that is
/// the name apps like Illustrator copy into a package – so this list is how FontCraft tells its
/// files from fonts the person installed themselves.
#[cfg(windows)]
mod placed {
    use std::collections::{BTreeMap, HashSet};
    use std::path::{Path, PathBuf};

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    pub fn guard() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn file() -> PathBuf {
        crate::config::data_dir().join("placed-fonts.json")
    }

    /// file in the font folder -> original file
    pub fn load() -> BTreeMap<String, String> {
        std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(map: &BTreeMap<String, String>) {
        let f = file();
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_vec_pretty(map) {
            let tmp = f.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, &f);
            }
        }
    }

    /// The file FontCraft placed for this original, if any.
    pub fn for_original(map: &BTreeMap<String, String>, original: &Path) -> Option<PathBuf> {
        let o = original.to_string_lossy();
        map.iter().find(|(_, v)| v.eq_ignore_ascii_case(&o)).map(|(k, _)| PathBuf::from(k))
    }

    /// Lower-case paths of all files FontCraft placed.
    pub fn set() -> HashSet<String> {
        load().keys().map(|k| k.to_lowercase()).collect()
    }
}

/// Files FontCraft placed in the system's font folders (lower-case paths), so the library scan
/// doesn't show them a second time as system fonts.
pub fn placed_files() -> std::collections::HashSet<String> {
    #[cfg(windows)]
    {
        placed::set()
    }
    #[cfg(not(windows))]
    {
        Default::default()
    }
}

/// Put the font into the user's font folder under its own file name: a hard link when it is on
/// the same drive (no extra space), otherwise a copy. Returns the file in the font folder.
/// Fails when the person already installed the very same font file themselves.
#[cfg(windows)]
fn place_in_font_folder(p: &Path) -> Result<PathBuf, String> {
    let dir = link_dir().ok_or("no font folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _g = placed::guard();
    let mut map = placed::load();
    let size = std::fs::metadata(p).map(|m| m.len()).map_err(|e| e.to_string())?;
    let target = match placed::for_original(&map, p) {
        Some(t) => {
            match std::fs::metadata(&t) {
                Ok(m) if m.len() == size => return Ok(t),
                // the original changed (a newer version of the font): place it again
                Ok(_) => {
                    unload(&t);
                    let _ = std::fs::remove_file(&t);
                }
                Err(_) => {}
            }
            t
        }
        None => {
            let ours: std::collections::HashSet<String> = map.keys().map(|k| k.to_lowercase()).collect();
            let name = p.file_name().ok_or("no file name")?.to_string_lossy().to_string();
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let ext = p.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
            let mut t = dir.join(&name);
            let mut k = 2;
            while t.exists() || ours.contains(&t.to_string_lossy().to_lowercase()) {
                let theirs = !ours.contains(&t.to_string_lossy().to_lowercase());
                if theirs && std::fs::metadata(&t).map(|m| m.len() == size).unwrap_or(false) {
                    return Err("already installed".into());
                }
                // another font with the same file name (e.g. two "Regular.ttf" from different folders)
                t = dir.join(format!("{stem}-{k}{ext}"));
                k += 1;
            }
            t
        }
    };
    std::fs::hard_link(p, &target).or_else(|_| std::fs::copy(p, &target).map(|_| ())).map_err(|e| e.to_string())?;
    map.insert(target.to_string_lossy().to_string(), p.to_string_lossy().to_string());
    placed::save(&map);
    Ok(target)
}

#[cfg(windows)]
fn activate_one(p: &Path) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::AddFontResourceExW;
    if !p.exists() {
        return Err("file not found".into());
    }
    // older FontCraft versions loaded fonts straight from their folder
    unload(p);
    // The font goes into the user's font folder and font list, like a font installed "for me
    // only": apps that copy fonts into a package (Illustrator, InDesign, …) only look there.
    let file = place_in_font_folder(p).unwrap_or_else(|_| p.to_path_buf());
    let w = wide(&file);
    let n = unsafe { AddFontResourceExW(w.as_ptr(), 0, std::ptr::null()) };
    if n == 0 {
        if file != p {
            forget_placed(&file);
        }
        Err("Windows refused to load this font".into())
    } else {
        if file != p {
            userfonts::register(&file);
        }
        Ok(())
    }
}

/// Unload, unlist and delete a file FontCraft placed in the font folder.
#[cfg(windows)]
fn forget_placed(file: &Path) {
    userfonts::unregister(file);
    unload(file);
    // (an app that still has it open keeps it until FontCraft tidies up at its next start)
    if std::fs::remove_file(file).is_ok() || !file.exists() {
        let _g = placed::guard();
        let mut map = placed::load();
        let key = file.to_string_lossy().to_lowercase();
        map.retain(|k, _| k.to_lowercase() != key);
        placed::save(&map);
    }
}

#[cfg(windows)]
fn deactivate_one(p: &Path) -> Result<(), String> {
    let mine = placed::for_original(&placed::load(), p);
    if let Some(f) = mine {
        forget_placed(&f);
    }
    // FontCraft 1.2.1 test builds named the files "FontCraft-…"
    if let Some(old) = link_path(p) {
        if old.exists() {
            forget_placed(&old);
        }
    }
    userfonts::unregister(p);
    unload(p);
    Ok(())
}

/// Tidy up after fonts that are no longer active (Windows): FontCraft's entries in the user's font
/// list and its files in the user's font folder that don't belong to an active font are removed –
/// for example after FontCraft was closed in a way that left them behind.
pub fn sync_listed(active: &[PathBuf]) {
    #[cfg(windows)]
    {
        let active: std::collections::HashSet<String> = active.iter().map(|a| a.to_string_lossy().to_lowercase()).collect();
        prune(&active);
    }
    #[cfg(not(windows))]
    let _ = active;
}

/// The placed files (lower-case) that belong to one of these active originals (lower-case), looked
/// up in the record as it is right now – fonts being switched on at the same moment are kept.
#[cfg(windows)]
fn kept(active: &std::collections::HashSet<String>) -> std::collections::HashSet<String> {
    placed::load().into_iter().filter(|(_, o)| active.contains(&o.to_lowercase())).map(|(p, _)| p.to_lowercase()).collect()
}

/// Remove FontCraft's font-list entries and font-folder files that don't belong to an active font
/// (`active`: lower-case paths of the original files).
#[cfg(windows)]
fn prune(active: &std::collections::HashSet<String>) {
    userfonts::prune(&|| kept(active));
    let Some(dir) = link_dir() else { return };
    let mut gone = false;
    // files placed by this version, and "FontCraft-…" files of the 1.2.1 test builds
    let mut old: Vec<PathBuf> = placed::load().keys().map(PathBuf::from).collect();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        old.extend(rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(LINK_PREFIX)).map(|e| e.path()));
    }
    for path in old {
        if !kept(active).contains(&path.to_string_lossy().to_lowercase()) {
            forget_placed(&path);
            gone = true;
        }
    }
    if gone {
        notify();
    }
}

/// The user's own font list in the registry (HKCU\…\Fonts): "name (TrueType)" = full path of the file.
/// Windows writes the same kind of entry when a font is installed only for the current user.
/// FontCraft's entries point to its own "FontCraft-…" files in the user's font folder, so they
/// are never mixed up with fonts the person installed.
#[cfg(windows)]
mod userfonts {
    use std::path::Path;
    #[cfg(test)]
    use std::path::PathBuf;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegEnumValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ,
    };

    const KEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";
    pub const MARK: &str = "(FontCraft)";
    /// one change at a time, so two background jobs never add the same font twice
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn guard() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn open() -> Option<Key> {
        let mut h: HKEY = std::ptr::null_mut();
        let k = w(KEY);
        // opens the key (made if missing) with read and write access
        let r = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, k.as_ptr(), &mut h) };
        (r == 0).then_some(Key(h))
    }

    /// All (name, data) values of the key.
    fn entries(k: &Key) -> Vec<(String, String)> {
        let mut out = vec![];
        let mut i = 0u32;
        loop {
            let mut name = vec![0u16; 16384];
            let mut nlen = name.len() as u32;
            let mut data = vec![0u8; 65536];
            let mut dlen = data.len() as u32;
            let mut kind = 0u32;
            let r = unsafe {
                RegEnumValueW(k.0, i, name.as_mut_ptr(), &mut nlen, std::ptr::null(), &mut kind, data.as_mut_ptr(), &mut dlen)
            };
            if r != 0 {
                break;
            }
            i += 1;
            if kind != REG_SZ {
                continue;
            }
            let n = String::from_utf16_lossy(&name[..nlen as usize]);
            let words: Vec<u16> = data[..dlen as usize].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let d = String::from_utf16_lossy(&words).trim_end_matches('\0').to_string();
            out.push((n, d));
        }
        out
    }

    fn same(a: &str, b: &Path) -> bool {
        a.eq_ignore_ascii_case(&b.to_string_lossy())
    }

    /// Is this entry one FontCraft made? Its file is one FontCraft placed in the font folder (see
    /// `placed`); entries of the 1.2.1 test builds pointed to "FontCraft-…" files or carried
    /// "(FontCraft)" in the name.
    fn is_ours(name: &str, data: &str, placed: &std::collections::HashSet<String>) -> bool {
        let file = data.rsplit(['\\', '/']).next().unwrap_or("");
        placed.contains(&data.to_lowercase()) || file.starts_with(super::LINK_PREFIX) || name.contains(MARK)
    }

    /// "Lobster Regular (TrueType)" – exactly the way Windows names the entry when you install a
    /// font. Apps like Illustrator get fonts from Windows by name and find the file through this
    /// entry name, so it must be the font's English full name and nothing else.
    fn entry_name(p: &Path) -> String {
        let mut names = vec![];
        let mut cff = false;
        if let Ok(data) = std::fs::read(p) {
            let n = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
            for i in 0..n.min(32) {
                if let Ok(face) = ttf_parser::Face::parse(&data, i) {
                    cff |= face.tables().cff.is_some() || face.tables().cff2.is_some();
                    let full: Vec<(bool, bool, String)> = face
                        .names()
                        .into_iter()
                        .filter(|n| n.name_id == ttf_parser::name_id::FULL_NAME)
                        .filter_map(|n| {
                            let s = n.to_string()?.trim().to_string();
                            let english = n.language() == ttf_parser::Language::English_UnitedStates;
                            let windows = n.platform_id == ttf_parser::PlatformId::Windows;
                            (!s.is_empty()).then_some((english, windows, s))
                        })
                        .collect();
                    // Windows' own English name first, then any English name, then whatever there is
                    let best = full
                        .iter()
                        .find(|(e, w, _)| *e && *w)
                        .or_else(|| full.iter().find(|(e, _, _)| *e))
                        .or_else(|| full.first())
                        .map(|(_, _, s)| s.clone());
                    if let Some(f) = best {
                        if !names.contains(&f) {
                            names.push(f);
                        }
                    }
                }
            }
        }
        if names.is_empty() {
            names.push(p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Font".into()));
        }
        format!("{} ({})", names.join(" & "), if cff { "OpenType" } else { "TrueType" })
    }

    fn set(k: &Key, name: &str, p: &Path) -> bool {
        let data = w(&p.to_string_lossy());
        let wn = w(name);
        unsafe { RegSetValueExW(k.0, wn.as_ptr(), 0, REG_SZ, data.as_ptr() as *const u8, (data.len() * 2) as u32) == 0 }
    }

    fn delete(k: &Key, name: &str) {
        let wn = w(name);
        unsafe { RegDeleteValueW(k.0, wn.as_ptr()) };
    }

    pub fn register(p: &Path) {
        let _g = guard();
        let Some(k) = open() else { return };
        let existing = entries(&k);
        let ours = super::placed::set();
        let name = entry_name(p);
        for (n, d) in &existing {
            // an older entry of FontCraft's for this file (with "(FontCraft)" in its name) goes
            if same(d, p) && !n.eq_ignore_ascii_case(&name) && is_ours(n, d, &ours) {
                delete(&k, n);
            }
        }
        match existing.iter().find(|(n, _)| n.eq_ignore_ascii_case(&name)) {
            Some((_, d)) if same(d, p) => {}
            // The person installed this font themselves ("for me only"): their entry stays, and
            // apps already find the file through it.
            Some((n, d)) if !is_ours(n, d, &ours) => {}
            // FontCraft's entry for another copy of the same font, or none yet
            _ => {
                set(&k, &name, p);
            }
        }
    }

    pub fn unregister(p: &Path) {
        let _g = guard();
        let Some(k) = open() else { return };
        let ours = super::placed::set();
        for (name, data) in entries(&k) {
            if is_ours(&name, &data, &ours) && same(&data, p) {
                delete(&k, &name);
            }
        }
    }

    /// Remove FontCraft's entries whose file isn't in `keep()` (lower-case paths of files in the font
    /// folder, worked out once the font list is locked, so entries being added right now are kept).
    pub fn prune(keep: &dyn Fn() -> std::collections::HashSet<String>) {
        let _g = guard();
        let Some(k) = open() else { return };
        let keep = keep();
        let ours = super::placed::set();
        for (name, data) in entries(&k) {
            if is_ours(&name, &data, &ours) && (!keep.contains(&data.to_lowercase()) || name.contains(MARK)) {
                delete(&k, &name);
            }
        }
    }

    /// For tests: the files of all of FontCraft's entries.
    #[cfg(test)]
    pub fn all_listed() -> Vec<PathBuf> {
        let Some(k) = open() else { return vec![] };
        let ours = super::placed::set();
        entries(&k).into_iter().filter(|(n, d)| is_ours(n, d, &ours)).map(|(_, d)| PathBuf::from(d)).collect()
    }

    /// For tests: the name of FontCraft's entry for this file, if there is one.
    #[cfg(test)]
    pub fn listed(p: &Path) -> Option<String> {
        let k = open()?;
        let ours = super::placed::set();
        entries(&k).into_iter().find(|(n, d)| is_ours(n, d, &ours) && same(d, p)).map(|(n, _)| n)
    }
}

#[cfg(windows)]
fn notify() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, HWND_BROADCAST, WM_FONTCHANGE};
    unsafe {
        PostMessageW(HWND_BROADCAST, WM_FONTCHANGE, 0, 0);
    }
}

#[cfg(unix)]
fn activate_one(p: &Path) -> Result<(), String> {
    if !p.exists() {
        return Err("file not found".into());
    }
    let link = link_path(p).ok_or("no home folder")?;
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    match link.symlink_metadata() {
        // a symbolic link from an older FontCraft is turned into a hard link (below)
        Ok(m) if m.file_type().is_symlink() => {
            let _ = std::fs::remove_file(&link);
        }
        Ok(_) => return Ok(()),
        Err(_) => {}
    }
    // A hard link looks like the font file itself to every app, so "Package" in Illustrator
    // or InDesign copies the real font. Across drives (or on network folders) that isn't
    // possible and a symbolic link is used instead.
    if std::fs::hard_link(p, &link).is_ok() {
        return Ok(());
    }
    match std::os::unix::fs::symlink(p, &link) {
        Ok(()) => Ok(()),
        Err(_) => std::fs::copy(p, &link).map(|_| ()).map_err(|e| e.to_string()),
    }
}

#[cfg(unix)]
fn deactivate_one(p: &Path) -> Result<(), String> {
    let link = link_path(p).ok_or("no home folder")?;
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(unix)]
fn notify() {
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(dir) = link_dir() {
            let _ = std::process::Command::new("fc-cache").arg("-f").arg(dir).spawn();
        }
    }
}

#[cfg(not(any(windows, unix)))]
fn activate_one(_p: &Path) -> Result<(), String> {
    Err("not supported on this system".into())
}
#[cfg(not(any(windows, unix)))]
fn deactivate_one(_p: &Path) -> Result<(), String> {
    Ok(())
}
#[cfg(not(any(windows, unix)))]
fn notify() {}

/// Start FontCraft when the user logs in (so active fonts come back after a restart).
pub fn set_launch_at_login(on: bool, hidden: bool) -> Result<(), String> {
    #[cfg_attr(target_os = "macos", allow(unused_variables))]
    let flag = if hidden { " --minimized" } else { "" };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let mut cmd = std::process::Command::new("reg");
        if on {
            let val = format!("\"{}\"{flag}", exe.display());
            cmd.args(["add", key, "/v", "FontCraft", "/t", "REG_SZ", "/d", &val, "/f"]);
        } else {
            cmd.args(["delete", key, "/v", "FontCraft", "/f"]);
        }
        let st = cmd.creation_flags(0x0800_0000).status().map_err(|e| e.to_string())?;
        if !st.success() && on {
            return Err("could not change the startup setting".into());
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("no home")?;
        let dir = home.join("Library/LaunchAgents");
        let file = dir.join("app.fontcraft.plist");
        if on {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let plist = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>app.fontcraft</string><key>ProgramArguments</key><array><string>{}</string>{}</array><key>RunAtLoad</key><true/></dict></plist>\n",
                exe.display(),
                if hidden { "<string>--minimized</string>" } else { "" }
            );
            std::fs::write(&file, plist).map_err(|e| e.to_string())
        } else {
            let _ = std::fs::remove_file(&file);
            Ok(())
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .ok_or("no home")?;
        let dir = base.join("autostart");
        let file = dir.join("fontcraft.desktop");
        if on {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let s = format!(
                "[Desktop Entry]\nType=Application\nName=FontCraft\nExec=\"{}\"{}\nX-GNOME-Autostart-enabled=true\n",
                exe.display(),
                flag
            );
            std::fs::write(&file, s).map_err(|e| e.to_string())
        } else {
            let _ = std::fs::remove_file(&file);
            Ok(())
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (on, exe);
        Err("not supported".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// A copy of a font outside the system folder can be switched on and off again.
    #[test]
    fn activate_and_deactivate_a_copied_font() {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let src = ["arial.ttf", "segoeui.ttf", "calibri.ttf"].iter().map(|f| win.join("Fonts").join(f)).find(|p| p.exists());
        let Some(src) = src else { return };
        let dst = std::env::temp_dir().join("fontcraft-activation-test.ttf");
        std::fs::copy(&src, &dst).unwrap();
        assert!(activate(&[dst.clone()]).is_empty(), "activation failed");
        // the font sits in the user's font folder (under its own file name) and font list,
        // where apps that package fonts look
        let link = placed::for_original(&placed::load(), &dst).expect("not placed");
        assert_eq!(link.parent(), link_dir().as_deref());
        let fname = link.file_name().unwrap().to_string_lossy().to_string();
        assert!(fname.starts_with("fontcraft-activation-test") && !fname.starts_with(LINK_PREFIX), "{fname}");
        assert!(link.exists(), "not in the user's font folder");
        assert!(placed_files().contains(&link.to_string_lossy().to_lowercase()));
        assert_eq!(std::fs::read(&link).unwrap(), std::fs::read(&dst).unwrap());
        let name = userfonts::listed(&link).expect("not in the user's font list");
        // named exactly like Windows names an installed font: "Arial (TrueType)", nothing extra
        let full = crate::library::parse_file(&dst).pop().unwrap().full_name;
        assert!(name == format!("{full} (TrueType)") || name == format!("{full} (OpenType)"), "{name}");
        assert!(deactivate(&[dst.clone()]).is_empty(), "deactivation failed");
        assert!(userfonts::listed(&link).is_none(), "still in the user's font list");
        assert!(!link.exists(), "still in the user's font folder");
        // tidying up removes what doesn't belong to an active font, and keeps everything else
        assert!(!placed_files().contains(&link.to_string_lossy().to_lowercase()));
        assert!(activate(&[dst.clone()]).is_empty());
        let link = placed::for_original(&placed::load(), &dst).expect("not placed");
        let mine = link.to_string_lossy().to_lowercase();
        // (the cleanup itself isn't run here: it would also tidy up the fonts of this computer)
        let active: std::collections::HashSet<String> = [dst.to_string_lossy().to_lowercase()].into();
        assert!(kept(&active).contains(&mine), "an active font would be tidied away");
        assert!(!kept(&Default::default()).contains(&mine), "an inactive font would be kept");
        let others = userfonts::all_listed().len() - 1;
        forget_placed(&link);
        assert!(userfonts::listed(&link).is_none());
        assert!(!link.exists());
        assert!(!placed_files().contains(&mine));
        assert_eq!(userfonts::all_listed().len(), others, "other fonts were touched");
        let _ = std::fs::remove_file(&dst);
    }
}

/// Open the system file browser with the file selected.
pub fn show_in_folder(p: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(p).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = if p.is_dir() { p.to_path_buf() } else { p.parent().map(|d| d.to_path_buf()).unwrap_or_default() };
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}
