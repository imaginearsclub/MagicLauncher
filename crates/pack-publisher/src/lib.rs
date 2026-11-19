//! The publishing engine shared by `pack-publisher` (command line) and
//! `Imaginears PackPublisher` (window): build the pack from a Prism / Modrinth App / CurseForge instance, compare it
//! with the live one, upload changed files + `pack.json` to S3, clear the CloudFront cache;
//! and maintenance mode.
//!
//! Nothing here reads from stdin: questions (which instance, which version, AWS keys) are the
//! caller's job, so the GUI can never freeze waiting for typed input. Progress goes to a
//! [`Log`] callback.

use aws_sdk_s3::primitives::ByteStream;
use futures_util::{stream, TryStreamExt};
use launcher_core::pack::PackManifest;
use pack_builder::diff::{self, Diff};
use pack_builder::{build, instances, BuildOptions, Built};
pub use pack_builder::{stdout_log, BoxError, Log};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const KEYRING_SERVICE: &str = "club.imaginears.pack-publisher";
pub const KEYRING_USER: &str = "aws";

// ---------- where the pack lives ----------

/// The bucket/CDN a pack is published to. `channel` = a test-channel pack ("beta", "alpha"),
/// with its own `pack.json` and files under `<channel>/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub base_url: String,
    pub bucket: String,
    pub prefix: String,
    pub distribution_id: String,
    pub region: String,
    pub channel: Option<String>,
}

// Default publish target: bucket `magiclauncher`, CloudFront distribution E2AZL86MSJLTJV
// (origin path `/pack`). Each can also be set with a CLI flag.
pub const DEFAULT_BASE_URL: &str = "https://launcher.imaginears.club/";
pub const DEFAULT_BUCKET: &str = "magiclauncher";
pub const DEFAULT_PREFIX: &str = "pack";
pub const DEFAULT_DISTRIBUTION_ID: &str = "E2AZL86MSJLTJV";
pub const DEFAULT_REGION: &str = "us-east-1";

impl Default for Target {
    fn default() -> Self {
        Target {
            base_url: DEFAULT_BASE_URL.into(),
            bucket: DEFAULT_BUCKET.into(),
            prefix: DEFAULT_PREFIX.into(),
            distribution_id: DEFAULT_DISTRIBUTION_ID.into(),
            region: DEFAULT_REGION.into(),
            channel: None,
        }
    }
}

impl Target {
    /// Public URL of this channel's pack (no trailing slash), e.g. `https://…/beta`.
    pub fn pack_base(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        match &self.channel {
            Some(ch) => format!("{base}/{ch}"),
            None => base.to_string(),
        }
    }

    /// S3 key prefix of this channel's pack, e.g. `pack/beta`.
    pub fn pack_prefix(&self) -> String {
        let p = self.prefix.trim_matches('/');
        match &self.channel {
            Some(ch) => format!("{p}/{ch}"),
            None => p.to_string(),
        }
    }

    fn s3(&self, creds: &Credentials) -> aws_sdk_s3::Client {
        aws_sdk_s3::Client::from_conf(
            aws_sdk_s3::Config::builder()
                .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
                .region(aws_sdk_s3::config::Region::new(self.region.clone()))
                .credentials_provider(creds.clone())
                .build(),
        )
    }

    /// Clear CloudFront's cache for `path` (e.g. "/*").
    async fn invalidate(
        &self,
        creds: &Credentials,
        path: &str,
        reference: &str,
    ) -> Result<(), String> {
        if self.distribution_id.trim().is_empty() {
            return Err("no CloudFront distribution ID is set (--distribution-id)".into());
        }
        let cf = aws_sdk_cloudfront::Client::from_conf(
            aws_sdk_cloudfront::Config::builder()
                .behavior_version(aws_sdk_cloudfront::config::BehaviorVersion::latest())
                .region(aws_sdk_cloudfront::config::Region::new("us-east-1"))
                .credentials_provider(creds.clone())
                .build(),
        );
        use aws_sdk_cloudfront::types::{InvalidationBatch, Paths};
        let batch = InvalidationBatch::builder()
            .caller_reference(format!("{reference}-{}", now_millis()))
            .paths(
                Paths::builder()
                    .quantity(1)
                    .items(path)
                    .build()
                    .map_err(|e| e.to_string())?,
            )
            .build()
            .map_err(|e| e.to_string())?;
        cf.create_invalidation()
            .distribution_id(&self.distribution_id)
            .invalidation_batch(batch)
            .send()
            .await
            .map_err(|e| aws_sdk_cloudfront::error::DisplayErrorContext(e).to_string())?;
        Ok(())
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

// ---------- preparing a publish ----------

/// What a publish would do, before anything is uploaded.
pub struct Prepared {
    pub instance: PathBuf,
    pub live: Option<PackManifest>,
    pub built: Built,
    /// `None` = first publish.
    pub diff: Option<Diff>,
    pub suggested: semver::Version,
    /// Self-hosted files to upload: (path in the pack, file on disk).
    pub uploads: Vec<(String, PathBuf)>,
    pub upload_bytes: u64,
    /// Self-hosted mods that weren't self-hosted in the live pack: check their licenses.
    pub new_self_hosted: Vec<String>,
}

impl Prepared {
    /// Nothing to publish: the live pack already matches the instance.
    pub fn unchanged(&self) -> bool {
        self.diff.as_ref().is_some_and(Diff::is_empty)
    }
}

#[derive(Debug, Clone, Default)]
pub struct PrepareOptions {
    /// Override the instance's Minecraft version (needed for plain folders).
    pub minecraft: Option<String>,
    /// Override the instance's Fabric Loader version.
    pub fabric: Option<String>,
    /// Recommended memory (MiB). `None` = the builder's default.
    pub memory_mb: Option<u32>,
    /// Upload every self-hosted file, not only changed ones.
    pub reupload_all: bool,
}

/// Fetch the live pack, build the instance and work out what changed. Uploads nothing.
pub async fn prepare(
    target: &Target,
    instance: &Path,
    opts: &PrepareOptions,
    log: Log,
) -> Result<Prepared, BoxError> {
    let base = target.pack_base();
    let live = fetch_live(&format!("{base}/pack.json")).await?;
    log(&match &live {
        Some(l) => format!("Checking the live pack … version {}", l.version),
        None => "Checking the live pack … none yet (first publish)".to_string(),
    });
    let start = live
        .as_ref()
        .map(|l| l.version.clone())
        .unwrap_or_else(|| semver::Version::new(1, 0, 0));
    let mut bopts = BuildOptions::new(instance.to_path_buf(), start, base.clone());
    bopts.minecraft = opts.minecraft.clone();
    bopts.fabric = opts.fabric.clone();
    if let Some(mb) = opts.memory_mb {
        bopts.memory_mb = mb;
    }
    bopts.log = log;
    let built = build(bopts).await?;

    let d = live.as_ref().map(|l| diff::diff(l, &built.manifest));
    let suggested = match (&live, &d) {
        (Some(l), Some(d)) => diff::next_version(&l.version, d.bump()),
        _ => semver::Version::new(1, 0, 0),
    };
    let uploads = files_to_upload(&built, live.as_ref(), &base, opts.reupload_all);
    let upload_bytes = uploads
        .iter()
        .map(|(_, p)| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();
    let new_self_hosted = match &live {
        Some(l) => built
            .self_hosted_mods()
            .filter(|m| {
                !l.files
                    .iter()
                    .any(|f| f.path == *m && f.url.starts_with(&base))
            })
            .map(String::from)
            .collect(),
        None => Vec::new(),
    };
    Ok(Prepared {
        instance: instance.to_path_buf(),
        live,
        built,
        diff: d,
        suggested,
        uploads,
        upload_bytes,
        new_self_hosted,
    })
}

/// Set the version and the players' "What's new" (diff + optional note). The version must
/// be higher than the live one unless `force`.
pub fn finalize(
    p: &mut Prepared,
    version: semver::Version,
    notes: Option<String>,
    force: bool,
) -> Result<(), BoxError> {
    check_version(p.live.as_ref(), &version, force)?;
    p.built.manifest.version = version;
    p.built.manifest.changes = match (&p.live, &p.diff) {
        (Some(l), Some(d)) => Some(d.changes(Some(l.version.clone()), notes)),
        _ => notes
            .filter(|n| !n.trim().is_empty())
            .map(|n| launcher_core::pack::PackChanges {
                notes: Some(n),
                ..Default::default()
            }),
    };
    Ok(())
}

fn check_version(
    live: Option<&PackManifest>,
    version: &semver::Version,
    force: bool,
) -> Result<(), BoxError> {
    if let Some(l) = live {
        if *version <= l.version && !force {
            return Err(format!(
                "version {version} must be higher than the live {}",
                l.version
            )
            .into());
        }
    }
    Ok(())
}

/// Upload changed files, then `pack.json`, then clear the CDN cache. Re-checks the live
/// version first (someone may have published since `prepare`). `progress(done, total)` is
/// called after each file.
pub async fn publish(
    target: &Target,
    p: &Prepared,
    creds: &Credentials,
    force: bool,
    log: Log,
    progress: &(dyn Fn(usize, usize) + Send + Sync),
) -> Result<(), BoxError> {
    let version = p.built.manifest.version.clone();
    let live_now = fetch_live(&format!("{}/pack.json", target.pack_base())).await?;
    check_version(live_now.as_ref(), &version, force)?;

    let s3 = target.s3(creds);
    let prefix = target.pack_prefix();
    let total = p.uploads.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    stream::iter(p.uploads.iter().map(Ok::<_, BoxError>))
        .try_for_each_concurrent(6, |(rel, path)| {
            let (s3, bucket, done) = (&s3, &target.bucket, &done);
            let key = format!("{prefix}/files/{rel}");
            async move {
                put(s3, bucket, &key, path, content_type(rel), None)
                    .await
                    .map_err(|e| format!("uploading {rel}: {e}"))?;
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                progress(n, total);
                Ok(())
            }
        })
        .await?;

    let manifest_path = std::env::temp_dir().join("magiclauncher-pack.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&p.built.manifest)?,
    )?;
    let r = put(
        &s3,
        &target.bucket,
        &format!("{prefix}/pack.json"),
        &manifest_path,
        "application/json",
        Some("no-cache"),
    )
    .await;
    let _ = std::fs::remove_file(&manifest_path);
    r.map_err(|e| format!("uploading pack.json: {e}"))?;
    log("Uploaded pack.json.");

    target
        .invalidate(creds, "/*", &format!("pack-{version}"))
        .await
        .map_err(|e| {
            format!(
                "pack.json is uploaded, but clearing the CDN cache failed: {e}. \
                 Players get the update once the cache expires."
            )
        })?;
    log("Cleared the CDN cache.");
    Ok(())
}

/// Self-hosted files that are new, changed, or were previously served from Modrinth.
pub fn files_to_upload(
    built: &Built,
    live: Option<&PackManifest>,
    base: &str,
    all: bool,
) -> Vec<(String, PathBuf)> {
    built
        .self_hosted
        .iter()
        .filter(|(rel, _)| {
            if all {
                return true;
            }
            let new = built.manifest.files.iter().find(|f| &f.path == rel);
            let old = live.and_then(|l| l.files.iter().find(|f| &f.path == rel));
            match (new, old) {
                (Some(n), Some(o)) => n.sha1 != o.sha1 || !o.url.starts_with(base),
                _ => true,
            }
        })
        .cloned()
        .collect()
}

async fn put(
    s3: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    path: &Path,
    content_type: &str,
    cache_control: Option<&str>,
) -> Result<(), BoxError> {
    let mut req = s3
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type(content_type)
        .body(ByteStream::from_path(path).await?);
    if let Some(c) = cache_control {
        req = req.cache_control(c);
    }
    req.send()
        .await
        .map_err(|e| aws_sdk_s3::error::DisplayErrorContext(e).to_string())?;
    Ok(())
}

fn content_type(rel: &str) -> &'static str {
    let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jar" => "application/java-archive",
        "zip" => "application/zip",
        "json" | "mcmeta" => "application/json",
        "png" => "image/png",
        "toml" | "txt" | "cfg" | "properties" | "json5" | "snbt" | "yml" | "yaml" | "ini" => {
            "text/plain; charset=utf-8"
        }
        _ => "application/octet-stream",
    }
}

pub async fn fetch_live(url: &str) -> Result<Option<PackManifest>, BoxError> {
    let resp = launcher_core::http_client()
        .get(url)
        .header("Cache-Control", "no-cache")
        .send()
        .await?;
    if matches!(resp.status().as_u16(), 403 | 404) {
        return Ok(None);
    }
    Ok(Some(resp.error_for_status()?.json().await?))
}

/// The diff as readable lines (indented), ending with the suggested bump.
pub fn diff_lines(d: &Diff) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((a, b)) = &d.minecraft {
        out.push(format!("  Minecraft: {a} → {b}"));
    }
    if let Some((a, b)) = &d.loader {
        out.push(format!("  Fabric Loader: {a} → {b}"));
    }
    if d.features_changed {
        out.push("  Optional features / presets changed".into());
    }
    if let Some((a, b)) = &d.memory {
        let gb = |m: &Option<u32>| {
            m.map(|v| format!("{:.1} GB", v as f64 / 1024.0))
                .unwrap_or("none".into())
        };
        out.push(format!("  Recommended memory: {} → {}", gb(a), gb(b)));
    }
    let name = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    let mut list = |title: &str, items: Vec<String>| {
        if items.is_empty() {
            return;
        }
        out.push(format!("  {title} ({}):", items.len()));
        for i in items.iter().take(25) {
            out.push(format!("    {i}"));
        }
        if items.len() > 25 {
            out.push(format!("    … and {} more", items.len() - 25));
        }
    };
    list(
        "Mods & packs added",
        d.mods_added.iter().map(|p| name(p)).collect(),
    );
    list(
        "Mods & packs removed",
        d.mods_removed.iter().map(|p| name(p)).collect(),
    );
    list(
        "Mods & packs updated",
        d.mods_updated
            .iter()
            .map(|(a, b)| {
                if a == b {
                    name(a)
                } else {
                    format!("{} → {}", name(a), name(b))
                }
            })
            .collect(),
    );
    list("Other files added", d.files_added.clone());
    list("Other files removed", d.files_removed.clone());
    list("Other files changed", d.files_changed.clone());
    out.push(format!("  Suggested bump: {}", bump_reason(d.bump())));
    out
}

pub fn bump_reason(b: diff::Bump) -> &'static str {
    match b {
        diff::Bump::Major => {
            "MAJOR (mods or packs removed, or Minecraft changed: may break worlds)"
        }
        diff::Bump::Minor => "MINOR (mods or packs added)",
        diff::Bump::Patch => "PATCH (updates and config changes only)",
    }
}

// ---------- maintenance mode ----------

/// Turn maintenance on (with a message) or off: writes `launcher/status.json` (root, not per
/// channel) and clears it from the CDN cache.
pub async fn set_maintenance(
    target: &Target,
    on: bool,
    message: Option<String>,
    creds: &Credentials,
) -> Result<(), BoxError> {
    let status = launcher_core::maintenance::Maintenance {
        maintenance: on,
        message: if on {
            message
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
        } else {
            None
        },
        since: on.then(|| (now_millis() / 1000) as u64),
    };
    let tmp = std::env::temp_dir().join("magiclauncher-status.json");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&status)?)?;
    let key = format!("{}/launcher/status.json", target.prefix.trim_matches('/'));
    let r = put(
        &target.s3(creds),
        &target.bucket,
        &key,
        &tmp,
        "application/json",
        Some("no-cache"),
    )
    .await;
    let _ = std::fs::remove_file(&tmp);
    r.map_err(|e| format!("uploading status.json: {e}"))?;
    target
        .invalidate(creds, "/launcher/status.json", "maintenance")
        .await
        .map_err(|e| format!("status.json is uploaded, but clearing the CDN cache failed: {e}"))?;
    Ok(())
}

/// The live maintenance flag (`None` = no status file yet = off).
pub async fn fetch_maintenance(
    target: &Target,
) -> Result<Option<launcher_core::maintenance::Maintenance>, BoxError> {
    let url = format!(
        "{}/launcher/status.json",
        target.base_url.trim_end_matches('/')
    );
    let resp = launcher_core::http_client()
        .get(url)
        .header("Cache-Control", "no-cache")
        .send()
        .await?;
    if matches!(resp.status().as_u16(), 403 | 404) {
        return Ok(None);
    }
    Ok(Some(resp.error_for_status()?.json().await?))
}

// ---------- remembered instances ----------

#[derive(Default, Serialize, Deserialize)]
pub struct Saved {
    /// The Release pack's instance.
    pub instance: Option<PathBuf>,
    /// Instances for test-channel packs ("beta", "alpha").
    #[serde(default)]
    pub channels: std::collections::BTreeMap<String, PathBuf>,
}

fn saved_path() -> Option<PathBuf> {
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(dir.join("ImaginearsClub").join("pack-publisher.json"))
}

pub fn load_saved() -> Saved {
    saved_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// The instance remembered for `channel` (None = Release), if it still exists.
pub fn remembered_instance(channel: Option<&str>) -> Option<PathBuf> {
    let saved = load_saved();
    let p = match channel {
        Some(ch) => saved.channels.get(ch).cloned(),
        None => saved.instance,
    };
    p.filter(|p| instances::read(p).is_some())
}

pub fn save_instance(path: &Path, channel: Option<&str>) {
    if let Some(p) = saved_path() {
        let _ = std::fs::create_dir_all(p.parent().expect("has parent"));
        let mut saved = load_saved();
        match channel {
            Some(ch) => {
                saved.channels.insert(ch.to_string(), path.to_path_buf());
            }
            None => saved.instance = Some(path.to_path_buf()),
        }
        let _ = std::fs::write(p, serde_json::to_vec_pretty(&saved).unwrap_or_default());
    }
}

// ---------- AWS keys ----------

pub type Credentials = aws_sdk_s3::config::Credentials;

#[derive(Serialize, Deserialize)]
struct StoredKeys {
    access_key_id: String,
    secret_access_key: String,
}

/// Keys from the standard AWS environment variables (scripts/CI win over the stored key).
pub fn env_credentials() -> Option<Credentials> {
    let id = std::env::var("AWS_ACCESS_KEY_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let secret = std::env::var("AWS_SECRET_ACCESS_KEY")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(Credentials::new(
        id,
        secret,
        None,
        None,
        "pack-publisher-env",
    ))
}

/// Keys saved in this computer's credential manager / keychain.
pub fn stored_credentials() -> Option<Credentials> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?;
    let k: StoredKeys = serde_json::from_str(&entry.get_password().ok()?).ok()?;
    Some(Credentials::new(
        k.access_key_id,
        k.secret_access_key,
        None,
        None,
        "pack-publisher-keychain",
    ))
}

/// Save keys in the credential manager (not in a file).
pub fn store_credentials(access_key_id: &str, secret: &str) -> Result<Credentials, BoxError> {
    let (id, secret) = (access_key_id.trim(), secret.trim());
    if id.is_empty() || secret.is_empty() {
        return Err("no access key entered".into());
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
    entry.set_password(&serde_json::to_string(&StoredKeys {
        access_key_id: id.to_string(),
        secret_access_key: secret.to_string(),
    })?)?;
    Ok(Credentials::new(
        id,
        secret,
        None,
        None,
        "pack-publisher-keychain",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_core::pack::{FilePolicy, Loader, LoaderKind, PackFile};

    fn file(path: &str, sha1: &str, url: &str) -> PackFile {
        PackFile {
            path: path.into(),
            sha1: sha1.into(),
            size: 1,
            url: url.into(),
            policy: FilePolicy::Managed,
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
                version: "1".into(),
            },
            recommended_memory_mb: None,
            changes: None,
            features: vec![],
            presets: vec![],
            files,
        }
    }

    #[test]
    fn uploads_only_new_changed_or_previously_modrinth_files() {
        let b = "https://pack.example";
        let built = Built {
            manifest: manifest(vec![
                file(
                    "mods/same.jar",
                    "1",
                    "https://pack.example/files/mods/same.jar",
                ),
                file(
                    "mods/changed.jar",
                    "2",
                    "https://pack.example/files/mods/changed.jar",
                ),
                file(
                    "mods/was-mr.jar",
                    "1",
                    "https://pack.example/files/mods/was-mr.jar",
                ),
                file(
                    "mods/new name.jar",
                    "1",
                    "https://pack.example/files/mods/new%20name.jar",
                ),
            ]),
            self_hosted: [
                "mods/same.jar",
                "mods/changed.jar",
                "mods/was-mr.jar",
                "mods/new name.jar",
            ]
            .iter()
            .map(|r| (r.to_string(), PathBuf::from(r)))
            .collect(),
            warnings: vec![],
        };
        let live = manifest(vec![
            file(
                "mods/same.jar",
                "1",
                "https://pack.example/files/mods/same.jar",
            ),
            file(
                "mods/changed.jar",
                "1",
                "https://pack.example/files/mods/changed.jar",
            ),
            file("mods/was-mr.jar", "1", "https://cdn.modrinth.com/x.jar"),
        ]);
        let up: Vec<_> = files_to_upload(&built, Some(&live), b, false)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        // Raw path (with the space) is what becomes the S3 key.
        assert_eq!(
            up,
            ["mods/changed.jar", "mods/was-mr.jar", "mods/new name.jar"]
        );
        assert_eq!(files_to_upload(&built, Some(&live), b, true).len(), 4);
    }

    #[test]
    fn version_must_rise_unless_forced() {
        let live = manifest(vec![]);
        let v = |s: &str| semver::Version::parse(s).unwrap();
        assert!(check_version(Some(&live), &v("1.0.0"), false).is_err());
        assert!(check_version(Some(&live), &v("1.0.0"), true).is_ok());
        assert!(check_version(Some(&live), &v("1.0.1"), false).is_ok());
        assert!(check_version(None, &v("0.1.0"), false).is_ok());
    }

    #[test]
    fn channel_paths() {
        let mut t = Target::default();
        assert_eq!(t.pack_base(), "https://launcher.imaginears.club");
        assert_eq!(t.pack_prefix(), "pack");
        t.channel = Some("beta".into());
        assert_eq!(t.pack_base(), "https://launcher.imaginears.club/beta");
        assert_eq!(t.pack_prefix(), "pack/beta");
    }
}
