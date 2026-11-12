//! Deleting font files for good, or moving them out of the system's font folders
//! into a folder of the library.
//!
//! * Fonts in the person's own folders are switched off and deleted directly.
//! * System fonts need administrator rights on every OS:
//!   - Windows: a small PowerShell script removes the font's registry entry, unloads
//!     it and deletes the file. Fonts of all users (in the Windows folder) run it
//!     through the administrator prompt (UAC); fonts installed only for this user
//!     don't need it. A file Windows still holds on to is deleted at the next restart.
//!   - macOS: `/Library/Fonts` asks for the administrator password; fonts in
//!     `/System/Library/Fonts` are protected by macOS and are never touched.
//!   - Linux: fonts outside the home folder are removed through `pkexec`.

use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct Outcome {
    /// deleted (or moved away) now
    pub removed: Vec<PathBuf>,
    /// gone after the next restart (Windows only)
    pub after_restart: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
    /// the administrator prompt was cancelled
    pub cancelled: bool,
    /// where moved files ended up: (old path, new path)
    pub moved: Vec<(PathBuf, PathBuf)>,
}

/// A free file name for `name` inside `dir`; an identical file already there is reused.
fn target_for(src: &Path, dir: &Path) -> PathBuf {
    let name = src.file_name().map(|n| n.to_owned()).unwrap_or_default();
    let first = dir.join(&name);
    let same = |p: &Path| match (std::fs::metadata(src), std::fs::metadata(p)) {
        (Ok(a), Ok(b)) => a.len() == b.len(),
        _ => false,
    };
    if !first.exists() || same(&first) {
        return first;
    }
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = src.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    for k in 2..1000 {
        let p = dir.join(format!("{stem} ({k}).{ext}"));
        if !p.exists() || same(&p) {
            return p;
        }
    }
    first
}

/// Copy the files into `dir` (made if needed). Returns (original, copy, made now) and the failures.
#[allow(clippy::type_complexity)]
fn copy_into(files: &[PathBuf], dir: &Path) -> (Vec<(PathBuf, PathBuf, bool)>, Vec<(PathBuf, String)>) {
    let mut ok = vec![];
    let mut bad = vec![];
    if let Err(e) = std::fs::create_dir_all(dir) {
        for f in files {
            bad.push((f.clone(), format!("could not create {}: {e}", dir.display())));
        }
        return (ok, bad);
    }
    for f in files {
        let t = target_for(f, dir);
        if t.exists() {
            ok.push((f.clone(), t, false));
            continue;
        }
        match std::fs::copy(f, &t) {
            Ok(_) => ok.push((f.clone(), t, true)),
            Err(e) => bad.push((f.clone(), format!("could not copy it: {e}"))),
        }
    }
    (ok, bad)
}

/// Delete (or, with `move_to`, move) font files. `system` says they come from the system's font folders.
pub fn remove_files(files: &[PathBuf], move_to: Option<&Path>, system: bool) -> Outcome {
    let mut out = Outcome::default();
    let mut todo: Vec<PathBuf> = files.to_vec();
    let mut copies = vec![];
    if let Some(dir) = move_to {
        let (ok, bad) = copy_into(&todo, dir);
        out.failed.extend(bad);
        todo = ok.iter().map(|(a, _, _)| a.clone()).collect();
        copies = ok;
    }
    if todo.is_empty() {
        return out;
    }
    if system {
        remove_system(&todo, &mut out);
    } else {
        // Switch them off first, or Windows keeps the files locked.
        crate::activation::deactivate(&todo);
        for f in &todo {
            match std::fs::remove_file(f) {
                Ok(()) => out.removed.push(f.clone()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => out.removed.push(f.clone()),
                Err(e) => out.failed.push((f.clone(), friendly(&e))),
            }
        }
    }
    // A move whose original could not be deleted is undone, so the font isn't there twice.
    // (Files that go away at the next restart count as moved.)
    for (src, dst, made) in copies {
        let done = out.removed.contains(&src) || out.after_restart.contains(&src);
        if done {
            out.moved.push((src, dst));
        } else if made {
            let _ = std::fs::remove_file(&dst);
        }
    }
    out
}

fn friendly(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "you don't have permission to delete it (read-only folder or drive?)".into(),
        _ => e.to_string(),
    }
}

// ------------------------------------------------------------------ the fonts the OS needs

/// Fonts the operating system itself uses for its menus, dialogs and symbols.
pub fn is_os_font(path: &Path, family: &str) -> bool {
    let f = family.to_lowercase();
    let starts = |list: &[&str]| list.iter().any(|n| f == *n || f.starts_with(&format!("{n} ")));
    if cfg!(windows) {
        starts(&[
            "segoe ui", "segoe ui variable", "segoe ui emoji", "segoe ui symbol", "segoe ui historic", "segoe mdl2 assets",
            "segoe fluent icons", "segoe print", "segoe script", "arial", "times new roman", "courier new", "calibri",
            "cambria", "cambria math", "consolas", "tahoma", "verdana", "marlett", "symbol", "wingdings", "webdings",
            "microsoft sans serif", "ms gothic", "yu gothic", "yu gothic ui", "malgun gothic", "microsoft yahei",
            "microsoft yahei ui", "microsoft jhenghei", "microsoft jhenghei ui", "simsun", "nsimsun", "lucida console",
            "lucida sans unicode", "georgia", "trebuchet ms", "gadugi", "nirmala ui", "ebrima", "leelawadee ui",
            "javanese text", "myanmar text", "mongolian baiti", "mv boli", "sylfaen", "microsoft himalaya",
            "microsoft yi baiti", "microsoft tai le", "microsoft new tai lue", "microsoft phagspa", "cascadia code",
            "cascadia mono", "sitka", "bahnschrift", "ink free",
        ])
    } else if cfg!(target_os = "macos") {
        path.starts_with("/System/Library/Fonts")
            || f.starts_with('.')
            || starts(&[
                "sf pro", "sf compact", "sf mono", "sf arabic", "new york", "helvetica", "helvetica neue", "lucida grande",
                "apple color emoji", "apple symbols", "menlo", "monaco", "geneva", "times", "courier", "pingfang sc",
                "pingfang tc", "pingfang hk", "hiragino sans", "hiragino kaku gothic pron", "apple sd gothic neo", "symbol",
                "zapf dingbats", "avenir", "avenir next",
            ])
    } else {
        starts(&[
            "dejavu sans", "dejavu serif", "dejavu sans mono", "noto sans", "noto serif", "noto color emoji", "noto mono",
            "liberation sans", "liberation serif", "liberation mono", "cantarell", "ubuntu", "ubuntu mono", "droid sans",
            "adwaita sans", "adwaita mono", "bitstream vera sans", "freesans", "freeserif", "freemono", "nimbus sans",
            "nimbus roman", "nimbus mono ps", "urw gothic", "source code pro", "inter",
        ])
    }
}

/// Fonts that can't be deleted at all (macOS protects its own fonts).
pub fn is_protected(path: &Path) -> bool {
    cfg!(target_os = "macos") && path.starts_with("/System/Library")
}

/// What the person should know before deleting system fonts on this OS.
pub fn os_warning() -> &'static str {
    if cfg!(windows) {
        "Windows uses some of its fonts itself – Segoe UI for menus and dialogs, Segoe Fluent Icons and Segoe MDL2 \
         Assets for symbols, plus Arial, Calibri, Consolas, Tahoma and others. Deleting them can leave menus, buttons \
         and apps with missing letters or empty boxes, and a Windows update may put them back. Fonts installed for all \
         users need administrator permission; a font Windows is still using is removed at the next restart."
    } else if cfg!(target_os = "macos") {
        "macOS needs its own fonts (SF Pro, Helvetica, Lucida Grande, Menlo, Apple Color Emoji, …) for its menus and \
         apps. Fonts in the System folder are protected by macOS and can't be deleted or moved – FontCraft leaves them \
         out. Fonts in /Library/Fonts need your administrator password."
    } else {
        "Your Linux desktop needs some fonts itself (DejaVu, Noto, Liberation, Cantarell, Ubuntu, …) for menus and \
         apps. Fonts in /usr/share/fonts belong to your distribution's packages: deleting them needs administrator \
         rights, can break the desktop, and the package manager may put them back with the next update."
    }
}

/// Fonts that need administrator rights to be deleted.
#[allow(dead_code)]
fn needs_admin(p: &Path) -> bool {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    #[cfg(windows)]
    {
        let _ = &home;
        let user = std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
        !user.map(|u| p.starts_with(u)).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        !home.map(|h| p.starts_with(h)).unwrap_or(false)
    }
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(windows)]
pub(crate) fn windows_script(files: &[PathBuf], result: &Path) -> String {
    let list: Vec<String> = files.iter().map(|f| ps_quote(&f.to_string_lossy())).collect();
    format!(
        r#"$ErrorActionPreference = 'Continue'
$files = @({files})
$out = {out}
Add-Type -Namespace FontCraft -Name Native -MemberDefinition @'
[DllImport("gdi32.dll", CharSet = CharSet.Unicode)] public static extern int RemoveFontResourceW(string file);
[DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] public static extern bool MoveFileExW(string a, string b, int flags);
'@
$keys = @('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts', 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts')
$skip = @('PSPath', 'PSParentPath', 'PSChildName', 'PSDrive', 'PSProvider')
$res = New-Object System.Collections.Generic.List[string]
foreach ($f in $files) {{
  $name = [IO.Path]::GetFileName($f)
  foreach ($k in $keys) {{
    $item = Get-ItemProperty -LiteralPath $k -ErrorAction SilentlyContinue
    if ($item) {{
      foreach ($p in $item.PSObject.Properties) {{
        if ($skip -notcontains $p.Name -and ("$($p.Value)" -ieq $name -or "$($p.Value)" -ieq $f)) {{
          Remove-ItemProperty -LiteralPath $k -Name $p.Name -ErrorAction SilentlyContinue
        }}
      }}
    }}
  }}
  for ($i = 0; $i -lt 16; $i++) {{ if ([FontCraft.Native]::RemoveFontResourceW($f) -eq 0) {{ break }} }}
  if (-not (Test-Path -LiteralPath $f)) {{ $res.Add("ok`t$f"); continue }}
  try {{
    Remove-Item -LiteralPath $f -Force -ErrorAction Stop
    $res.Add("ok`t$f")
  }} catch {{
    if ([FontCraft.Native]::MoveFileExW($f, $null, 4)) {{ $res.Add("later`t$f") }}
    else {{ $res.Add("fail`t$f`t$($_.Exception.Message)") }}
  }}
}}
[IO.File]::WriteAllLines($out, $res, (New-Object System.Text.UTF8Encoding($false)))
"#,
        files = list.join(", "),
        out = ps_quote(&result.to_string_lossy())
    )
}

#[cfg(windows)]
fn run_windows_script(files: &[PathBuf], elevated: bool, out: &mut Outcome) {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let stamp = format!("{}-{}", std::process::id(), crate::activation::fnv(&format!("{:?}{files:?}", std::time::SystemTime::now())));
    let tmp = std::env::temp_dir();
    let script = tmp.join(format!("fontcraft-remove-{stamp}.ps1"));
    let result = tmp.join(format!("fontcraft-remove-{stamp}.txt"));
    let _ = std::fs::remove_file(&result);
    // a byte-order mark makes Windows PowerShell read the script as UTF-8 (names with accents)
    let mut body = vec![0xEF, 0xBB, 0xBF];
    body.extend_from_slice(windows_script(files, &result).as_bytes());
    if let Err(e) = std::fs::write(&script, body) {
        for f in files {
            out.failed.push((f.clone(), format!("could not prepare the removal: {e}")));
        }
        return;
    }
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass"]).creation_flags(NO_WINDOW);
    if elevated {
        let inner = format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File \"{}\"", script.display());
        let launcher = format!(
            "try {{ Start-Process -FilePath 'powershell.exe' -ArgumentList {} -Verb RunAs -Wait -WindowStyle Hidden -ErrorAction Stop; exit 0 }} catch {{ exit 5 }}",
            ps_quote(&inner)
        );
        cmd.args(["-Command", &launcher]);
    } else {
        cmd.arg("-File").arg(&script);
    }
    let status = cmd.status();
    let text = std::fs::read_to_string(&result).unwrap_or_default();
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&result);
    if text.trim().is_empty() {
        let cancelled = elevated && matches!(status, Ok(s) if s.code() == Some(5));
        out.cancelled |= cancelled;
        for f in files {
            let why = if cancelled { "administrator permission was not given".to_string() } else { "Windows did not remove it".to_string() };
            out.failed.push((f.clone(), why));
        }
        return;
    }
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.trim_start_matches('\u{feff}').splitn(3, '\t').collect();
        if parts.len() < 2 {
            continue;
        }
        let p = PathBuf::from(parts[1]);
        seen.insert(p.clone());
        match parts[0] {
            "ok" => out.removed.push(p),
            "later" => out.after_restart.push(p),
            _ => out.failed.push((p, parts.get(2).unwrap_or(&"Windows refused").to_string())),
        }
    }
    for f in files {
        if !seen.contains(f) {
            out.failed.push((f.clone(), "Windows did not remove it".into()));
        }
    }
}

#[cfg(windows)]
fn remove_system(files: &[PathBuf], out: &mut Outcome) {
    let (admin, user): (Vec<PathBuf>, Vec<PathBuf>) = files.iter().cloned().partition(|p| needs_admin(p));
    if !user.is_empty() {
        run_windows_script(&user, false, out);
    }
    if !admin.is_empty() {
        run_windows_script(&admin, true, out);
    }
    crate::activation::notify_font_change();
}

// ------------------------------------------------------------------ macOS and Linux

#[cfg(unix)]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn remove_system(files: &[PathBuf], out: &mut Outcome) {
    let mut admin = vec![];
    for f in files {
        if is_protected(f) {
            out.failed.push((f.clone(), "macOS protects its own fonts; they can't be deleted".into()));
            continue;
        }
        crate::activation::deactivate(std::slice::from_ref(f));
        match std::fs::remove_file(f) {
            Ok(()) => out.removed.push(f.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => out.removed.push(f.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => admin.push(f.clone()),
            Err(e) => out.failed.push((f.clone(), e.to_string())),
        }
    }
    if !admin.is_empty() {
        let rm = format!("rm -f -- {}", admin.iter().map(|f| sh_quote(&f.to_string_lossy())).collect::<Vec<_>>().join(" "));
        let status = if cfg!(target_os = "macos") {
            let esc = rm.replace('\\', "\\\\").replace('"', "\\\"");
            std::process::Command::new("osascript")
                .arg("-e")
                .arg(format!("do shell script \"{esc}\" with administrator privileges"))
                .status()
        } else {
            let mut c = std::process::Command::new("pkexec");
            c.arg("rm").arg("-f").arg("--").args(&admin);
            c.status()
        };
        let ok = matches!(&status, Ok(s) if s.success());
        for f in admin {
            if !f.exists() {
                out.removed.push(f);
            } else {
                if !ok {
                    out.cancelled = true;
                }
                let why = if ok { "the system refused" } else { "administrator permission was not given" };
                out.failed.push((f, why.into()));
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = std::process::Command::new("fc-cache").arg("-f").spawn();
    }
}

#[cfg(not(any(windows, unix)))]
fn remove_system(files: &[PathBuf], out: &mut Outcome) {
    for f in files {
        out.failed.push((f.clone(), "not supported on this system".into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fontcraft-remove-test-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn deletes_folder_fonts() {
        let d = temp("delete");
        let a = d.join("A.ttf");
        std::fs::write(&a, b"font a").unwrap();
        let out = remove_files(&[a.clone()], None, false);
        assert_eq!(out.removed, vec![a.clone()]);
        assert!(out.failed.is_empty());
        assert!(!a.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn moves_into_a_folder_and_keeps_both_names() {
        let d = temp("move");
        let src = d.join("src");
        let dst = d.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        let a = src.join("Same.ttf");
        std::fs::write(&a, b"new font").unwrap();
        std::fs::write(dst.join("Same.ttf"), b"another, longer font").unwrap();
        let out = remove_files(&[a.clone()], Some(&dst), false);
        assert!(out.failed.is_empty(), "{:?}", out.failed);
        assert!(!a.exists());
        assert_eq!(out.moved.len(), 1);
        assert_eq!(out.moved[0].1, dst.join("Same (2).ttf"));
        assert_eq!(std::fs::read(dst.join("Same (2).ttf")).unwrap(), b"new font");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn knows_the_fonts_the_os_needs() {
        if cfg!(windows) {
            assert!(is_os_font(Path::new(r"C:\Windows\Fonts\segoeui.ttf"), "Segoe UI"));
            assert!(is_os_font(Path::new(r"C:\Windows\Fonts\arial.ttf"), "Arial"));
            assert!(!is_os_font(Path::new(r"C:\Windows\Fonts\x.ttf"), "Arialish Display"));
        }
        assert!(!is_os_font(Path::new("/x/Lobster.ttf"), "Lobster"));
    }

    /// The Windows removal script runs (without administrator rights) on a font in a temporary
    /// folder: it unloads and deletes the file and writes its report.
    #[cfg(windows)]
    #[test]
    fn windows_script_deletes_a_font() {
        let d = temp("script");
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let Some(src) = ["arial.ttf", "segoeui.ttf"].iter().map(|f| win.join("Fonts").join(f)).find(|p| p.exists()) else { return };
        let f = d.join("Test font with 'quote' é.ttf");
        std::fs::copy(&src, &f).unwrap();
        assert!(crate::activation::activate(&[f.clone()]).is_empty());
        let mut out = Outcome::default();
        run_windows_script(&[f.clone()], false, &mut out);
        assert_eq!(out.removed, vec![f.clone()], "{out:?}");
        assert!(!f.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
