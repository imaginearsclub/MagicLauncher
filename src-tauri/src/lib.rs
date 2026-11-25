//! Thin Tauri layer: commands + events. All real logic is in `launcher-core`.

mod deeplink;
mod discord;
mod monitoring;
mod titlebar;
mod updates;

use launcher_core::auth::{self, DeviceCode, Profile, Session};
use launcher_core::config::{self, Settings};
use launcher_core::paths::Paths;
use launcher_core::progress::{Progress, ProgressFn};
use launcher_core::{crash, maintenance, pipeline, status, website_link, Error, Result};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Mutex;

struct AppState {
    client: reqwest::Client,
    paths: Paths,
    session: Mutex<Option<Session>>,
    busy: AtomicBool,
    /// Found by `check_update`, consumed by `install_update`.
    pending_update: Mutex<Option<tauri_plugin_updater::Update>>,
    /// Scrubbed log of the last crash, for "Copy log" / "Share link".
    last_crash_log: std::sync::Mutex<Option<String>>,
    /// Anonymous usage events (only sent with consent).
    usage: Arc<monitoring::Usage>,
    discord: discord::Discord,
    /// The last `imaginears://` action, until the UI takes it (it may not be loaded yet when
    /// the launcher was started by the link). Only the newest counts: actions are screens to
    /// show, so an older one is out of date once a newer link arrives.
    pending_link: std::sync::Mutex<Option<deeplink::Action>>,
    /// A link focused the launcher while a launch was running: don't hide the window when
    /// the game starts (`close_on_launch`), the player asked to see it.
    link_during_launch: AtomicBool,
}

impl AppState {
    /// Update Discord Rich Presence, respecting the player's setting.
    fn presence(&self, p: discord::Presence) {
        if Settings::load(&self.paths.settings()).discord_presence {
            self.discord.set(p);
        } else {
            self.discord.set(discord::Presence::Clear);
        }
    }

    fn track(&self, name: &'static str, props: serde_json::Value) {
        self.usage.track(&self.paths.settings(), name, props);
    }
}

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    server: &'static str,
    dev_build: bool,
    /// For the UI's own Sentry client (started only with consent).
    sentry_dsn_frontend: &'static str,
    /// Dev builds: send a frontend test error to verify the Sentry setup.
    sentry_test: bool,
}

/// Show and focus the launcher window (also when it was hidden while the game runs).
fn focus_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// An `imaginears://` link was opened: focus the launcher and hand the action to the UI.
fn handle_links(app: &AppHandle, urls: &[tauri::Url]) {
    let Some(action) = urls.iter().find_map(deeplink::parse) else {
        return;
    };
    focus_main(app);
    let state = app.state::<AppState>();
    state.link_during_launch.store(true, Ordering::SeqCst);
    let name = action.as_str();
    *state.pending_link.lock().unwrap_or_else(|e| e.into_inner()) = Some(action);
    state.track("deep_link_opened", serde_json::json!({ "action": name }));
    let _ = app.emit("deep-link", ());
}

/// What the "link your account" prompt shows: the website account the link belongs to and
/// the Minecraft account that would be linked.
#[derive(Serialize)]
struct LinkPrompt {
    website_account: String,
    minecraft_name: String,
    expires_in_seconds: u64,
}

/// Details for the prompt after an `imaginears://link/<token>` link. Changes nothing.
#[tauri::command]
async fn link_prompt(state: State<'_, AppState>, token: String) -> Result<LinkPrompt> {
    let session = active_session(&state).await?;
    let challenge = website_link::challenge(&state.client, &token).await?;
    Ok(LinkPrompt {
        website_account: challenge.website_account,
        minecraft_name: session.profile.name,
        expires_in_seconds: challenge.expires_in_seconds,
    })
}

/// The player confirmed the prompt: prove the account to the website and link it.
/// Returns the linked Minecraft name.
#[tauri::command]
async fn link_confirm(state: State<'_, AppState>, token: String) -> Result<String> {
    let session = active_session(&state).await?;
    let result = website_link::link(&state.client, &session, &token).await;
    state.track(
        "website_link",
        serde_json::json!({ "ok": result.is_ok(), "error": result.as_ref().err().map(|e| e.category()) }),
    );
    result
}

/// The `imaginears://` action waiting for the UI, if any (taken once).
#[tauri::command]
fn take_deep_link(state: State<'_, AppState>) -> Option<deeplink::Action> {
    state
        .pending_link
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: launcher_core::VERSION,
        server: config::SERVER_ADDRESS,
        dev_build: cfg!(debug_assertions),
        sentry_dsn_frontend: config::SENTRY_DSN_FRONTEND,
        sentry_test: cfg!(debug_assertions) && std::env::var_os("MAGIC_SENTRY_TEST").is_some(),
    }
}

/// Answer to the telemetry prompt / Settings toggle. Starts or stops Sentry immediately.
#[tauri::command]
fn set_telemetry(state: State<'_, AppState>, enabled: bool) -> Result<()> {
    let mut settings = Settings::load(&state.paths.settings());
    let first_answer = settings.telemetry.is_none();
    settings.telemetry = Some(enabled);
    settings.save(&state.paths.settings())?;
    if enabled {
        monitoring::start_sentry();
        if first_answer {
            state.track("telemetry_enabled", serde_json::json!({}));
        }
    } else {
        monitoring::stop_sentry();
    }
    Ok(())
}

/// Restore the active account's sign-in, if any. Returns the profile to show.
/// Migrates a pre-0.5 single login into the account list on first use.
#[tauri::command]
async fn account(state: State<'_, AppState>) -> Result<Option<Profile>> {
    let mut guard = state.session.lock().await;
    if let Some(s) = guard.as_ref().filter(|s| !s.is_expired()) {
        return Ok(Some(s.profile.clone()));
    }
    let mut accounts = auth::Accounts::load(&state.paths.accounts());
    let uuid = match accounts.active.clone() {
        Some(u) => Some(u),
        // A throwaway test folder must not pick up this PC's real (keychain) sign-in.
        None if launcher_core::config::data_dir_override().is_none()
            && auth::has_legacy_login() =>
        {
            None
        }
        None => return Ok(None),
    };
    match auth::refresh(&state.client, uuid.as_deref()).await {
        Ok(s) => {
            let p = s.profile.clone();
            accounts.upsert_active(p.clone());
            let _ = accounts.save(&state.paths.accounts());
            let _ = std::fs::remove_file(state.paths.account()); // pre-0.5 file
            *guard = Some(s);
            Ok(Some(p))
        }
        Err(Error::NotSignedIn) => {
            // That account's login expired: drop it, the next one becomes active.
            if let Some(u) = uuid {
                accounts.remove(&u);
                let _ = accounts.save(&state.paths.accounts());
            }
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn login_start(app: AppHandle, state: State<'_, AppState>) -> Result<DeviceCode> {
    let dc = auth::start_device_code(&state.client).await?;
    let _ = app.opener().open_url(&dc.verification_uri, None::<&str>);
    Ok(dc)
}

/// Sign in (or add) an account; it becomes the active one.
#[tauri::command]
async fn login_finish(state: State<'_, AppState>, code: DeviceCode) -> Result<Profile> {
    let s = auth::finish_device_code(&state.client, &code).await?;
    let p = s.profile.clone();
    let mut accounts = auth::Accounts::load(&state.paths.accounts());
    accounts.upsert_active(p.clone());
    accounts.save(&state.paths.accounts())?;
    *state.session.lock().await = Some(s);
    Ok(p)
}

/// All signed-in accounts and the active one.
#[tauri::command]
fn accounts(state: State<'_, AppState>) -> auth::Accounts {
    auth::Accounts::load(&state.paths.accounts())
}

/// Make another signed-in account active (silent sign-in with its saved login).
#[tauri::command]
async fn switch_account(state: State<'_, AppState>, uuid: String) -> Result<Profile> {
    if state.busy.load(Ordering::SeqCst) {
        return Err(Error::other(
            "wait until the launcher is done before switching accounts",
        ));
    }
    let s = match auth::refresh(&state.client, Some(&uuid)).await {
        Ok(s) => s,
        Err(Error::NotSignedIn) => {
            let mut accounts = auth::Accounts::load(&state.paths.accounts());
            accounts.remove(&uuid);
            let _ = accounts.save(&state.paths.accounts());
            return Err(Error::other(
                "that account's sign-in has expired; add it again with \"Add account\"",
            ));
        }
        Err(e) => return Err(e),
    };
    let p = s.profile.clone();
    let mut accounts = auth::Accounts::load(&state.paths.accounts());
    accounts.upsert_active(p.clone());
    accounts.save(&state.paths.accounts())?;
    *state.session.lock().await = Some(s);
    Ok(p)
}

/// Sign out of the active account (removes its saved login). Another signed-in account,
/// if any, becomes active; the UI then calls `account` to sign it in.
#[tauri::command]
async fn logout(state: State<'_, AppState>) -> Result<()> {
    let current = state.session.lock().await.take().map(|s| s.profile.uuid);
    let mut accounts = auth::Accounts::load(&state.paths.accounts());
    let uuid = current.or_else(|| accounts.active.clone());
    if let Some(u) = &uuid {
        auth::delete_refresh_token(Some(u))?;
        accounts.remove(u);
        accounts.save(&state.paths.accounts())?;
    }
    let _ = auth::delete_refresh_token(None);
    let _ = std::fs::remove_file(state.paths.account());
    Ok(())
}

#[derive(Serialize)]
struct PackOverview {
    /// Pack version installed in this channel's game folder (`None` = not installed yet).
    installed: Option<String>,
    latest: String,
    update_available: bool,
    /// "What's new" for `latest`, if the publisher included it.
    changes: Option<launcher_core::pack::PackChanges>,
    /// Graphics presets offered by the pack (empty = none).
    presets: Vec<launcher_core::pack::Preset>,
    features: Vec<launcher_core::pack::Feature>,
}

/// Result of checking the published pack, for the home screen.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum PackCheck {
    Ready(Box<PackOverview>),
    /// Nothing published for this channel (nor Release, which it falls back to).
    NotPublished,
    /// Offline, server error or unreadable pack; Play still works with what's installed.
    Unreachable,
}

/// Installed vs published pack for the effective channel, for the home screen.
#[tauri::command]
async fn pack_overview(state: State<'_, AppState>) -> Result<PackCheck> {
    let channel = updates::status(&state).await.effective;
    let url = pack_url();
    let m = match launcher_core::pack::fetch_manifest_for(&state.client, &url, channel).await {
        Ok(m) => m,
        Err(Error::PackNotPublished(_)) => return Ok(PackCheck::NotPublished),
        Err(_) => return Ok(PackCheck::Unreachable),
    };
    let installed = launcher_core::pack::Lock::load(&state.paths.instance(channel)).pack_version;
    Ok(PackCheck::Ready(Box::new(PackOverview {
        update_available: installed.as_ref() != Some(&m.version),
        installed: installed.map(|v| v.to_string()),
        latest: m.version.to_string(),
        changes: m.changes,
        presets: m.presets,
        features: m.features,
    })))
}

/// The Release pack URL (dev builds may override it with MAGIC_PACK_URL at runtime).
fn pack_url() -> String {
    if cfg!(debug_assertions) {
        std::env::var("MAGIC_PACK_URL").unwrap_or_else(|_| config::PACK_MANIFEST_URL.into())
    } else {
        config::PACK_MANIFEST_URL.to_string()
    }
}

/// Live status of the server the launcher joins (players, MOTD, ping).
/// `None` = offline or unreachable.
#[tauri::command]
async fn server_status(state: State<'_, AppState>) -> Result<Option<status::ServerStatus>> {
    Ok(status::ping(&state.client, config::SERVER_ADDRESS)
        .await
        .ok())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    Settings::load(&state.paths.settings())
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<()> {
    settings.save(&state.paths.settings())?;
    // Apply the Discord toggle right away (the game, if running, updates it on exit).
    if !state.busy.load(Ordering::SeqCst) {
        state.presence(discord::Presence::Launcher);
    }
    Ok(())
}

#[tauri::command]
async fn open_game_folder(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let channel = updates::status(&state).await.effective;
    let dir = state.paths.instance(channel);
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::other(e.to_string()))
}

/// The active account's session, refreshed first if it has expired.
async fn active_session(state: &AppState) -> Result<Session> {
    let mut guard = state.session.lock().await;
    if guard.as_ref().is_none_or(|s| s.is_expired()) {
        let active = auth::Accounts::load(&state.paths.accounts()).active;
        *guard = Some(auth::refresh(&state.client, active.as_deref()).await?);
    }
    guard.clone().ok_or(Error::NotSignedIn)
}

#[tauri::command]
async fn play(app: AppHandle, state: State<'_, AppState>) -> Result<String> {
    let session = active_session(&state).await?;
    // Enforced here too, not just in the UI. Alpha testers may play to test the update.
    let m = maintenance::current(&state.client).await;
    if m.maintenance && !updates::status(&state).await.alpha_allowed {
        return Err(Error::other(format!(
            "The server is under maintenance{}",
            m.message.map(|t| format!(": {t}")).unwrap_or_default()
        )));
    }
    run_pipeline(&app, &state, &session).await
}

/// The signed-in player's skin image URL (Mojang), for the header head. `None` = default
/// skin, not signed in, or offline.
#[tauri::command]
async fn skin_url(state: State<'_, AppState>) -> Result<Option<String>> {
    let uuid = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|s| s.profile.uuid.clone());
    Ok(match uuid {
        Some(u) => launcher_core::skin::cached_skin(&state.client, &state.paths.cache(), &u).await,
        None => None,
    })
}

/// The last crash's log, scrubbed of personal data (for "Copy log").
#[tauri::command]
fn crash_log(state: State<'_, AppState>) -> Result<String> {
    state
        .last_crash_log
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .ok_or_else(|| Error::other("no crash log from this session"))
}

/// Upload the last crash's scrubbed log to mclo.gs and return the public link.
#[tauri::command]
async fn share_crash_log(state: State<'_, AppState>) -> Result<String> {
    let text = crash_log(state.clone())?;
    crash::share_mclogs(&state.client, &text).await
}

/// Open the current channel's logs folder.
#[tauri::command]
async fn open_logs_folder(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let channel = updates::status(&state).await.effective;
    let dir = state.paths.instance(channel).join("logs");
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::other(e.to_string()))
}

/// Background pre-download: install/update everything for the current channel without
/// starting the game, so the next Play is instant. Skipped (Ok(None)) when busy.
#[tauri::command]
async fn prepare_update(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Ok(None);
    }
    let emitter = app.clone();
    let progress: ProgressFn = Arc::new(move |p: Progress| {
        let _ = emitter.emit("progress", p);
    });
    let settings = Settings::load(&state.paths.settings());
    let channel = updates::status(&state).await.effective;
    let started = std::time::Instant::now();
    let result = pipeline::prepare(
        &state.client,
        &state.paths,
        &settings,
        channel,
        &pack_url(),
        false,
        &progress,
    )
    .await
    .map(|p| Some(p.pack_version));
    state.busy.store(false, Ordering::SeqCst);
    track_result(&state, "predownload_finished", &result, started, channel);
    result
}

/// Settings → Repair game files: re-verify and fix the install for the current channel.
#[tauri::command]
async fn repair(app: AppHandle, state: State<'_, AppState>) -> Result<String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err(Error::other(
            "the launcher is busy; try again when it's done",
        ));
    }
    let emitter = app.clone();
    let progress: ProgressFn = Arc::new(move |p: Progress| {
        let _ = emitter.emit("progress", p);
    });
    let settings = Settings::load(&state.paths.settings());
    let channel = updates::status(&state).await.effective;
    let repair_started = std::time::Instant::now();
    let result = pipeline::repair(
        &state.client,
        &state.paths,
        &settings,
        channel,
        &pack_url(),
        &progress,
    )
    .await;
    state.busy.store(false, Ordering::SeqCst);
    track_result(&state, "repair_finished", &result, repair_started, channel);
    result
}

/// Usage event + Sentry report for Play / Repair outcomes.
fn track_result<T>(
    state: &AppState,
    name: &'static str,
    result: &Result<T>,
    started: std::time::Instant,
    channel: launcher_core::channel::Channel,
) {
    let mut props = serde_json::json!({
        "ok": result.is_ok(),
        "duration_s": started.elapsed().as_secs(),
        "channel": channel.as_str(),
    });
    if let Err(e) = result {
        props["error"] = e.category().into();
        // Expected situations, not bugs.
        if !matches!(e, Error::NotSignedIn | Error::Http(_)) {
            monitoring::report_error(name, e);
        }
    }
    state.track(name, props);
}

#[derive(Serialize)]
struct MaintenanceStatus {
    maintenance: bool,
    message: Option<String>,
    /// Allowlisted Alpha testers can still press Play during maintenance.
    can_bypass: bool,
}

#[tauri::command]
async fn maintenance_status(state: State<'_, AppState>) -> Result<MaintenanceStatus> {
    let m = maintenance::current(&state.client).await;
    let can_bypass = m.maintenance && updates::status(&state).await.alpha_allowed;
    Ok(MaintenanceStatus {
        maintenance: m.maintenance,
        message: m.message,
        can_bypass,
    })
}

/// Debug builds only: launch with an offline profile to test install/launch
/// before the Azure app is approved. Online-mode servers will reject it.
#[cfg(debug_assertions)]
#[tauri::command]
async fn play_offline(
    app: AppHandle,
    state: State<'_, AppState>,
    username: String,
) -> Result<String> {
    let hash = launcher_core::download::sha1_hex(format!("OfflinePlayer:{username}").as_bytes());
    let session = Session {
        profile: Profile {
            name: username,
            uuid: hash[..32].to_string(),
        },
        access_token: "0".into(),
        xuid: None,
        expires_at: u64::MAX,
        user_type: "legacy",
    };
    run_pipeline(&app, &state, &session).await
}

async fn run_pipeline(app: &AppHandle, state: &AppState, session: &Session) -> Result<String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err(Error::other("already launching"));
    }
    state.link_during_launch.store(false, Ordering::SeqCst);
    let emitter = app.clone();
    let progress: ProgressFn = Arc::new(move |p: Progress| {
        let _ = emitter.emit("progress", p);
    });
    let settings = Settings::load(&state.paths.settings());
    let channel = updates::status(state).await.effective;
    let game_dir = state.paths.instance(channel);
    let started = std::time::SystemTime::now();
    let play_started = std::time::Instant::now();

    let url = pack_url();

    let result = pipeline::play(
        &state.client,
        &state.paths,
        session,
        &settings,
        channel,
        &url,
        &progress,
    )
    .await;
    state.busy.store(false, Ordering::SeqCst);
    track_result(state, "play_finished", &result, play_started, channel);
    let mut launched = result?;
    let pack_version = launched.pack_version.clone();
    // Discord: "Playing Imaginears Club · Pack x" with the server's player count.
    {
        let app = app.clone();
        let pack_version = pack_version.clone();
        let started = discord::now_unix();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let online = status::ping(&state.client, config::SERVER_ADDRESS)
                .await
                .ok()
                .map(|s| (s.online, s.max));
            state.presence(discord::Presence::Playing {
                pack_version,
                online,
                started,
            });
        });
    }

    if settings.close_on_launch && !state.link_during_launch.load(Ordering::SeqCst) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.hide();
        }
    }
    // When the game exits (normally or by crashing) bring the launcher back,
    // so players can see what happened and relaunch.
    let app = app.clone();
    std::thread::spawn(move || {
        let code = launched.child.wait().ok().and_then(|s| s.code());
        // Crash = non-zero exit or a crash report written during this run.
        let report = crash::new_crash_report(&game_dir, started);
        if code != Some(0) || report.is_some() {
            let text = crash::collect_log(&game_dir, report.as_deref());
            let info = crash::classify(&text, code, report);
            let state = app.state::<AppState>();
            if let Ok(mut slot) = state.last_crash_log.lock() {
                *slot = Some(crash::scrub(&text));
            }
            state.track(
                "game_crashed",
                serde_json::json!({
                    "signature": info.cause.signature(),
                    "mod": info.culprit.clone().unwrap_or_default(),
                    "minutes": started.elapsed().map(|d| d.as_secs() / 60).unwrap_or(0),
                    "pack_version": pack_version,
                }),
            );
            let _ = app.emit("game-crashed", info);
        } else {
            app.state::<AppState>().track(
                "game_session",
                serde_json::json!({
                    "minutes": started.elapsed().map(|d| d.as_secs() / 60).unwrap_or(0),
                    "pack_version": pack_version,
                }),
            );
        }
        app.state::<AppState>()
            .presence(discord::Presence::Launcher);
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
        }
        let _ = app.emit("game-exited", code);
    });
    Ok(launched.pack_version)
}

/// WebKitGTK's DMA-BUF renderer leaves the window blank on many setups (NVIDIA drivers,
/// WSL, some VMs). Turn it off unless the player has set the variable themselves; this
/// must run before any thread starts, because it changes the process environment.
fn linux_webkit_workarounds() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    linux_webkit_workarounds();
    // Before Tauri starts its async runtime, so every thread reports to the same client.
    monitoring::init_from_settings();
    tauri::Builder::default()
        // First plugin: a second launch (or an `imaginears://` link while the launcher is
        // open) goes to the running launcher instead of starting another one. With its
        // `deep-link` feature the link is passed on to the deep-link plugin below.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            focus_main(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let root = match launcher_core::config::data_dir_override() {
                Some(dir) => dir,
                None => app.path().app_local_data_dir()?,
            };
            std::fs::create_dir_all(&root)?;
            let client = launcher_core::http_client();
            app.manage(AppState {
                usage: monitoring::Usage::new(client.clone()),
                discord: discord::Discord::start(),
                client,
                paths: Paths::new(root),
                session: Mutex::new(None),
                busy: AtomicBool::new(false),
                pending_update: Mutex::new(None),
                last_crash_log: std::sync::Mutex::new(None),
                pending_link: std::sync::Mutex::new(None),
                link_during_launch: AtomicBool::new(false),
            });
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                // Installers register `imaginears://` (NSIS, .deb/.rpm, macOS bundle). The
                // AppImage and Windows/Linux dev builds aren't installed, so they register it
                // themselves. macOS can't register at runtime: there links only reach a built
                // app bundle (`pnpm tauri build --debug`), not `pnpm tauri dev`.
                #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
                if let Err(e) = app.deep_link().register_all() {
                    eprintln!("couldn't register imaginears:// links: {e}");
                }
                if let Ok(Some(urls)) = app.deep_link().get_current() {
                    handle_links(app.handle(), &urls);
                }
                let handle = app.handle().clone();
                app.deep_link()
                    .on_open_url(move |event| handle_links(&handle, &event.urls()));
            }
            if let Some(window) = app.get_webview_window("main") {
                titlebar::apply(&window);
            }
            let state = app.state::<AppState>();
            state.presence(discord::Presence::Launcher);
            let settings = Settings::load(&state.paths.settings());
            state.track(
                "app_started",
                serde_json::json!({
                    "channel": settings.channel.as_str(),
                    "memory": if settings.memory_mb.is_some() { "manual" } else { "auto" },
                }),
            );
            // Dev builds: MAGIC_SENTRY_TEST=1 sends a real error through the reporting
            // path to verify the Sentry setup end to end.
            if cfg!(debug_assertions) && std::env::var_os("MAGIC_SENTRY_TEST").is_some() {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                monitoring::report_error(
                    "sentry_test",
                    &Error::other(format!("Sentry test error {ts}")),
                );
            }
            Ok(())
        })
        .invoke_handler(handlers())
        .run(tauri::generate_context!())
        .expect("error while running the launcher");
}

#[cfg(debug_assertions)]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_info,
        account,
        login_start,
        login_finish,
        logout,
        accounts,
        switch_account,
        get_settings,
        save_settings,
        open_game_folder,
        play,
        play_offline,
        server_status,
        pack_overview,
        maintenance_status,
        repair,
        skin_url,
        crash_log,
        share_crash_log,
        open_logs_folder,
        set_telemetry,
        prepare_update,
        updates::channel_status,
        updates::check_update,
        updates::install_update,
        take_deep_link,
        link_prompt,
        link_confirm
    ]
}

#[cfg(not(debug_assertions))]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_info,
        account,
        login_start,
        login_finish,
        logout,
        accounts,
        switch_account,
        get_settings,
        save_settings,
        open_game_folder,
        play,
        server_status,
        pack_overview,
        maintenance_status,
        repair,
        skin_url,
        crash_log,
        share_crash_log,
        open_logs_folder,
        set_telemetry,
        prepare_update,
        updates::channel_status,
        updates::check_update,
        updates::install_update,
        take_deep_link,
        link_prompt,
        link_confirm
    ]
}
