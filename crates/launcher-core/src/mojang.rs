//! Vanilla game metadata (piston-meta), libraries, assets and version merging.

use crate::download::{self, Download};
use crate::paths::Paths;
use crate::progress::ProgressFn;
use crate::rules::{self, Env, Rule};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RESOURCES: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
struct VersionManifest {
    versions: Vec<ManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    id: String,
    url: String,
    sha1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<VersionDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: ArgValue },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

impl Argument {
    /// The argument strings that apply in `env`.
    pub fn resolve(&self, env: &Env) -> Vec<String> {
        match self {
            Argument::Plain(s) => vec![s.clone()],
            Argument::Conditional { rules: r, value } if rules::allowed(r, env) => match value {
                ArgValue::One(s) => vec![s.clone()],
                ArgValue::Many(v) => v.clone(),
            },
            Argument::Conditional { .. } => vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    /// Maven repository base for libraries without `downloads` (Fabric profiles).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// SHA-1 given next to `url` (Fabric meta lists it for most libraries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<LoggingEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingEntry {
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

/// `group:artifact:version[:classifier][@ext]` → maven relative path.
pub fn maven_path(coord: &str) -> Option<String> {
    let (coord, ext) = coord.split_once('@').unwrap_or((coord, "jar"));
    let parts: Vec<&str> = coord.split(':').collect();
    let (group, artifact, version) = (parts.first()?, parts.get(1)?, parts.get(2)?);
    let file = match parts.get(3) {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

impl Library {
    /// Dedup key: group:artifact[:classifier] (version excluded so the child wins).
    fn key(&self) -> String {
        let p: Vec<&str> = self
            .name
            .split('@')
            .next()
            .unwrap_or("")
            .split(':')
            .collect();
        match p.get(3) {
            Some(c) => format!("{}:{}:{}", p[0], p.get(1).unwrap_or(&""), c),
            None => format!("{}:{}", p[0], p.get(1).unwrap_or(&"")),
        }
    }

    pub fn relative_path(&self) -> Option<String> {
        self.downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .and_then(|a| a.path.clone())
            .or_else(|| maven_path(&self.name))
    }

    pub fn local_path(&self, paths: &Paths) -> Option<PathBuf> {
        self.relative_path().map(|p| paths.libraries().join(p))
    }

    /// A download job, or `None` when the library has no download URL.
    pub fn download(&self, paths: &Paths) -> Option<Download> {
        let dest = self.local_path(paths)?;
        if let Some(a) = self.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            if a.url.is_empty() {
                return None;
            }
            return Some(Download {
                url: a.url.clone(),
                dest,
                sha1: a.sha1.clone(),
            });
        }
        let base = self.url.as_deref()?;
        Some(Download {
            url: format!("{}/{}", base.trim_end_matches('/'), self.relative_path()?),
            dest,
            sha1: self.sha1.clone(),
        })
    }
}

impl VersionJson {
    pub fn libraries_for<'a>(&'a self, env: &'a Env) -> impl Iterator<Item = &'a Library> {
        self.libraries
            .iter()
            .filter(move |l| rules::allowed(&l.rules, env))
    }
}

/// Merge a loader profile (`child`, with `inheritsFrom`) onto the vanilla `parent`.
/// Child libraries come first and replace parent libraries with the same group:artifact.
pub fn merge(parent: VersionJson, child: VersionJson) -> VersionJson {
    let child_keys: HashSet<String> = child.libraries.iter().map(Library::key).collect();
    let mut libraries = child.libraries;
    libraries.extend(
        parent
            .libraries
            .into_iter()
            .filter(|l| !child_keys.contains(&l.key())),
    );

    let pa = parent.arguments.unwrap_or_default();
    let ca = child.arguments.unwrap_or_default();
    let arguments = Arguments {
        game: pa.game.into_iter().chain(ca.game).collect(),
        jvm: pa.jvm.into_iter().chain(ca.jvm).collect(),
    };

    VersionJson {
        id: child.id,
        inherits_from: None,
        main_class: child.main_class.or(parent.main_class),
        arguments: Some(arguments),
        libraries,
        asset_index: child.asset_index.or(parent.asset_index),
        downloads: parent.downloads,
        java_version: child.java_version.or(parent.java_version),
        logging: child.logging.or(parent.logging),
        kind: child.kind.or(parent.kind),
    }
}

pub fn load_version(paths: &Paths, id: &str) -> Result<VersionJson> {
    let bytes = std::fs::read(paths.version_json(id))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Install vanilla `version`: version json, client jar, libraries, assets, logging config.
/// `verify_assets`: hash every asset instead of trusting files with the right size (repair).
pub async fn install_vanilla(
    client: &reqwest::Client,
    paths: &Paths,
    version: &str,
    verify_assets: bool,
    progress: &ProgressFn,
) -> Result<VersionJson> {
    let json_path = paths.version_json(version);
    let manifest: VersionManifest = download::fetch_json(client, VERSION_MANIFEST).await?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|v| v.id == version)
        .ok_or_else(|| {
            Error::other(format!(
                "Minecraft {version} not found in Mojang's manifest"
            ))
        })?;

    download::download_one(
        client,
        &Download {
            url: entry.url,
            dest: json_path,
            sha1: Some(entry.sha1),
        },
    )
    .await?;
    let vj = load_version(paths, version)?;

    let mut jobs = Vec::new();
    if let Some(d) = &vj.downloads {
        jobs.push(Download {
            url: d.client.url.clone(),
            dest: paths.version_jar(version),
            sha1: d.client.sha1.clone(),
        });
    }
    if let Some(l) = vj.logging.as_ref().and_then(|l| l.client.as_ref()) {
        jobs.push(Download {
            url: l.file.url.clone(),
            dest: paths.assets().join("log_configs").join(&l.file.id),
            sha1: Some(l.file.sha1.clone()),
        });
    }
    let env = Env::current();
    jobs.extend(vj.libraries_for(&env).filter_map(|l| l.download(paths)));
    download::download_all(client, jobs, "Downloading Minecraft", progress).await?;

    install_assets(client, paths, &vj, verify_assets, progress).await?;
    Ok(vj)
}

async fn install_assets(
    client: &reqwest::Client,
    paths: &Paths,
    vj: &VersionJson,
    verify: bool,
    progress: &ProgressFn,
) -> Result<()> {
    let Some(ai) = &vj.asset_index else {
        return Ok(());
    };
    let index_path = paths
        .assets()
        .join("indexes")
        .join(format!("{}.json", ai.id));
    download::download_one(
        client,
        &Download {
            url: ai.url.clone(),
            dest: index_path.clone(),
            sha1: Some(ai.sha1.clone()),
        },
    )
    .await?;
    let index: AssetIndex = serde_json::from_slice(&tokio::fs::read(&index_path).await?)?;

    // Asset files are named by their own hash, so a file with the right size is
    // trusted without re-hashing ~4000 objects on every launch. New downloads are
    // still verified. Repair (`verify`) hashes everything and deletes bad files.
    let objects = paths.assets().join("objects");
    let mut seen = HashSet::new();
    let unique: Vec<_> = index
        .objects
        .into_values()
        .filter(|o| seen.insert(o.hash.clone()))
        .collect();
    if verify {
        let total = unique.len() as u64;
        for (i, o) in unique.iter().enumerate() {
            if i % 200 == 0 {
                crate::progress::report(progress, "Checking assets", i as u64, total);
            }
            let f = objects.join(&o.hash[..2]).join(&o.hash);
            if f.is_file() && !download::is_valid(&f, Some(&o.hash)).await {
                let _ = std::fs::remove_file(&f);
            }
        }
    }
    let jobs = unique
        .into_iter()
        .filter(|o| {
            std::fs::metadata(objects.join(&o.hash[..2]).join(&o.hash))
                .map(|m| m.len() != o.size)
                .unwrap_or(true)
        })
        .map(|o| {
            let prefix = &o.hash[..2];
            Download {
                url: format!("{RESOURCES}/{prefix}/{}", o.hash),
                dest: paths.assets().join("objects").join(prefix).join(&o.hash),
                sha1: Some(o.hash),
            }
        })
        .collect();
    download::download_all(client, jobs, "Downloading assets", progress).await
}

/// Make sure every library of a (merged) profile is present, downloading what is missing.
pub async fn ensure_libraries(
    client: &reqwest::Client,
    paths: &Paths,
    vj: &VersionJson,
    progress: &ProgressFn,
) -> Result<()> {
    let env = Env::current();
    let mut jobs = Vec::new();
    for lib in vj.libraries_for(&env) {
        match lib.download(paths) {
            Some(d) => jobs.push(d),
            None => {
                let p = lib.local_path(paths);
                if !p.as_ref().is_some_and(|p| p.is_file()) {
                    return Err(Error::other(format!(
                        "library {} is missing and has no download URL; reinstall the loader",
                        lib.name
                    )));
                }
            }
        }
    }
    download::download_all(client, jobs, "Checking libraries", progress).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("org.ow2.asm:asm:9.7").unwrap(),
            "org/ow2/asm/asm/9.7/asm-9.7.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(
            maven_path("net.neoforged:neoforge:21.1.252:client@zip").unwrap(),
            "net/neoforged/neoforge/21.1.252/neoforge-21.1.252-client.zip"
        );
    }

    fn lib(name: &str) -> Library {
        Library {
            name: name.into(),
            downloads: None,
            rules: vec![],
            url: None,
            sha1: None,
        }
    }

    fn version(id: &str, libs: &[&str], game: &[&str]) -> VersionJson {
        VersionJson {
            id: id.into(),
            inherits_from: None,
            main_class: Some(format!("{id}.Main")),
            arguments: Some(Arguments {
                game: game
                    .iter()
                    .map(|s| Argument::Plain(s.to_string()))
                    .collect(),
                jvm: vec![],
            }),
            libraries: libs.iter().map(|n| lib(n)).collect(),
            asset_index: None,
            downloads: None,
            java_version: None,
            logging: None,
            kind: None,
        }
    }

    #[test]
    fn merge_child_overrides_and_appends() {
        let parent = version(
            "1.21.1",
            &["org.ow2.asm:asm:9.6", "com.google:gson:2.10"],
            &["--a"],
        );
        let mut child = version(
            "fabric-loader-0.17.3-1.21.1",
            &["org.ow2.asm:asm:9.7"],
            &["--b"],
        );
        child.inherits_from = Some("1.21.1".into());
        let m = merge(parent, child);
        let names: Vec<_> = m.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["org.ow2.asm:asm:9.7", "com.google:gson:2.10"]);
        assert_eq!(
            m.main_class.as_deref(),
            Some("fabric-loader-0.17.3-1.21.1.Main")
        );
        let game: Vec<_> = m
            .arguments
            .unwrap()
            .game
            .iter()
            .flat_map(|a| a.resolve(&Env::current()))
            .collect();
        assert_eq!(game, ["--a", "--b"]);
    }

    #[test]
    fn natives_classifier_not_deduped_against_main_jar() {
        let parent = version(
            "p",
            &[
                "org.lwjgl:lwjgl:3.3.3",
                "org.lwjgl:lwjgl:3.3.3:natives-linux",
            ],
            &[],
        );
        let child = version("c", &["org.lwjgl:lwjgl:3.3.4"], &[]);
        let m = merge(parent, child);
        assert_eq!(m.libraries.len(), 2);
    }

    #[test]
    fn parses_conditional_arguments() {
        let a: Vec<Argument> = serde_json::from_str(
            r#"["--plain",{"rules":[{"action":"allow","features":{"is_quick_play_multiplayer":true}}],"value":["--quickPlayMultiplayer","${quickPlayMultiplayer}"]}]"#,
        )
        .unwrap();
        let env = Env::current();
        assert_eq!(a.iter().flat_map(|x| x.resolve(&env)).count(), 1);
        let env = env.with_feature("is_quick_play_multiplayer", true);
        assert_eq!(a.iter().flat_map(|x| x.resolve(&env)).count(), 3);
    }
}
