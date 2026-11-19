//! The whole "press Play" flow in one place:
//! pack manifest → vanilla → Java → Fabric → libraries → pack sync → launch.
//!
//! `prepare` installs/updates everything; `play` prepares then starts the game; `repair`
//! clears the launcher's "already installed" shortcuts and prepares without starting.

use crate::auth::Session;
use crate::channel::Channel;
use crate::config::{self, Settings};
use crate::launch::{self, LaunchSpec};
use crate::pack::{self, LoaderKind};
use crate::paths::Paths;
use crate::progress::{report, ProgressFn};
use crate::{fabric, java, mojang, Result};

pub struct Launched {
    /// The running game. Callers may `wait()` on it (from a blocking thread).
    pub child: std::process::Child,
    pub pack_version: String,
}

/// Everything installed and ready to launch.
pub struct Prepared {
    pub spec: LaunchSpec,
    pub pack_version: String,
}

/// Install or update everything for `channel` (the *effective* channel: it picks the game
/// folder and the pack, `<channel>/pack.json` falling back to the Release `manifest_url`).
/// With `repair`, nothing installed is trusted: Java and the Fabric profile are re-fetched and
/// every asset is re-hashed. Library, game and managed pack files are hash-checked on
/// every run anyway; player-owned (`default`) pack files are never touched.
pub async fn prepare(
    client: &reqwest::Client,
    paths: &Paths,
    settings: &Settings,
    channel: Channel,
    manifest_url: &str,
    repair: bool,
    progress: &ProgressFn,
) -> Result<Prepared> {
    report(progress, "Checking for modpack updates", 0, 1);
    let manifest = pack::fetch_manifest_for(client, manifest_url, channel).await?;
    let game_dir = paths.instance(channel);
    let LoaderKind::Fabric = manifest.loader.kind;

    let vanilla =
        mojang::install_vanilla(client, paths, &manifest.minecraft, repair, progress).await?;
    let component = vanilla
        .java_version
        .as_ref()
        .map(|j| j.component.clone())
        .unwrap_or_else(|| "java-runtime-delta".into());
    if repair {
        // Forces every Java runtime file to be re-checked against Mojang's manifest.
        let _ = std::fs::remove_file(
            paths
                .runtimes()
                .join(&component)
                .join(".magiclauncher-manifest-sha1"),
        );
    }
    let runtime = java::ensure_runtime(client, paths, &component, progress).await?;

    if repair {
        // Fetches the Fabric profile again from Fabric meta.
        let id = fabric::version_id(&manifest.minecraft, &manifest.loader.version);
        let _ = std::fs::remove_file(paths.version_json(&id));
    }
    let loader = fabric::ensure_installed(
        client,
        paths,
        &manifest.minecraft,
        &manifest.loader.version,
        progress,
    )
    .await?;
    let profile = mojang::merge(vanilla, loader);
    mojang::ensure_libraries(client, paths, &profile, progress).await?;

    // Fabric loads the vanilla game jar from the classpath as is.
    let game_jar = paths.version_jar(&manifest.minecraft);

    let disabled = pack::disabled_features(
        &manifest,
        settings.preset.as_deref(),
        &settings.custom_features_off,
    );
    pack::sync(client, &game_dir, &manifest, &disabled, progress).await?;

    Ok(Prepared {
        spec: LaunchSpec {
            java: java::java_binary(&runtime, true),
            version: profile,
            game_jar,
            game_dir,
            memory_mb: settings.memory_mb.unwrap_or_else(|| {
                launch::auto_memory_mb(
                    manifest.recommended_memory_mb,
                    settings.window_width.zip(settings.window_height),
                )
            }),
            server: settings
                .auto_join
                .then(|| config::SERVER_ADDRESS.to_string()),
            extra_jvm_args: settings.extra_jvm_args.clone(),
            resolution: settings.window_width.zip(settings.window_height),
            fullscreen: settings.fullscreen,
        },
        pack_version: manifest.version.to_string(),
    })
}

pub async fn play(
    client: &reqwest::Client,
    paths: &Paths,
    session: &Session,
    settings: &Settings,
    channel: Channel,
    manifest_url: &str,
    progress: &ProgressFn,
) -> Result<Launched> {
    let p = prepare(
        client,
        paths,
        settings,
        channel,
        manifest_url,
        false,
        progress,
    )
    .await?;
    report(progress, "Starting Minecraft", 0, 1);
    let child = launch::spawn(paths, &p.spec, session)?;
    report(progress, "Starting Minecraft", 1, 1);
    Ok(Launched {
        child,
        pack_version: p.pack_version,
    })
}

/// Re-verify and fix the whole install for `channel` without starting the game.
pub async fn repair(
    client: &reqwest::Client,
    paths: &Paths,
    settings: &Settings,
    channel: Channel,
    manifest_url: &str,
    progress: &ProgressFn,
) -> Result<String> {
    let p = prepare(
        client,
        paths,
        settings,
        channel,
        manifest_url,
        true,
        progress,
    )
    .await?;
    report(progress, "Repair finished", 1, 1);
    Ok(p.pack_version)
}
