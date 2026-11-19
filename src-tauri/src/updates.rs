//! Launcher self-update per release channel, plus the channel/alpha-access status.
//!
//! The Release endpoint, public key and Windows install mode live in tauri.conf.json;
//! here the endpoint is swapped for the effective channel's feed. Only the explicit
//! "go back to Release" action may install an older version.

use crate::AppState;
use launcher_core::channel::{self, Channel};
use launcher_core::config::Settings;
use launcher_core::{allowlist, Error, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize, Clone)]
pub struct ChannelStatus {
    /// What the player picked in Settings.
    pub selected: Channel,
    /// What is actually used (Alpha falls back to Beta when not allowlisted).
    pub effective: Channel,
    /// Whether the signed-in account may see and use Alpha.
    pub alpha_allowed: bool,
    /// The channel of the running launcher version (for the BETA/ALPHA badge).
    pub running: Channel,
}

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    /// `"mandatory": true` in the feed disables Play until updated.
    pub mandatory: bool,
    /// True for "go back to Release": installs an older version.
    pub downgrade: bool,
}

#[derive(Serialize, Clone)]
struct UpdateProgress {
    downloaded: u64,
    total: Option<u64>,
}

fn running_channel() -> Channel {
    semver::Version::parse(launcher_core::VERSION)
        .map(|v| Channel::of_version(&v))
        .unwrap_or_default()
}

/// The signed-in account's UUID, from the Rust session (never from the UI).
async fn signed_in_uuid(state: &AppState) -> Option<String> {
    state
        .session
        .lock()
        .await
        .as_ref()
        .map(|s| s.profile.uuid.clone())
}

pub async fn status(state: &AppState) -> ChannelStatus {
    let selected = Settings::load(&state.paths.settings()).channel;
    let alpha_allowed = match signed_in_uuid(state).await {
        Some(uuid) => allowlist::alpha_allowed(&state.client, &state.paths, &uuid).await,
        None => false,
    };
    ChannelStatus {
        selected,
        effective: channel::effective(selected, alpha_allowed),
        alpha_allowed,
        running: running_channel(),
    }
}

#[tauri::command]
pub async fn channel_status(state: State<'_, AppState>) -> Result<ChannelStatus> {
    Ok(status(&state).await)
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::other(e.to_string())
}

/// Check the effective channel's feed. With `go_back`, check the Release feed and
/// accept any *different* version, so a beta/alpha build can return to Release.
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    state: State<'_, AppState>,
    go_back: bool,
) -> Result<Option<UpdateInfo>> {
    let feed = if go_back {
        Channel::Release
    } else {
        status(&state).await.effective
    };
    let url = tauri::Url::parse(&feed.feed_url()).map_err(err)?;
    let mut builder = app.updater_builder().endpoints(vec![url]).map_err(err)?;
    if go_back {
        builder = builder.version_comparator(|current, remote| remote.version != current);
    }
    let update = builder.build().map_err(err)?.check().await.map_err(err)?;

    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
        mandatory: u.raw_json.get("mandatory") == Some(&serde_json::Value::Bool(true)),
        downgrade: go_back,
    });
    *state.pending_update.lock().await = update;
    Ok(info)
}

/// Download and install the update found by the last `check_update`, then restart.
/// Progress is emitted as `update-progress` events.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let update = state
        .pending_update
        .lock()
        .await
        .take()
        .ok_or_else(|| Error::other("no update to install; check again"))?;
    let mut downloaded = 0u64;
    let emitter = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = emitter.emit("update-progress", UpdateProgress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(err)?;
    // On Windows the installer has already closed the app; elsewhere restart into it.
    app.restart();
}
