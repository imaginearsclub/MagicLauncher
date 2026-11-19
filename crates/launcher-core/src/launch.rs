//! Builds the java command line and starts the game.

use crate::auth::Session;
use crate::config::LAUNCHER_BRAND;
use crate::mojang::VersionJson;
use crate::paths::Paths;
use crate::rules::Env;
use crate::{Error, Result};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub java: PathBuf,
    pub version: VersionJson,
    /// Game jar for the end of the classpath (the vanilla client jar; Fabric loads it as is).
    pub game_jar: PathBuf,
    /// The channel's game directory (`Paths::instance(channel)`): saves, mods, logs.
    pub game_dir: PathBuf,
    pub memory_mb: u32,
    pub server: Option<String>,
    pub extra_jvm_args: Vec<String>,
    /// Window size (width, height); uses the version's `has_custom_resolution` arguments.
    pub resolution: Option<(u32, u32)>,
    pub fullscreen: bool,
}

/// Windows bigger than this (e.g. 3440×1440 ultrawide, 4K) need more heap: bigger
/// framebuffers, and map mods render more tiles.
pub const LARGE_WINDOW_PIXELS: u32 = 2560 * 1440;
/// Extra heap for a large window.
pub const LARGE_WINDOW_EXTRA_MB: u32 = 2048;

pub fn is_large_window(resolution: Option<(u32, u32)>) -> bool {
    resolution.is_some_and(|(w, h)| w.saturating_mul(h) > LARGE_WINDOW_PIXELS)
}

/// Pick a heap size: the pack's recommendation (+2 GiB for a large window), capped at half
/// the system RAM, never below 2 GiB. Over-allocating makes GC pauses worse, not better.
pub fn auto_memory_mb(recommended: Option<u32>, resolution: Option<(u32, u32)>) -> u32 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_mb = (sys.total_memory() / 1024 / 1024) as u32;
    pick_memory_mb(recommended, resolution, total_mb)
}

fn pick_memory_mb(recommended: Option<u32>, resolution: Option<(u32, u32)>, total_mb: u32) -> u32 {
    let mut want = recommended.unwrap_or(6144);
    if is_large_window(resolution) {
        want += LARGE_WINDOW_EXTRA_MB;
    }
    want.min(total_mb / 2).max(2048)
}

/// Replace `${name}` placeholders.
pub fn substitute(arg: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        match rest[start..].find('}') {
            Some(end) => {
                let key = &rest[start + 2..start + end];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&rest[start..start + end + 1]),
                }
                rest = &rest[start + end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Returns the full argument list (without the java binary itself).
pub fn build_args(paths: &Paths, spec: &LaunchSpec, session: &Session) -> Result<Vec<String>> {
    let v = &spec.version;
    let mut env = Env::current();
    if spec.server.is_some() {
        env = env.with_feature("is_quick_play_multiplayer", true);
    }
    if spec.resolution.is_some() {
        env = env.with_feature("has_custom_resolution", true);
    }

    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut classpath: Vec<String> = Vec::new();
    for lib in v.libraries_for(&env) {
        if let Some(p) = lib.local_path(paths) {
            let s = p.to_string_lossy().into_owned();
            if !classpath.contains(&s) {
                classpath.push(s);
            }
        }
    }
    classpath.push(spec.game_jar.to_string_lossy().into_owned());

    let natives = paths.natives(&v.id);
    std::fs::create_dir_all(&natives)?;
    let asset_index = v
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_default();

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", session.profile.name.clone());
    vars.insert("auth_uuid", session.profile.uuid.clone());
    vars.insert("auth_access_token", session.access_token.clone());
    vars.insert("auth_xuid", session.xuid.clone().unwrap_or_default());
    vars.insert("clientid", crate::config::MS_CLIENT_ID.to_string());
    vars.insert("user_type", session.user_type.to_string());
    vars.insert("version_name", v.id.clone());
    vars.insert(
        "version_type",
        v.kind.clone().unwrap_or_else(|| "release".into()),
    );
    vars.insert(
        "game_directory",
        spec.game_dir.to_string_lossy().into_owned(),
    );
    vars.insert("assets_root", paths.assets().to_string_lossy().into_owned());
    vars.insert("assets_index_name", asset_index);
    vars.insert("natives_directory", natives.to_string_lossy().into_owned());
    vars.insert(
        "library_directory",
        paths.libraries().to_string_lossy().into_owned(),
    );
    vars.insert("classpath_separator", sep.to_string());
    vars.insert("classpath", classpath.join(sep));
    vars.insert("launcher_name", LAUNCHER_BRAND.to_string());
    vars.insert("launcher_version", crate::VERSION.to_string());
    if let Some((w, h)) = spec.resolution {
        vars.insert("resolution_width", w.to_string());
        vars.insert("resolution_height", h.to_string());
    }
    vars.insert(
        "quickPlayMultiplayer",
        spec.server.clone().unwrap_or_default(),
    );

    let mut args = vec![
        format!("-Xmx{}M", spec.memory_mb),
        format!("-Xms{}M", spec.memory_mb.min(2048)),
        "-XX:+UseG1GC".into(),
        "-XX:MaxGCPauseMillis=50".into(),
        "-XX:+ParallelRefProcEnabled".into(),
    ];
    args.extend(spec.extra_jvm_args.iter().cloned());

    if let Some(l) = v.logging.as_ref().and_then(|l| l.client.as_ref()) {
        let cfg = paths.assets().join("log_configs").join(&l.file.id);
        args.push(l.argument.replace("${path}", &cfg.to_string_lossy()));
    }

    let arguments = v
        .arguments
        .as_ref()
        .ok_or_else(|| Error::other("version has no modern arguments block"))?;
    args.extend(
        arguments
            .jvm
            .iter()
            .flat_map(|a| a.resolve(&env))
            .map(|a| substitute(&a, &vars)),
    );
    args.push(
        v.main_class
            .clone()
            .ok_or_else(|| Error::other("version has no main class"))?,
    );
    args.extend(
        arguments
            .game
            .iter()
            .flat_map(|a| a.resolve(&env))
            .map(|a| substitute(&a, &vars)),
    );
    if spec.fullscreen {
        args.push("--fullscreen".into());
    }
    Ok(args)
}

/// Start the game detached from the launcher. Output goes to `<game_dir>/logs/launcher-game.log`.
pub fn spawn(paths: &Paths, spec: &LaunchSpec, session: &Session) -> Result<std::process::Child> {
    let args = build_args(paths, spec, session)?;
    let logs = spec.game_dir.join("logs");
    std::fs::create_dir_all(&logs)?;
    let log = std::fs::File::create(logs.join("launcher-game.log"))?;

    let mut cmd = std::process::Command::new(&spec.java);
    cmd.args(&args)
        .current_dir(&spec.game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    Ok(cmd.spawn()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_known_and_keeps_unknown() {
        let mut vars = HashMap::new();
        vars.insert("a", "1".to_string());
        vars.insert("b", "two".to_string());
        assert_eq!(substitute("x=${a},y=${b}", &vars), "x=1,y=two");
        assert_eq!(substitute("${missing}", &vars), "${missing}");
        assert_eq!(substitute("plain", &vars), "plain");
        assert_eq!(substitute("unterminated ${a", &vars), "unterminated ${a");
        assert_eq!(
            substitute("-DignoreList=${a}${b}.jar", &vars),
            "-DignoreList=1two.jar"
        );
    }

    #[test]
    fn auto_memory_is_within_bounds() {
        let m = auto_memory_mb(Some(6144), None);
        assert!((2048..=6144).contains(&m));
    }

    #[test]
    fn large_windows_get_more_memory() {
        let ram = 32 * 1024;
        assert_eq!(pick_memory_mb(Some(8192), Some((1920, 1080)), ram), 8192);
        assert_eq!(pick_memory_mb(Some(8192), Some((2560, 1440)), ram), 8192);
        assert_eq!(pick_memory_mb(Some(8192), Some((3440, 1440)), ram), 10240);
        assert_eq!(pick_memory_mb(Some(8192), Some((3840, 2160)), ram), 10240);
        assert_eq!(pick_memory_mb(Some(8192), None, ram), 8192);
        // Still capped at half the RAM.
        assert_eq!(
            pick_memory_mb(Some(8192), Some((3840, 2160)), 16 * 1024),
            8192
        );
    }
}
