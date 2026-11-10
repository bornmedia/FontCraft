//! Making fonts available to every other program (Word, Photoshop, Affinity, ...)
//! without installing them permanently.
//!
//! * Windows: the font file is registered for the current login session with
//!   `AddFontResourceExW` and all windows are told that the font list changed.
//!   FontCraft re-registers active fonts every time it starts.
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

#[cfg(windows)]
fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn activate_one(p: &Path) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::AddFontResourceExW;
    if !p.exists() {
        return Err("file not found".into());
    }
    let w = wide(p);
    let n = unsafe { AddFontResourceExW(w.as_ptr(), 0, std::ptr::null()) };
    if n == 0 {
        Err("Windows refused to load this font".into())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn deactivate_one(p: &Path) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::RemoveFontResourceExW;
    let w = wide(p);
    // A font may have been registered more than once (e.g. FontCraft restarted
    // within the same session), so remove it until Windows says it is gone.
    for _ in 0..16 {
        let ok = unsafe { RemoveFontResourceExW(w.as_ptr(), 0, std::ptr::null()) };
        if ok == 0 {
            break;
        }
    }
    Ok(())
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
    if link.symlink_metadata().is_ok() {
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
        assert!(deactivate(&[dst.clone()]).is_empty(), "deactivation failed");
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
