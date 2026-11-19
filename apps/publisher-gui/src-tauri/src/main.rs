//! Imaginears PackPublisher: the window version of `pack-publisher`.
//!
//! Every action calls the same library as the command line (`pack_publisher`), so the safety
//! checks, uploads and maintenance flag behave identically. Nothing here asks for typed console
//! input; AWS keys are entered in the window and stored in the same credential-manager entry
//! the command line uses.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pack_builder::instances;
use pack_publisher::{self as publisher, PrepareOptions, Prepared, Target};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::Mutex;

#[derive(Default)]
struct AppState {
    /// The last preview, published by `publish` (must match what the owner looked at).
    prepared: Mutex<Option<(Option<String>, Prepared)>>,
    /// One publish/preview at a time.
    busy: AtomicBool,
    /// Update found by `check_update`, installed by `install_update`.
    pending_update: Mutex<Option<tauri_plugin_updater::Update>>,
}

/// Clears `busy` when the command ends, however it ends.
struct BusyGuard<'a>(&'a AtomicBool);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn claim(state: &AppState) -> Result<BusyGuard<'_>, String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Already working on something. Wait for it to finish.".into());
    }
    Ok(BusyGuard(&state.busy))
}

fn target(channel: Option<String>) -> Target {
    Target {
        channel: channel.filter(|c| !c.is_empty()),
        ..Target::default()
    }
}

fn gui_log(app: &AppHandle) -> publisher::Log {
    let app = app.clone();
    Arc::new(move |line: &str| {
        let _ = app.emit("log", line.to_string());
    })
}

#[derive(Serialize)]
struct InstanceDto {
    name: String,
    path: PathBuf,
    kind: &'static str,
    minecraft: String,
    loader: Option<String>,
}

impl From<instances::Instance> for InstanceDto {
    fn from(i: instances::Instance) -> Self {
        InstanceDto {
            name: i.name,
            path: i.path,
            kind: i.kind.label(),
            minecraft: i.minecraft,
            loader: i.loader,
        }
    }
}

#[derive(Serialize)]
struct Info {
    version: &'static str,
    instances: Vec<InstanceDto>,
    /// Remembered instance per channel ("" = Release).
    remembered: std::collections::BTreeMap<String, PathBuf>,
}

#[tauri::command]
fn info() -> Info {
    let instances = instances::list_default()
        .into_iter()
        .map(InstanceDto::from)
        .collect();
    let mut remembered = std::collections::BTreeMap::new();
    for ch in ["", "beta", "alpha"] {
        let c = (!ch.is_empty()).then_some(ch);
        if let Some(p) = publisher::remembered_instance(c) {
            remembered.insert(ch.to_string(), p);
        }
    }
    Info {
        version: env!("CARGO_PKG_VERSION"),
        instances,
        remembered,
    }
}

/// Check a folder the owner picked/pasted is a pack instance.
#[tauri::command]
fn check_instance(path: PathBuf) -> Result<InstanceDto, String> {
    instances::read(&path)
        .map(InstanceDto::from)
        .ok_or_else(|| {
            format!(
                "{} is not a pack instance (Prism Launcher, Modrinth App, CurseForge, \
             or a folder with mods/)",
                path.display()
            )
        })
}

#[derive(Deserialize)]
struct PreviewArgs {
    instance: PathBuf,
    channel: Option<String>,
    minecraft: Option<String>,
    fabric: Option<String>,
    memory_mb: Option<u32>,
    reupload_all: bool,
}

#[derive(Serialize)]
struct Preview {
    live_version: Option<String>,
    suggested: String,
    unchanged: bool,
    first_publish: bool,
    minecraft: String,
    fabric: String,
    memory_mb: Option<u32>,
    files: usize,
    self_hosted_mods: usize,
    uploads: usize,
    upload_mb: f64,
    diff: Vec<String>,
    new_self_hosted: Vec<String>,
    warnings: Vec<String>,
}

#[tauri::command]
async fn preview(
    app: AppHandle,
    state: State<'_, AppState>,
    args: PreviewArgs,
) -> Result<Preview, String> {
    let _busy = claim(&state)?;
    instances::read(&args.instance).ok_or("That folder is not a pack instance.")?;
    let t = target(args.channel.clone());
    let opts = PrepareOptions {
        minecraft: args.minecraft.filter(|s| !s.trim().is_empty()),
        fabric: args.fabric.filter(|s| !s.trim().is_empty()),
        memory_mb: args.memory_mb,
        reupload_all: args.reupload_all,
    };
    let p = publisher::prepare(&t, &args.instance, &opts, gui_log(&app))
        .await
        .map_err(|e| e.to_string())?;
    publisher::save_instance(&args.instance, t.channel.as_deref());
    let m = &p.built.manifest;
    let out = Preview {
        live_version: p.live.as_ref().map(|l| l.version.to_string()),
        suggested: p.suggested.to_string(),
        unchanged: p.unchanged(),
        first_publish: p.live.is_none(),
        minecraft: m.minecraft.clone(),
        fabric: m.loader.version.clone(),
        memory_mb: m.recommended_memory_mb,
        files: m.files.len(),
        self_hosted_mods: p.built.self_hosted_mods().count(),
        uploads: p.uploads.len(),
        upload_mb: p.upload_bytes as f64 / 1_048_576.0,
        diff: p
            .diff
            .as_ref()
            .map(publisher::diff_lines)
            .unwrap_or_default(),
        new_self_hosted: p.new_self_hosted.clone(),
        warnings: p.built.warnings.clone(),
    };
    *state.prepared.lock().await = Some((t.channel.clone(), p));
    Ok(out)
}

#[derive(Serialize, Clone)]
struct UploadProgress {
    done: usize,
    total: usize,
}

#[tauri::command]
async fn publish(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
    notes: Option<String>,
    force: bool,
) -> Result<String, String> {
    let _busy = claim(&state)?;
    let creds = publisher::env_credentials()
        .or_else(publisher::stored_credentials)
        .ok_or("No AWS key saved yet. Add it under Settings first.")?;
    let version = semver::Version::parse(version.trim().trim_start_matches('v'))
        .map_err(|_| format!("\"{version}\" isn't a version like 1.2.5"))?;
    let mut slot = state.prepared.lock().await;
    let (channel, p) = slot.as_mut().ok_or("Check the pack first (press Check).")?;
    let t = target(channel.clone());
    publisher::finalize(p, version.clone(), notes, force).map_err(|e| e.to_string())?;
    let progress_app = app.clone();
    publisher::publish(&t, p, &creds, force, gui_log(&app), &move |done, total| {
        let _ = progress_app.emit("upload-progress", UploadProgress { done, total });
    })
    .await
    .map_err(|e| e.to_string())?;
    // Published: the preview is stale now.
    *slot = None;
    Ok(format!(
        "Published pack {version}. Players get it the next time they press Play."
    ))
}

#[derive(Serialize)]
struct KeysStatus {
    /// Keys from AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY (they win over the saved key).
    from_env: bool,
    saved: bool,
}

#[tauri::command]
fn keys_status() -> KeysStatus {
    KeysStatus {
        from_env: publisher::env_credentials().is_some(),
        saved: publisher::stored_credentials().is_some(),
    }
}

#[tauri::command]
fn save_keys(access_key_id: String, secret_access_key: String) -> Result<(), String> {
    publisher::store_credentials(&access_key_id, &secret_access_key)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn maintenance_status() -> Result<launcher_core::maintenance::Maintenance, String> {
    Ok(publisher::fetch_maintenance(&Target::default())
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or_default())
}

#[tauri::command]
async fn set_maintenance(
    state: State<'_, AppState>,
    on: bool,
    message: Option<String>,
) -> Result<(), String> {
    let _busy = claim(&state)?;
    let creds = publisher::env_credentials()
        .or_else(publisher::stored_credentials)
        .ok_or("No AWS key saved yet. Add it under Settings first.")?;
    publisher::set_maintenance(&Target::default(), on, message, &creds)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct SuggestionDto {
    name: String,
    id: String,
    file: String,
    /// `None` = could be optional; `Some(reason)` = probably keep.
    keep_reason: Option<&'static str>,
}

#[tauri::command]
fn suggest_optional(instance: PathBuf) -> Result<Vec<SuggestionDto>, String> {
    let (cfg, _) = pack_builder::features::Config::for_instance(&instance)?;
    let mods = pack_builder::suggest::scan_mods(&instance);
    Ok(pack_builder::suggest::suggest(&mods, &cfg)
        .into_iter()
        .map(|s| SuggestionDto {
            keep_reason: match s.verdict {
                pack_builder::suggest::Verdict::Candidate => None,
                pack_builder::suggest::Verdict::ProbablyKeep(why) => Some(why),
            },
            name: s.name,
            id: s.id,
            file: s.file,
        })
        .collect())
}

#[derive(Serialize)]
struct UpdateInfo {
    version: String,
    notes: Option<String>,
}

#[derive(Serialize, Clone)]
struct DownloadProgress {
    downloaded: u64,
    total: Option<u64>,
}

/// Is a newer Imaginears PackPublisher on the tools feed? Errors (offline, no feed yet) are
/// reported as "no update" by the UI, never as a blocker.
#[tauri::command]
async fn check_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
    });
    *state.pending_update.lock().await = update;
    Ok(info)
}

/// Download, verify (signature) and install the update, then restart into it.
#[tauri::command]
async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let _busy = claim(&state)?;
    let update = state
        .pending_update
        .lock()
        .await
        .take()
        .ok_or("No update to install; check again.")?;
    let mut downloaded = 0u64;
    let emitter = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = emitter.emit("update-progress", DownloadProgress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    // On Windows the installer has already closed the app; elsewhere restart into it.
    app.restart();
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            info,
            check_instance,
            preview,
            publish,
            keys_status,
            save_keys,
            maintenance_status,
            set_maintenance,
            suggest_optional,
            check_update,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running Imaginears PackPublisher");
}
