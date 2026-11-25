//! Shared logic for `pack-builder` (writes an upload folder) and `pack-publisher`
//! (uploads straight to S3): turn a Prism / Modrinth App / CurseForge instance (folder or
//! zip) into a `PackManifest` for Fabric. Jars that Modrinth knows by hash point at Modrinth's CDN; every
//! other file is self-hosted under `<base_url>/files/<sha1>/<path>` (see [`hosted_path`]).
//!
//! The input is only ever READ; nothing in the source instance is modified.

pub mod diff;
pub mod features;
pub mod instances;
pub mod suggest;
pub mod versions;

use launcher_core::download::sha1_file;
use launcher_core::pack::{FilePolicy, Loader, LoaderKind, PackFile, PackManifest, FORMAT_VERSION};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Where progress lines go: the console for the CLIs, the window for the GUI.
pub type Log = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

/// Print each line to stdout (what the command-line tools use).
pub fn stdout_log() -> Log {
    std::sync::Arc::new(|line: &str| println!("{line}"))
}

/// `options.txt` and `servers.dat` aren't included: the game writes them for whoever played
/// the source instance (their settings and server list), and the launcher joins the server
/// itself. `pack-builder --include` can still add a curated copy (shipped as `default`).
pub const DEFAULT_INCLUDE: &[&str] = &["mods", "config", "resourcepacks", "shaderpacks"];
/// Installed once if missing, then the player's: settings and configs they change. This is
/// also how `config/iris.properties` preselects the club shader without overriding a
/// player's later choice.
pub const DEFAULT_DEFAULT_PREFIX: &[&str] = &["config/", "servers.dat", "options.txt"];
pub const DEFAULT_MANAGED_PREFIX: &[&str] = &[];
/// Never shipped even if inside an included folder. Sodium's fingerprint is a per-install ID
/// the game writes on launch; it must not become every player's.
pub const DEFAULT_EXCLUDE: &[&str] = &["config/sodium-fingerprint.json"];

pub struct BuildOptions {
    /// Instance folder (already extracted if it came from a zip).
    pub root: PathBuf,
    pub pack_version: semver::Version,
    pub base_url: String,
    pub id: String,
    pub name: String,
    /// Override the instance's Minecraft version (needed for plain folders).
    pub minecraft: Option<String>,
    /// Override the instance's Fabric Loader version (needed for plain folders).
    pub fabric: Option<String>,
    pub memory_mb: u32,
    pub include: Vec<String>,
    pub default_prefix: Vec<String>,
    pub managed_prefix: Vec<String>,
    /// Path prefixes left out even if inside an included folder.
    pub exclude: Vec<String>,
    pub use_modrinth: bool,
    /// Progress and notes ("Hashing 1837 files …"). Default: stdout.
    pub log: Log,
}

impl BuildOptions {
    pub fn new(root: PathBuf, pack_version: semver::Version, base_url: String) -> Self {
        let own = |l: &[&str]| l.iter().map(|s| s.to_string()).collect();
        BuildOptions {
            root,
            pack_version,
            base_url,
            id: "imaginears-magic-pack".into(),
            name: "Imaginears Magic Pack".into(),
            minecraft: None,
            fabric: None,
            memory_mb: 4096,
            include: own(DEFAULT_INCLUDE),
            default_prefix: own(DEFAULT_DEFAULT_PREFIX),
            managed_prefix: own(DEFAULT_MANAGED_PREFIX),
            exclude: own(DEFAULT_EXCLUDE),
            use_modrinth: true,
            log: stdout_log(),
        }
    }
}

pub struct Built {
    pub manifest: PackManifest,
    /// Self-hosted files: (relative path, file on disk). Upload each to
    /// `<base>/files/<hosted_path>` (see [`hosted_path`]).
    pub self_hosted: Vec<(String, PathBuf)>,
    /// Things worth a look that don't stop publishing (also sent to `log`).
    pub warnings: Vec<String>,
}

impl Built {
    pub fn self_hosted_mods(&self) -> impl Iterator<Item = &str> {
        self.self_hosted
            .iter()
            .map(|(r, _)| r.as_str())
            .filter(|r| r.starts_with("mods/"))
    }
}

/// Files that are never worth shipping, wherever they are: config backups mods leave behind
/// and temporary files.
pub fn is_junk(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.ends_with(".bak") || lower.ends_with(".outdated") || lower.ends_with(".tmp")
}

/// Where the game files of the instance at `root` are (Prism: `.minecraft/`, CurseForge
/// export: `overrides/`, otherwise the folder itself).
pub fn content_root(root: &Path) -> PathBuf {
    instances::read(root)
        .map(|i| i.game_dir)
        .unwrap_or_else(|| root.to_path_buf())
}

pub async fn build(opts: BuildOptions) -> Result<Built, BoxError> {
    let log = opts.log.clone();
    let mut warnings = Vec::new();
    let inst = instances::read(&opts.root).ok_or(
        "not a pack instance: expected a Prism Launcher, Modrinth App or CurseForge \
         instance, or a folder with a mods/ folder",
    )?;
    if let Some(l) = inst.loader.as_deref().filter(|_| inst.fabric.is_none()) {
        if opts.fabric.is_none() {
            return Err(format!(
                "this instance uses {l}, not Fabric. The Imaginears Magic Pack is a Fabric pack."
            )
            .into());
        }
    }
    let root = inst.game_dir.clone();
    if !root.join("mods").is_dir() {
        return Err(format!(
            "{} has no mods/ folder. A CurseForge export lists mods by project ID only; \
             use the instance folder instead.",
            root.display()
        )
        .into());
    }
    let (feature_cfg, feature_src) = features::Config::for_instance(&opts.root)?;
    log(&format!(
        "Optional features ({feature_src} {}): {}",
        features::FILE_NAME,
        feature_cfg
            .features
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    let minecraft = opts
        .minecraft
        .clone()
        .or((!inst.minecraft.is_empty()).then(|| inst.minecraft.clone()))
        .ok_or("could not detect the Minecraft version; pass --minecraft")?;
    let fabric = opts
        .fabric
        .clone()
        .or(inst.fabric.clone())
        .ok_or("could not detect the Fabric Loader version; pass --fabric")?;

    // Checks that would otherwise only show up as a crash on players' PCs.
    let mods = suggest::scan_mods(&root);
    // A mod that requires a switchable one must switch off with it.
    let problems = suggest::check_optional_deps(&mods, &feature_cfg);
    if !problems.is_empty() {
        return Err(format!(
            "optional features would crash players:\n  {}",
            problems.join("\n  ")
        )
        .into());
    }
    // Every mod must accept the pack's Fabric Loader and Minecraft versions (Fabric refuses
    // to start otherwise).
    if let Some(msg) = suggest::check_loader_versions(&mods, &fabric, &minecraft).error {
        return Err(msg.into());
    }

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for inc in &opts.include {
        let p = root.join(inc);
        if p.is_file() {
            files.push((inc.replace('\\', "/"), p));
        } else if p.is_dir() {
            for e in walkdir::WalkDir::new(&p).into_iter().filter_map(Result::ok) {
                if e.file_type().is_file() {
                    let rel = e
                        .path()
                        .strip_prefix(&root)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.push((rel, e.path().to_path_buf()));
                }
            }
        }
    }
    files.retain(|(rel, _)| {
        !opts.exclude.iter().any(|x| rel.starts_with(x.as_str())) && !is_junk(rel)
    });
    files.sort();
    log(&format!("Hashing {} files …", files.len()));

    let mut hashed = Vec::with_capacity(files.len());
    for (rel, path) in files {
        let sha1 = sha1_file(&path).await?;
        let size = std::fs::metadata(&path)?.len();
        hashed.push((rel, path, sha1, size));
    }

    let modrinth = if opts.use_modrinth {
        let candidates: Vec<String> = hashed
            .iter()
            .filter(|(rel, ..)| rel.ends_with(".jar") || rel.ends_with(".zip"))
            .map(|(_, _, h, _)| h.clone())
            .collect();
        log(&format!(
            "Looking up {} jars/zips on Modrinth …",
            candidates.len()
        ));
        modrinth_lookup(&candidates).await.unwrap_or_else(|e| {
            let w = format!("Modrinth lookup failed ({e}); self-hosting everything");
            log(&format!("warning: {w}"));
            warnings.push(w);
            HashMap::new()
        })
    } else {
        HashMap::new()
    };

    let base = opts.base_url.trim_end_matches('/');
    let mut pack_files = Vec::new();
    let mut self_hosted = Vec::new();
    for (rel, path, sha1, size) in hashed {
        let url = match modrinth.get(&sha1) {
            Some(u) => u.clone(),
            None => {
                self_hosted.push((rel.clone(), path.clone()));
                format!("{base}/files/{}", encode_path(&hosted_path(&sha1, &rel)))
            }
        };
        let mod_id = (rel.starts_with("mods/") && rel.ends_with(".jar"))
            .then(|| read_mod_id(&path))
            .flatten();
        let policy = policy_for(&rel, &opts.default_prefix, &opts.managed_prefix);
        let feature = feature_cfg.feature_for(&rel);
        pack_files.push(PackFile {
            path: rel,
            sha1,
            size,
            url,
            policy,
            mod_id,
            feature,
        });
    }

    Ok(Built {
        manifest: PackManifest {
            format_version: FORMAT_VERSION,
            id: opts.id,
            name: opts.name,
            version: opts.pack_version,
            minecraft,
            loader: Loader {
                kind: LoaderKind::Fabric,
                version: fabric,
            },
            recommended_memory_mb: Some(opts.memory_mb),
            changes: None,
            features: feature_cfg.manifest_features(),
            presets: feature_cfg.presets.clone(),
            files: pack_files,
        },
        self_hosted,
        warnings,
    })
}

pub fn policy_for(rel: &str, default: &[String], managed: &[String]) -> FilePolicy {
    let best = |list: &[String]| {
        list.iter()
            .filter(|p| rel.starts_with(p.as_str()))
            .map(String::len)
            .max()
    };
    match (best(default), best(managed)) {
        (Some(d), Some(m)) if d > m => FilePolicy::Default,
        (Some(_), None) => FilePolicy::Default,
        _ => FilePolicy::Managed,
    }
}

/// Where a self-hosted file lives under `files/`: `<sha1>/<path>`. Content-addressed, so
/// publishing a changed file never overwrites the object the live `pack.json` still points
/// to (a failed or half-finished publish leaves the live pack intact), and an unchanged file
/// keeps its URL. Old objects stay in the bucket.
pub fn hosted_path(sha1: &str, rel: &str) -> String {
    format!("{sha1}/{rel}")
}

/// Percent-encodes each path segment for use in a URL. S3 object keys use the raw path.
pub fn encode_path(rel: &str) -> String {
    rel.split('/')
        .map(|seg| {
            seg.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The first mod ID declared in a jar (Fabric/Quilt json, or Forge-style mods.toml).
pub fn read_mod_id(jar: &Path) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar).ok()?).ok()?;
    let mut read = |name: &str| -> Option<String> {
        let mut s = String::new();
        zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
        Some(s)
    };
    for toml in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Some(id) = read(toml).and_then(|t| toml_mod_id(&t)) {
            return Some(id);
        }
    }
    for json in ["fabric.mod.json", "quilt.mod.json"] {
        if let Some(v) = read(json).and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
        {
            let id = v
                .get("id")
                .or_else(|| v.pointer("/quilt_loader/id"))
                .and_then(|i| i.as_str());
            if let Some(id) = id {
                return Some(id.to_string());
            }
        }
    }
    None
}

fn toml_mod_id(toml: &str) -> Option<String> {
    toml.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("modId")?.trim_start();
        let rest = rest.strip_prefix('=')?.trim();
        let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let inner = &rest[1..];
        Some(inner[..inner.find(q)?].to_string()).filter(|s| !s.is_empty())
    })
}

#[derive(Deserialize)]
struct ModrinthVersion {
    files: Vec<ModrinthFile>,
}

#[derive(Deserialize)]
struct ModrinthFile {
    url: String,
    hashes: HashMap<String, String>,
}

async fn modrinth_lookup(hashes: &[String]) -> Result<HashMap<String, String>, BoxError> {
    let client = launcher_core::http_client();
    let mut out = HashMap::new();
    for chunk in hashes.chunks(500) {
        let resp: HashMap<String, ModrinthVersion> = client
            .post("https://api.modrinth.com/v2/version_files")
            .json(&serde_json::json!({ "hashes": chunk, "algorithm": "sha1" }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        for (hash, version) in resp {
            if let Some(f) = version
                .files
                .into_iter()
                .find(|f| f.hashes.get("sha1") == Some(&hash))
            {
                out.insert(hash, f.url);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_prefix_wins() {
        let d = vec!["config/".to_string(), "servers.dat".to_string()];
        let m = vec!["config/imaginears/".to_string()];
        assert_eq!(
            policy_for("config/sodium-options.json", &d, &m),
            FilePolicy::Default
        );
        assert_eq!(
            policy_for("config/iris.properties", &d, &m),
            FilePolicy::Default
        );
        assert_eq!(
            policy_for("config/imaginears/x.txt", &d, &m),
            FilePolicy::Managed
        );
        assert_eq!(policy_for("mods/a.jar", &d, &m), FilePolicy::Managed);
        assert_eq!(
            policy_for("shaderpacks/ComplementaryReimagined_r5.7.1.zip", &d, &m),
            FilePolicy::Managed
        );
        assert_eq!(policy_for("servers.dat", &d, &m), FilePolicy::Default);
    }

    #[test]
    fn junk_is_skipped_but_real_data_kept() {
        assert!(is_junk("config/somemod-client.toml.bak"));
        assert!(is_junk("mods/old.jar.outdated"));
        assert!(!is_junk("config/sodium-options.json"));
        assert!(!is_junk("shaderpacks/ComplementaryReimagined_r5.7.1.zip"));
    }

    #[test]
    fn hosted_paths_change_with_content() {
        assert_eq!(
            hosted_path("ab12", "config/iris.properties"),
            "ab12/config/iris.properties"
        );
        assert_ne!(hosted_path("ab12", "a.jar"), hosted_path("cd34", "a.jar"));
    }

    #[test]
    fn encodes_url_segments() {
        assert_eq!(encode_path("mods/My Mod+1.jar"), "mods/My%20Mod%2B1.jar");
    }

    #[test]
    fn parses_mod_id_from_toml() {
        let t = "modLoader=\"javafml\"\n[[mods]]\n  modId = \"sophisticatedbackpacks\"\n";
        assert_eq!(toml_mod_id(t).as_deref(), Some("sophisticatedbackpacks"));
        assert_eq!(toml_mod_id("modId='x'").as_deref(), Some("x"));
        assert_eq!(toml_mod_id("modIdFoo = \"no\""), None);
    }
}
