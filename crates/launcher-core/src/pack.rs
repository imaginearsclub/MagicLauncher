//! Modpack manifest (`pack.json`) and sync. See docs/pack-manifest.md.
//!
//! Safety rules:
//! - Only files listed in the *previous* lock file are ever deleted, so player
//!   files (saves, screenshots, extra resource packs, waypoints) are never touched.
//! - `default` files are copied only when missing, so player tweaks to e.g.
//!   options.txt or client configs survive updates.
//! - Every path is validated with `safe_join` before use.

use crate::channel::Channel;
use crate::download::{self, Download};
use crate::paths::safe_join;
use crate::progress::{report, ProgressFn};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FORMAT_VERSION: u32 = 1;
const LOCK_FILE: &str = ".magiclauncher/pack-lock.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackManifest {
    /// Schema version of this file, bumped only on breaking format changes.
    pub format_version: u32,
    pub id: String,
    pub name: String,
    /// The pack's own SemVer, e.g. "1.4.0". Shown in the launcher; the server can require it.
    pub version: semver::Version,
    pub minecraft: String,
    pub loader: Loader,
    /// Suggested max heap in MiB for this pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_memory_mb: Option<u32>,
    pub files: Vec<PackFile>,
    /// What changed since the previous published version (written by pack-publisher).
    /// Optional and informational; older launchers ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<PackChanges>,
    /// Optional groups of client-side files (e.g. shaders) players can switch off via presets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<Feature>,
    /// Named sets of features to switch off (e.g. "Low-end PC").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<Preset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// On unless a preset turns it off.
    #[serde(default = "yes")]
    pub default: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Feature ids this preset turns off.
    #[serde(default)]
    pub disable: Vec<String>,
}

/// The preset id that means "the player picked features one by one".
pub const CUSTOM_PRESET: &str = "custom";

/// Features switched off for `preset` (by id). `"custom"`: the player's own `custom_off` list
/// (ids the pack no longer has are ignored, new features start on). Unknown or no preset:
/// features whose `default` is false.
pub fn disabled_features(
    m: &PackManifest,
    preset: Option<&str>,
    custom_off: &[String],
) -> std::collections::BTreeSet<String> {
    if preset == Some(CUSTOM_PRESET) {
        return m
            .features
            .iter()
            .filter(|f| custom_off.contains(&f.id))
            .map(|f| f.id.clone())
            .collect();
    }
    match preset.and_then(|id| m.presets.iter().find(|p| p.id == id)) {
        Some(p) => p.disable.iter().cloned().collect(),
        None => m
            .features
            .iter()
            .filter(|f| !f.default)
            .map(|f| f.id.clone())
            .collect(),
    }
}

/// "What's new" for a pack version, shown by the launcher.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackChanges {
    /// The version these changes are relative to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<semver::Version>,
    /// Free text from the publisher (`pack-publisher --notes`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Display names of mods and content packs.
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub removed: Vec<String>,
    #[serde(default)]
    pub updated: Vec<String>,
    /// Number of other files (configs etc.) that changed.
    #[serde(default)]
    pub other_files: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loader {
    #[serde(rename = "type")]
    pub kind: LoaderKind,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Fabric,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFile {
    /// Relative to the instance (game) directory, forward slashes.
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
    #[serde(default)]
    pub policy: FilePolicy,
    /// For mod jars: the mod ID read from the jar, so tools can tell an updated
    /// mod (new file name, same ID) from a removed one. Informational only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_id: Option<String>,
    /// The optional feature this file belongs to (see `PackManifest::features`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilePolicy {
    /// Always kept identical to the manifest; removed when dropped from it.
    #[default]
    Managed,
    /// Installed once if missing; afterwards it belongs to the player.
    Default,
}

/// What the launcher installed last time.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Lock {
    pub pack_version: Option<semver::Version>,
    /// Managed files only: path → sha1.
    pub managed: BTreeMap<String, String>,
}

impl Lock {
    pub fn load(instance: &Path) -> Lock {
        std::fs::read(instance.join(LOCK_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, instance: &Path) -> Result<()> {
        let p = instance.join(LOCK_FILE);
        std::fs::create_dir_all(p.parent().expect("lock has parent"))?;
        std::fs::write(p, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

pub async fn fetch_manifest(client: &reqwest::Client, url: &str) -> Result<PackManifest> {
    let resp = client.get(url).send().await?;
    // CloudFront in front of a private bucket answers 403 (not 404) for a missing object.
    if matches!(resp.status().as_u16(), 403 | 404) {
        return Err(Error::PackNotPublished(url.to_string()));
    }
    parse_manifest(&resp.error_for_status()?.bytes().await?)
}

/// Parse `pack.json`, checking `format_version` *before* the rest: a newer format may have
/// fields or values this launcher can't read, and the player should see "update the
/// launcher", not a parse error.
pub fn parse_manifest(bytes: &[u8]) -> Result<PackManifest> {
    #[derive(Deserialize)]
    struct Head {
        format_version: u32,
    }
    let head: Head = serde_json::from_slice(bytes)?;
    if head.format_version != FORMAT_VERSION {
        return Err(Error::other(format!(
            "pack format {} is not supported by this launcher (needs {FORMAT_VERSION}); please update the launcher",
            head.format_version
        )));
    }
    Ok(serde_json::from_slice(bytes)?)
}

/// The pack for a channel: its own `<channel>/pack.json` if published, else Release's.
/// Only "not there" (403/404; CloudFront answers 403 for missing objects) falls back.
/// Network errors and 5xx stay errors so a test channel never silently gets another pack.
pub async fn fetch_manifest_for(
    client: &reqwest::Client,
    release_url: &str,
    channel: Channel,
) -> Result<PackManifest> {
    if let Some(url) = channel.pack_manifest_url(release_url) {
        let resp = client.get(&url).send().await?;
        if !matches!(resp.status().as_u16(), 403 | 404) {
            let bytes = resp.error_for_status()?.bytes().await?;
            return parse_manifest(&bytes);
        }
    }
    fetch_manifest(client, release_url).await
}

/// The plan for a sync, computed without touching the network. Pure for testing.
#[derive(Debug, Default)]
pub struct Plan {
    pub downloads: Vec<(PathBuf, PackFile)>,
    pub deletions: Vec<PathBuf>,
}

/// What to download and delete. Files of a `disabled` feature are left out, so if the
/// launcher installed them before (managed), they are removed; player-owned (default)
/// files are never deleted.
pub fn plan(
    instance: &Path,
    manifest: &PackManifest,
    lock: &Lock,
    disabled: &std::collections::BTreeSet<String>,
) -> Result<Plan> {
    let mut plan = Plan::default();
    // Every path still in the manifest is kept, whatever its policy. A file that
    // moves from managed to default must not be deleted.
    let mut wanted = BTreeMap::new();
    for f in manifest.files.iter().filter(|f| is_enabled(f, disabled)) {
        let dest = safe_join(instance, &f.path)?;
        wanted.insert(f.path.clone(), ());
        match f.policy {
            FilePolicy::Managed => plan.downloads.push((dest, f.clone())),
            FilePolicy::Default if !dest.exists() => plan.downloads.push((dest, f.clone())),
            FilePolicy::Default => {}
        }
    }
    for old in lock.managed.keys() {
        if !wanted.contains_key(old) {
            plan.deletions.push(safe_join(instance, old)?);
        }
    }
    Ok(plan)
}

fn is_enabled(f: &PackFile, disabled: &std::collections::BTreeSet<String>) -> bool {
    f.feature.as_ref().is_none_or(|id| !disabled.contains(id))
}

pub async fn sync(
    client: &reqwest::Client,
    instance: &Path,
    manifest: &PackManifest,
    disabled: &std::collections::BTreeSet<String>,
    progress: &ProgressFn,
) -> Result<()> {
    std::fs::create_dir_all(instance)?;
    let lock = Lock::load(instance);
    let plan = plan(instance, manifest, &lock, disabled)?;

    let jobs = plan
        .downloads
        .iter()
        .map(|(dest, f)| Download {
            url: f.url.clone(),
            dest: dest.clone(),
            sha1: Some(f.sha1.clone()),
        })
        .collect();
    download::download_all(client, jobs, "Updating modpack", progress).await?;

    for (i, p) in plan.deletions.iter().enumerate() {
        report(
            progress,
            "Removing old modpack files",
            i as u64,
            plan.deletions.len() as u64,
        );
        match std::fs::remove_file(p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }

    Lock {
        pack_version: Some(manifest.version.clone()),
        managed: manifest
            .files
            .iter()
            .filter(|f| f.policy == FilePolicy::Managed && is_enabled(f, disabled))
            .map(|f| (f.path.clone(), f.sha1.clone()))
            .collect(),
    }
    .save(instance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_format_asks_for_a_launcher_update() {
        // Format 2 with a loader this launcher doesn't know: the version check comes first.
        let v2 = br#"{"format_version":2,"loader":{"type":"quilt","version":"1"},"files":[]}"#;
        let e = parse_manifest(v2).unwrap_err().to_string();
        assert!(e.contains("please update the launcher"), "{e}");
        assert!(parse_manifest(b"not json").is_err());
    }

    fn file(path: &str, policy: FilePolicy) -> PackFile {
        PackFile {
            path: path.into(),
            sha1: "00".into(),
            size: 1,
            url: "https://example.invalid/x".into(),
            policy,
            mod_id: None,
            feature: None,
        }
    }

    fn manifest(files: Vec<PackFile>) -> PackManifest {
        PackManifest {
            format_version: 1,
            id: "t".into(),
            name: "t".into(),
            version: semver::Version::new(1, 0, 0),
            minecraft: "1.21.1".into(),
            loader: Loader {
                kind: LoaderKind::Fabric,
                version: "0.17.3".into(),
            },
            recommended_memory_mb: None,
            changes: None,
            features: vec![],
            presets: vec![],
            files,
        }
    }

    #[test]
    fn deletes_only_previously_managed_files() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Lock {
            pack_version: None,
            managed: [
                ("mods/old.jar".to_string(), "00".to_string()),
                ("mods/kept.jar".to_string(), "00".to_string()),
            ]
            .into_iter()
            .collect(),
        };
        let m = manifest(vec![file("mods/kept.jar", FilePolicy::Managed)]);
        let p = plan(dir.path(), &m, &lock, &Default::default()).unwrap();
        assert_eq!(p.deletions, vec![dir.path().join("mods").join("old.jar")]);
        assert_eq!(p.downloads.len(), 1);
    }

    #[test]
    fn default_files_only_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("options.txt"), "player tweaks").unwrap();
        let m = manifest(vec![
            file("options.txt", FilePolicy::Default),
            file("servers.dat", FilePolicy::Default),
        ]);
        let p = plan(dir.path(), &m, &Lock::default(), &Default::default()).unwrap();
        let paths: Vec<_> = p.downloads.iter().map(|(_, f)| f.path.as_str()).collect();
        assert_eq!(paths, ["servers.dat"]);
        assert!(p.deletions.is_empty());
    }

    #[test]
    fn managed_to_default_is_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("config").join("x.toml"), "player edit").unwrap();
        let lock = Lock {
            pack_version: None,
            managed: [("config/x.toml".to_string(), "00".to_string())]
                .into_iter()
                .collect(),
        };
        let m = manifest(vec![file("config/x.toml", FilePolicy::Default)]);
        let p = plan(dir.path(), &m, &lock, &Default::default()).unwrap();
        assert!(p.deletions.is_empty());
        assert!(p.downloads.is_empty());
    }

    #[test]
    fn disabled_feature_files_are_skipped_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mut shader = file("shaderpacks/a.zip", FilePolicy::Managed);
        shader.feature = Some("shaders".into());
        let mut m = manifest(vec![file("mods/core.jar", FilePolicy::Managed), shader]);
        m.features = vec![Feature {
            id: "shaders".into(),
            name: "Shaders".into(),
            description: None,
            default: true,
        }];
        m.presets = vec![Preset {
            id: "low".into(),
            name: "Low".into(),
            description: None,
            disable: vec!["shaders".into()],
        }];
        // No preset: everything on.
        let all = disabled_features(&m, None, &[]);
        assert!(all.is_empty());
        let p = plan(dir.path(), &m, &Lock::default(), &all).unwrap();
        assert_eq!(p.downloads.len(), 2);
        // Low preset after shaders were installed: skipped and removed.
        let low = disabled_features(&m, Some("low"), &[]);
        let lock = Lock {
            pack_version: None,
            managed: [("shaderpacks/a.zip".to_string(), "00".to_string())]
                .into_iter()
                .collect(),
        };
        let p = plan(dir.path(), &m, &lock, &low).unwrap();
        assert_eq!(p.downloads.len(), 1);
        assert_eq!(
            p.deletions,
            vec![dir.path().join("shaderpacks").join("a.zip")]
        );
        // An unknown preset falls back to the feature defaults.
        assert!(disabled_features(&m, Some("nope"), &[]).is_empty());
        // Custom: the player's own list, ignoring features the pack no longer has.
        let custom = disabled_features(
            &m,
            Some(CUSTOM_PRESET),
            &["shaders".to_string(), "gone".to_string()],
        );
        assert_eq!(
            custom.into_iter().collect::<Vec<_>>(),
            vec!["shaders".to_string()]
        );
        assert!(disabled_features(&m, Some(CUSTOM_PRESET), &[]).is_empty());
    }

    #[test]
    fn rejects_traversal_in_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest(vec![file("../../evil.jar", FilePolicy::Managed)]);
        assert!(plan(dir.path(), &m, &Lock::default(), &Default::default()).is_err());
    }

    #[test]
    fn rejects_traversal_in_lock() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Lock {
            pack_version: None,
            managed: [("../outside.txt".to_string(), "00".to_string())]
                .into_iter()
                .collect(),
        };
        assert!(plan(dir.path(), &manifest(vec![]), &lock, &Default::default()).is_err());
    }
}
