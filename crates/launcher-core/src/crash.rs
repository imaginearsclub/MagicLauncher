//! Crash helper: after the game exits abnormally, find the crash report / log from that run,
//! work out the likely cause, and prepare a scrubbed log the player can copy or share.
//!
//! Scrubbing (before anything leaves the PC): the user's home folder in paths (it contains their
//! Windows/macOS user name) and any session token. mclo.gs additionally strips IPs.

use crate::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    OutOfMemory,
    MemorySetting,
    MissingDependency,
    ModLoading,
    GraphicsDriver,
    JavaVersion,
    Unknown,
}

impl Cause {
    /// Short stable name, e.g. for anonymous telemetry ("crash signature").
    pub fn signature(self) -> &'static str {
        match self {
            Cause::OutOfMemory => "out_of_memory",
            Cause::MemorySetting => "memory_setting",
            Cause::MissingDependency => "missing_dependency",
            Cause::ModLoading => "mod_loading",
            Cause::GraphicsDriver => "graphics_driver",
            Cause::JavaVersion => "java_version",
            Cause::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CrashInfo {
    pub cause: Cause,
    pub title: String,
    pub hint: String,
    /// The mod most likely involved, if the log names one.
    pub culprit: Option<String>,
    pub exit_code: Option<i32>,
    /// The crash report written by this run, if any.
    pub crash_report: Option<PathBuf>,
}

/// Newest file in `<game_dir>/crash-reports` written after `since`.
pub fn new_crash_report(game_dir: &Path, since: SystemTime) -> Option<PathBuf> {
    std::fs::read_dir(game_dir.join("crash-reports"))
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?;
            (m >= since).then(|| (m, e.path()))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p)
}

fn tail(text: &str, max_lines: usize) -> &str {
    let mut count = 0;
    for (i, c) in text.char_indices().rev() {
        if c == '\n' {
            count += 1;
            if count == max_lines {
                return &text[i + 1..];
            }
        }
    }
    text
}

/// Text to classify and share: the crash report (if any), then the end of the game output.
pub fn collect_log(game_dir: &Path, crash_report: Option<&Path>) -> String {
    let mut out = String::new();
    if let Some(r) = crash_report.and_then(|p| std::fs::read_to_string(p).ok()) {
        out.push_str(&r);
        out.push_str("\n\n---- launcher-game.log (end) ----\n");
    }
    let log = std::fs::read_to_string(game_dir.join("logs").join("launcher-game.log"))
        .or_else(|_| std::fs::read_to_string(game_dir.join("logs").join("latest.log")))
        .unwrap_or_default();
    out.push_str(tail(&log, 1500));
    out
}

/// Pull a quoted or bracketed token after `marker`, e.g. `Mod ID: 'foo'` → `foo`.
fn after(text: &str, marker: &str) -> Option<String> {
    let i = text.find(marker)? + marker.len();
    let rest = text[i..].trim_start();
    let rest = rest.trim_start_matches(['\'', '"', '[', '(']);
    let end = rest
        .find(|c: char| {
            c == '\'' || c == '"' || c == ']' || c == ')' || c.is_whitespace() || c == ','
        })
        .unwrap_or(rest.len());
    let s = &rest[..end];
    (!s.is_empty()).then(|| s.to_string())
}

/// Classify a crash from its log text.
pub fn classify(text: &str, exit_code: Option<i32>, crash_report: Option<PathBuf>) -> CrashInfo {
    let has = |s: &str| text.contains(s);
    let (cause, culprit) = if has("java.lang.OutOfMemoryError") {
        (Cause::OutOfMemory, None)
    } else if has("Could not reserve enough space for object heap")
        || has("Invalid maximum heap size")
        || has("Initial heap size set to a larger value")
    {
        (Cause::MemorySetting, None)
    } else if has("Missing or unsupported mandatory dependencies") {
        (
            Cause::MissingDependency,
            after(text, "Mod ID:").or_else(|| after(text, "requires")),
        )
    } else if has("UnsupportedClassVersionError")
        || has("has been compiled by a more recent version of the Java Runtime")
    {
        (Cause::JavaVersion, None)
    } else if has("ModLoadingException")
        || has("Mod loading has failed")
        || has("Failed to load mod")
        || has("encountered an error during the")
    {
        (
            Cause::ModLoading,
            after(text, "Mod ID:")
                .or_else(|| after(text, "Mod File:"))
                .or_else(|| after(text, "Failed to load mod")),
        )
    } else if has("EXCEPTION_ACCESS_VIOLATION")
        || has("Pixel format not accelerated")
        || has("GLFW error 65542")
        || has("nvoglv")
        || has("atio6axx")
        || has("ig9icd")
        || has("ig7icd")
        || has("ig75icd")
    {
        (Cause::GraphicsDriver, None)
    } else {
        (Cause::Unknown, None)
    };

    let (title, hint) = match cause {
        Cause::OutOfMemory => (
            "Minecraft ran out of memory",
            "Give it more memory in Settings (untick \"Choose memory automatically\"), and close other programs while playing. Big windows like 4K need about 2 GB extra.",
        ),
        Cause::MemorySetting => (
            "Java couldn't reserve the memory it was given",
            "Lower the memory in Settings, or tick \"Choose memory automatically\".",
        ),
        Cause::MissingDependency => (
            "A mod is missing something it needs",
            "Use Settings → Repair game files. If it keeps happening, share the log with staff.",
        ),
        Cause::ModLoading => (
            "A mod failed to load",
            "Use Settings → Repair game files. If it keeps happening, share the log with staff.",
        ),
        Cause::GraphicsDriver => (
            "The graphics driver crashed",
            "Update your graphics driver (NVIDIA, AMD or Intel). If you use shaders, try without them.",
        ),
        Cause::JavaVersion => (
            "Wrong Java version",
            "Use Settings → Repair game files to reinstall the launcher's Java.",
        ),
        Cause::Unknown => (
            "Minecraft closed unexpectedly",
            "Try again. If it keeps happening, copy or share the log and send it to staff.",
        ),
    };
    CrashInfo {
        cause,
        title: title.into(),
        hint: hint.into(),
        culprit,
        exit_code,
        crash_report,
    }
}

/// Remove personal data before a log leaves the PC: the home folder (contains the OS user name)
/// and session tokens.
pub fn scrub(text: &str) -> String {
    let mut out = text.to_string();
    for var in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(var) {
            if home.len() > 3 {
                out = out.replace(&home, "<home>");
                out = out.replace(&home.replace('\\', "/"), "<home>");
                out = out.replace(&home.replace('\\', "\\\\"), "<home>");
            }
        }
    }
    for user_var in ["USERNAME", "USER"] {
        if let Ok(u) = std::env::var(user_var) {
            if u.len() > 2 {
                for prefix in ["\\Users\\", "/Users/", "/home/"] {
                    out = out.replace(&format!("{prefix}{u}"), &format!("{prefix}<user>"));
                }
            }
        }
    }
    redact_after(&redact_after(&out, "--accessToken "), "token:")
}

/// Replace the token that follows each `marker` with `<redacted>`.
fn redact_after(text: &str, marker: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(marker) {
        let (head, tail) = rest.split_at(i + marker.len());
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || c == ',' || c == ')')
            .unwrap_or(tail.len());
        if end > 0 {
            out.push_str("<redacted>");
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Upload a (scrubbed) log to mclo.gs; returns the public link.
pub async fn share_mclogs(client: &reqwest::Client, scrubbed: &str) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Resp {
        success: bool,
        url: Option<String>,
        error: Option<String>,
    }
    let r: Resp = client
        .post("https://api.mclo.gs/1/log")
        .form(&[("content", scrubbed)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    match (r.success, r.url) {
        (true, Some(url)) => Ok(url),
        _ => Err(crate::Error::other(format!(
            "mclo.gs refused the log: {}",
            r.error.unwrap_or_else(|| "unknown error".into())
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_crashes() {
        let c = |t: &str| classify(t, Some(1), None).cause;
        assert_eq!(
            c("Exception in thread \"main\" java.lang.OutOfMemoryError: Java heap space"),
            Cause::OutOfMemory
        );
        assert_eq!(c("Error occurred during initialization of VM\nCould not reserve enough space for object heap"), Cause::MemorySetting);
        assert_eq!(c("A fatal error has been detected...\n# EXCEPTION_ACCESS_VIOLATION (0xc0000005)\n# C  [atio6axx.dll+0x1]"), Cause::GraphicsDriver);
        assert_eq!(c("java.lang.UnsupportedClassVersionError: foo has been compiled by a more recent version"), Cause::JavaVersion);
        assert_eq!(c("everything is fine"), Cause::Unknown);
    }

    #[test]
    fn finds_the_mod() {
        let t = "net.neoforged.fml.ModLoadingException: Mod loading has failed\n\tMod File: /x/mods/foo-1.2.jar\n\tMod ID: 'foo', Requested by: 'bar'";
        let info = classify(t, Some(1), None);
        assert_eq!(info.cause, Cause::ModLoading);
        assert_eq!(info.culprit.as_deref(), Some("foo"));

        let dep = "Missing or unsupported mandatory dependencies:\n\tMod ID: 'architectury', Requested by: 'waystones'";
        let info = classify(dep, Some(1), None);
        assert_eq!(info.cause, Cause::MissingDependency);
        assert_eq!(info.culprit.as_deref(), Some("architectury"));
    }

    #[test]
    fn scrubs_tokens() {
        let s = scrub(
            "args --accessToken eyJhbG.secret.x --version 1\n(Session ID is token:abc123:def)",
        );
        assert!(!s.contains("eyJhbG"));
        assert!(!s.contains("abc123"));
        assert!(s.contains("--accessToken <redacted>"));
    }

    #[test]
    fn scrubs_home_folder() {
        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            let s = scrub(&format!(
                "loading {home}/stuff and {}",
                home.replace('\\', "/")
            ));
            assert!(!s.contains(&home), "{s}");
        }
    }

    #[test]
    fn tail_keeps_last_lines() {
        assert_eq!(tail("a\nb\nc\nd", 2), "c\nd");
        assert_eq!(tail("a\nb", 5), "a\nb");
    }
}
