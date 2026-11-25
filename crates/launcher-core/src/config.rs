//! Build-time branding/endpoints and per-user settings.
//!
//! Endpoints and service IDs can be overridden at compile time with environment variables
//! so CI can inject real values without committing them:
//!   MAGIC_PACK_URL              URL of the hosted pack.json
//!   MAGIC_MS_CLIENT_ID          Azure application (client) ID approved for Minecraft
//!   MAGIC_SERVER                server address to auto-join
//!   MAGIC_APTABASE_KEY          Aptabase app key (anonymous usage telemetry)
//!   MAGIC_DISCORD_APP_ID        Discord application ID (Rich Presence)
//!   MAGIC_SENTRY_DSN            Sentry DSN for the Rust side
//!   MAGIC_SENTRY_DSN_FRONTEND   Sentry DSN for the web UI
//! An empty service ID switches that feature off (no sign-in, telemetry, Sentry or
//! Rich Presence) instead of failing.

use crate::channel::Channel;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const APP_IDENTIFIER: &str = "club.imaginears.magiclauncher";
pub const DISPLAY_NAME: &str = "Imaginears MagicLauncher";
pub const LAUNCHER_BRAND: &str = "magiclauncher";

/// Azure app "Imaginears MagicLauncher" (docs/azure-setup.md); Minecraft services only accept
/// it once Mojang has approved it. A public identifier, not a secret.
pub const MS_CLIENT_ID: &str = match option_env!("MAGIC_MS_CLIENT_ID") {
    Some(v) if !v.is_empty() => v,
    _ => "693dce49-2443-4b96-8912-d1b353b81fcd",
};

/// Hosted pack manifest (S3 + CloudFront; see docs/pack-manifest.md).
pub const PACK_MANIFEST_URL: &str = match option_env!("MAGIC_PACK_URL") {
    Some(v) if !v.is_empty() => v,
    _ => "https://launcher.imaginears.club/pack.json",
};

/// Aptabase app key for anonymous usage telemetry (public; ships in the app).
pub const APTABASE_APP_KEY: &str = match option_env!("MAGIC_APTABASE_KEY") {
    Some(v) if !v.is_empty() => v,
    _ => "",
};

/// Discord application for Rich Presence (public ID).
pub const DISCORD_APP_ID: &str = match option_env!("MAGIC_DISCORD_APP_ID") {
    Some(v) if !v.is_empty() => v,
    _ => "",
};

/// Sentry DSNs (public by design): the Rust backend and the web UI.
pub const SENTRY_DSN: &str = match option_env!("MAGIC_SENTRY_DSN") {
    Some(v) if !v.is_empty() => v,
    _ => "",
};
pub const SENTRY_DSN_FRONTEND: &str = match option_env!("MAGIC_SENTRY_DSN_FRONTEND") {
    Some(v) if !v.is_empty() => v,
    _ => "",
};

/// The launcher's data folder, the same one Tauri's `app_local_data_dir()` returns. Needed
/// before Tauri starts (Sentry must be initialised first, and only with consent).
pub fn default_data_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(dir) = data_dir_override() {
        return Some(dir);
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join(APP_IDENTIFIER))
}

/// Debug builds only: `MAGIC_DATA_DIR` replaces the data folder, so the WebDriver tests
/// run against a throwaway one and never read or change a real install. Release builds
/// ignore it.
pub fn data_dir_override() -> Option<std::path::PathBuf> {
    if cfg!(debug_assertions) {
        std::env::var_os("MAGIC_DATA_DIR").map(std::path::PathBuf::from)
    } else {
        None
    }
}

/// The Imaginears Club server players join (default port 25565 / SRV record).
/// `iears.us` is the server's own address (SRV → `play.iears.us`); `imaginears.club` only
/// points at it through an SRV record that also lists a host which never answers.
pub const SERVER_ADDRESS: &str = match option_env!("MAGIC_SERVER") {
    Some(v) if !v.is_empty() => v,
    _ => "iears.us",
};

/// The Imaginears Club website. Account linking only ever talks to this origin; it's never
/// taken from an `imaginears://` link. Dev builds can point it at a local site with
/// `MAGIC_WEBSITE_URL=http://localhost:3000`.
pub const WEBSITE_URL: &str = match option_env!("MAGIC_WEBSITE_URL") {
    Some(v) if !v.is_empty() => v,
    _ => "https://imaginears.club",
};

/// Per-user settings, stored as `settings.json` in the app data directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Max heap in MiB. `None` = choose automatically from system RAM.
    pub memory_mb: Option<u32>,
    /// Pass `--quickPlayMultiplayer` so the game joins the server on start.
    pub auto_join: bool,
    /// Hide the launcher window once the game has started.
    pub close_on_launch: bool,
    /// Extra JVM flags appended after the defaults (advanced).
    pub extra_jvm_args: Vec<String>,
    /// Selected release channel. Alpha only takes effect while allowlisted
    /// (see `channel::effective`).
    pub channel: Channel,
    /// Anonymous usage + crash reporting (Aptabase, Sentry). `None` = not asked yet:
    /// nothing is sent until the player answers the first-start prompt.
    pub telemetry: Option<bool>,
    /// Game window size in pixels; `None` = Minecraft's default.
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
    /// Start the game in fullscreen.
    pub fullscreen: bool,
    /// Download pack/game updates while the launcher is idle, so Play starts straight away.
    pub background_updates: bool,
    /// Show "Playing Imaginears Club" on the player's Discord profile.
    pub discord_presence: bool,
    /// Graphics preset id from the pack (e.g. "high", "balanced", "low"), or "custom";
    /// `None` = pack default.
    pub preset: Option<String>,
    /// With preset "custom": the optional features the player switched off.
    pub custom_features_off: Vec<String>,
    /// Launcher text size in percent (100 = default; the UI clamps to 80–200).
    pub text_scale: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory_mb: None,
            auto_join: true,
            close_on_launch: true,
            extra_jvm_args: Vec::new(),
            channel: Channel::Release,
            telemetry: None,
            window_width: None,
            window_height: None,
            fullscreen: false,
            background_updates: true,
            discord_presence: true,
            preset: None,
            custom_features_off: Vec::new(),
            text_scale: 100,
        }
    }
}

impl Settings {
    /// Load settings. A value this version can't read (e.g. a channel name from a newer
    /// launcher after going back a version) falls back to its default *alone*; the other
    /// settings are kept instead of all being reset.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .map(|b| Self::from_json_lenient(&b))
            .unwrap_or_default()
    }

    pub fn from_json_lenient(bytes: &[u8]) -> Self {
        if let Ok(s) = serde_json::from_slice::<Settings>(bytes) {
            return s;
        }
        let Ok(serde_json::Value::Object(file)) = serde_json::from_slice(bytes) else {
            return Settings::default();
        };
        let Ok(serde_json::Value::Object(mut merged)) = serde_json::to_value(Settings::default())
        else {
            return Settings::default();
        };
        // Take each value from the file only if the result still reads.
        for (k, v) in file {
            let previous = merged.insert(k.clone(), v);
            if serde_json::from_value::<Settings>(serde_json::Value::Object(merged.clone()))
                .is_err()
            {
                match previous {
                    Some(p) => merged.insert(k, p),
                    None => merged.remove(&k),
                };
            }
        }
        serde_json::from_value(serde_json::Value::Object(merged)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod lenient_tests {
    use super::*;

    #[test]
    fn one_unreadable_setting_does_not_reset_the_rest() {
        let json =
            br#"{"memory_mb": 10240, "channel": "nightly", "text_scale": 130, "auto_join": false}"#;
        let s = Settings::from_json_lenient(json);
        assert_eq!(s.memory_mb, Some(10240));
        assert_eq!(s.text_scale, 130);
        assert!(!s.auto_join);
        assert_eq!(s.channel, Settings::default().channel);
        // Garbage: defaults.
        assert_eq!(Settings::from_json_lenient(b"[1,2]").text_scale, 100);
    }
}
