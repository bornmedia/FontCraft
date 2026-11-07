//! "A new version is available" – asks GitHub for the newest release.
//!
//! The GitHub project (owner/name) is baked in when the release is built on GitHub
//! (the FONTCRAFT_REPO variable, set by .github/workflows/release.yml). Builds made
//! elsewhere don't know a project and simply skip the check.

use std::sync::mpsc::{channel, Receiver};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn repo() -> Option<&'static str> {
    option_env!("FONTCRAFT_REPO").filter(|r| r.contains('/'))
}

pub fn releases_url() -> Option<String> {
    repo().map(|r| format!("https://github.com/{r}/releases/latest"))
}

#[derive(Clone, Debug)]
pub struct Update {
    pub version: String,
    pub url: String,
}

fn parse_version(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-']).map(|p| p.parse().unwrap_or(0)).take(3).collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

/// Checks in the background; the answer arrives once (None = up to date or offline).
pub fn check(ctx: egui::Context) -> Option<Receiver<Option<Update>>> {
    let repo = repo()?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let api = format!("https://api.github.com/repos/{repo}/releases/latest");
        #[allow(unused_mut)]
        let mut cmd = std::process::Command::new("curl");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd
            .args(["-sSL", "--fail", "--max-time", "20", "-H", "Accept: application/vnd.github+json", "-A", "FontCraft"])
            .arg(&api)
            .output();
        let result = out.ok().filter(|o| o.status.success()).and_then(|o| {
            let v: serde_json::Value = serde_json::from_slice(&o.stdout).ok()?;
            let tag = v.get("tag_name")?.as_str()?.to_string();
            let url = v.get("html_url").and_then(|u| u.as_str()).map(String::from).unwrap_or_else(|| releases_url().unwrap_or_default());
            is_newer(&tag, VERSION).then(|| Update { version: tag.trim_start_matches('v').to_string(), url })
        });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    Some(rx)
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn version_comparison() {
        assert!(is_newer("v1.1.0", "1.0.0"));
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
    }
}
