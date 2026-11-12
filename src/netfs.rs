//! Font folders on the network: shared folders on a server or NAS, mapped drives,
//! mounted shares.
//!
//! * Windows: `\\server\share\folder` addresses and mapped drive letters (Z:\) work as they are.
//! * macOS: a share is connected in the Finder (or here, from an `smb://` address) and then
//!   lives under `/Volumes`.
//! * Linux: shares are mounted by the system (`/mnt`, `/media`) or by the file manager
//!   (`/run/user/<id>/gvfs/...`); `smb://` addresses are connected with `gio mount`.
//!
//! A network folder may be unreachable (laptop away from the office, server asleep), so
//! FontCraft checks it with a time limit and keeps its active fonts until it is back.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Is this folder on another computer?
pub fn is_network_path(p: &Path) -> bool {
    let s = p.to_string_lossy();
    #[cfg(windows)]
    {
        if s.starts_with(r"\\") && !s.starts_with(r"\\?\") && !s.starts_with(r"\\.\") {
            return true;
        }
        if s.starts_with(r"\\?\UNC\") {
            return true;
        }
        let b = s.as_bytes();
        if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
            use std::os::windows::ffi::OsStrExt;
            let root: Vec<u16> = std::ffi::OsStr::new(&format!("{}:\\", b[0] as char)).encode_wide().chain([0]).collect();
            // DRIVE_REMOTE = 4
            return unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr()) } == 4;
        }
        false
    }
    #[cfg(not(windows))]
    {
        if s.contains("/gvfs/") || s.starts_with("smb://") {
            return true;
        }
        mount_type(p).map(|t| is_network_fs(&t)).unwrap_or(false)
    }
}

#[cfg(not(windows))]
fn is_network_fs(t: &str) -> bool {
    let t = t.to_lowercase();
    ["nfs", "cifs", "smb", "smbfs", "smb3", "afpfs", "webdav", "davfs", "sshfs", "fuse.sshfs", "9p", "afs", "ceph", "glusterfs", "fuse.rclone", "fuse.gvfsd-fuse", "ftp"]
        .iter()
        .any(|n| t.starts_with(n))
}

/// The file system type of the mount a path lives on (from `mount` / `/proc/mounts`).
#[cfg(not(windows))]
fn mount_type(p: &Path) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    #[cfg(target_os = "linux")]
    let text = std::fs::read_to_string("/proc/mounts").ok()?;
    #[cfg(not(target_os = "linux"))]
    let text = String::from_utf8_lossy(&std::process::Command::new("mount").output().ok()?.stdout).to_string();
    for line in text.lines() {
        #[cfg(target_os = "linux")]
        let (point, kind) = {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 3 {
                continue;
            }
            (f[1].replace("\\040", " "), f[2].to_string())
        };
        #[cfg(not(target_os = "linux"))]
        let (point, kind) = {
            // "//user@server/share on /Volumes/share (smbfs, nodev, ...)"
            let Some((_, rest)) = line.split_once(" on ") else { continue };
            let Some((point, opts)) = rest.rsplit_once(" (") else { continue };
            (point.to_string(), opts.split(',').next().unwrap_or("").trim().to_string())
        };
        if p.starts_with(&point) && best.as_ref().map(|b| point.len() > b.0).unwrap_or(true) {
            best = Some((point.len(), kind));
        }
    }
    best.map(|b| b.1)
}

/// Can the folder be reached within `limit`? (A server that is away can make the system
/// wait for half a minute, so the look happens on its own thread.)
pub fn reachable(p: &Path, limit: Duration) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let p = p.to_path_buf();
    std::thread::spawn(move || {
        let ok = std::fs::read_dir(&p).is_ok();
        let _ = tx.send(ok);
    });
    rx.recv_timeout(limit).unwrap_or(false)
}

/// Turn what the person typed (`smb://server/share`, `\\server\share`, `file://…`, a path
/// with quotes around it) into a folder path this system understands.
pub fn normalize_address(input: &str) -> String {
    let mut s = input.trim().trim_matches('"').trim_matches('\'').trim().to_string();
    if let Some(rest) = s.strip_prefix("file://") {
        s = percent_decode(rest);
        #[cfg(windows)]
        {
            // file://server/share -> \\server\share ; file:///C:/x -> C:/x
            s = match s.strip_prefix('/') {
                Some(local) => local.to_string(),
                None => format!("//{s}"),
            };
        }
    }
    #[cfg(windows)]
    {
        if let Some(rest) = s.strip_prefix("smb://").or_else(|| s.strip_prefix("cifs://")) {
            // smb://user@server/share -> \\server\share
            let rest = rest.rsplit_once('@').map(|(_, r)| r).unwrap_or(rest);
            s = format!("//{}", percent_decode(rest));
        }
        if s.starts_with("//") {
            s = s.replace('/', "\\");
        }
        while s.len() > 3 && s.ends_with('\\') {
            s.pop();
        }
    }
    #[cfg(not(windows))]
    {
        if s.starts_with(r"\\") {
            // \\server\share typed on a Mac or Linux: the same share as smb://
            s = format!("smb:{}", s.replace('\\', "/"));
        }
        while s.len() > 1 && s.ends_with('/') && !s.ends_with("://") {
            s.pop();
        }
    }
    s
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// For a `smb://` (or `afp://`, `nfs://`) address on macOS or Linux: connect the share and
/// return the local folder it appears in. Other input is returned as a path unchanged.
pub fn connect(address: &str, limit: Duration) -> Result<PathBuf, String> {
    let a = normalize_address(address);
    if a.is_empty() {
        return Err("Type the address of a folder first".into());
    }
    let is_url = a.contains("://");
    if !is_url {
        return Ok(PathBuf::from(a));
    }
    #[cfg(windows)]
    {
        let _ = limit;
        Err("Use an address like \\\\server\\share\\fonts on Windows".into())
    }
    #[cfg(target_os = "macos")]
    {
        // "smb://server/share/sub/dir": the Finder connects the share at /Volumes/share
        let after = a.split_once("://").map(|x| x.1).unwrap_or("");
        let after = after.rsplit_once('@').map(|x| x.1).unwrap_or(after);
        let mut parts = after.split('/').filter(|p| !p.is_empty());
        let _server = parts.next();
        let share = parts.next().ok_or("The address needs a shared folder after the server name, like smb://server/fonts")?;
        let share = percent_decode(share);
        let rest: Vec<String> = parts.map(percent_decode).collect();
        let url = {
            let (scheme, _) = a.split_once("://").unwrap();
            let host_share: Vec<&str> = after.split('/').filter(|p| !p.is_empty()).take(2).collect();
            format!("{scheme}://{}", host_share.join("/"))
        };
        let existing = |prefix: &str| -> Option<PathBuf> {
            std::fs::read_dir("/Volumes").ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
                p.file_name().map(|n| n.to_string_lossy().to_string()).map(|n| n == prefix || n.starts_with(&format!("{prefix}-"))).unwrap_or(false)
            })
        };
        let target = |root: PathBuf| rest.iter().fold(root, |p, r| p.join(r));
        if let Some(v) = existing(&share) {
            if is_network_path(&v) {
                return Ok(target(v));
            }
        }
        let _ = std::process::Command::new("open").arg(&url).status();
        let start = std::time::Instant::now();
        while start.elapsed() < limit {
            if let Some(v) = existing(&share) {
                return Ok(target(v));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(format!("{url} was not connected. Connect it in the Finder (Go ▸ Connect to Server…) and try again."))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = limit;
        let _ = std::process::Command::new("gio").arg("mount").arg(&a).status();
        // ask GIO where it put the share
        let out = std::process::Command::new("gio").arg("info").arg("-a").arg("standard::name").arg(&a).output();
        if let Ok(o) = &out {
            let text = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = text.lines().find(|l| l.trim_start().starts_with("local path:")) {
                let p = line.split_once(':').map(|x| x.1.trim()).unwrap_or("");
                if !p.is_empty() {
                    return Ok(PathBuf::from(p));
                }
            }
        }
        // the usual gvfs place: /run/user/<uid>/gvfs/smb-share:server=x,share=y/rest
        let after = a.split_once("://").map(|x| x.1).unwrap_or("");
        let after = after.rsplit_once('@').map(|x| x.1).unwrap_or(after);
        let parts: Vec<String> = after.split('/').filter(|p| !p.is_empty()).map(percent_decode).collect();
        if parts.len() >= 2 {
            let uid = std::process::Command::new("id").arg("-u").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
            let base = PathBuf::from(format!("/run/user/{uid}/gvfs"));
            let name = format!("smb-share:server={},share={}", parts[0].to_lowercase(), parts[1].to_lowercase());
            let p = parts[2..].iter().fold(base.join(name), |p, r| p.join(r));
            if p.exists() {
                return Ok(p);
            }
        }
        Err(format!("{a} could not be connected. Open it once in your file manager (it may ask for a password) and try again."))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = limit;
        Err("Network addresses are not supported on this system".into())
    }
}

/// Example addresses shown in the "Add a network folder" window.
pub fn examples() -> &'static str {
    if cfg!(windows) {
        "\\\\server\\fonts   ·   \\\\192.168.1.20\\design\\fonts   ·   Z:\\Fonts (a mapped drive)"
    } else if cfg!(target_os = "macos") {
        "smb://server/fonts   ·   /Volumes/fonts (a share connected in the Finder)"
    } else {
        "smb://server/fonts   ·   /mnt/fonts   ·   /media/nas/fonts"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn understands_addresses() {
        if cfg!(windows) {
            assert_eq!(normalize_address(" \"\\\\server\\fonts\\\" "), r"\\server\fonts");
            assert_eq!(normalize_address("smb://anna@nas/Design%20Fonts/"), r"\\nas\Design Fonts");
            assert_eq!(normalize_address("//nas/fonts"), r"\\nas\fonts");
            assert!(is_network_path(Path::new(r"\\nas\fonts")));
            assert!(!is_network_path(Path::new(r"C:\Windows\Fonts")));
        } else {
            assert_eq!(normalize_address("\\\\nas\\fonts"), "smb://nas/fonts");
            assert_eq!(normalize_address("/mnt/fonts/"), "/mnt/fonts");
        }
        assert!(reachable(&std::env::temp_dir(), Duration::from_secs(3)));
        assert!(!reachable(Path::new("/this/folder/is/not/there"), Duration::from_secs(3)));
    }

    /// A font in a folder reached over the network (this computer's own C$ share, when it is
    /// open): scanned, activated, deactivated and deleted through its \\server\share address.
    #[cfg(windows)]
    #[test]
    fn fonts_in_a_network_folder() {
        let local = std::env::temp_dir().join("fontcraft-net-test");
        let _ = std::fs::remove_dir_all(&local);
        std::fs::create_dir_all(&local).unwrap();
        let s = local.to_string_lossy().to_string();
        let unc = PathBuf::from(format!(r"\\localhost\{}${}", &s[..1], &s[2..]));
        if !reachable(&unc, Duration::from_secs(5)) {
            return; // no admin share on this computer
        }
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let Some(src) = ["arial.ttf", "segoeui.ttf"].iter().map(|f| win.join("Fonts").join(f)).find(|p| p.exists()) else { return };
        std::fs::copy(&src, local.join("Net.ttf")).unwrap();
        let f = unc.join("Net.ttf");
        assert!(is_network_path(&unc));
        assert!(!crate::library::parse_file(&f).is_empty(), "font over the network can't be read");
        assert!(crate::activation::activate(&[f.clone()]).is_empty(), "activation over the network failed");
        assert!(crate::activation::deactivate(&[f.clone()]).is_empty());
        let out = crate::remove::remove_files(&[f.clone()], None, false);
        assert!(out.failed.is_empty(), "{:?}", out.failed);
        assert!(!local.join("Net.ttf").exists());
        let _ = std::fs::remove_dir_all(&local);
    }
}
