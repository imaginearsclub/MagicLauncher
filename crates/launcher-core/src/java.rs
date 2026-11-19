//! Downloads Mojang's own Java runtime for the component a version asks for
//! (1.21.1 → `java-runtime-delta`, Java 21), so players never install Java.

use crate::download::{self, Download};
use crate::paths::Paths;
use crate::progress::ProgressFn;
use crate::{Error, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const RUNTIME_INDEX: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
}

#[derive(Debug, Deserialize)]
struct ManifestRef {
    url: String,
    sha1: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    File {
        #[serde(default)]
        executable: bool,
        downloads: FileDownloads,
    },
    Directory,
    Link {
        #[cfg_attr(not(unix), allow(dead_code))]
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct FileDownloads {
    raw: RawFile,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    url: String,
    sha1: String,
}

/// Mojang's platform key for the running OS/arch.
fn platform_key() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", "x86") => "windows-x86",
        ("macos", "aarch64") => "mac-os-arm64",
        ("macos", _) => "mac-os",
        ("linux", "x86_64") => "linux",
        ("linux", "x86") => "linux-i386",
        (os, arch) => {
            return Err(Error::other(format!(
                "no Mojang Java runtime for {os}/{arch}; set a custom Java path"
            )))
        }
    })
}

/// Path of the java binary inside an extracted runtime.
/// `windowed` picks `javaw.exe` on Windows so no console window appears.
pub fn java_binary(runtime_dir: &Path, windowed: bool) -> PathBuf {
    if cfg!(target_os = "windows") {
        runtime_dir
            .join("bin")
            .join(if windowed { "javaw.exe" } else { "java.exe" })
    } else if cfg!(target_os = "macos") {
        runtime_dir.join("jre.bundle/Contents/Home/bin/java")
    } else {
        runtime_dir.join("bin/java")
    }
}

/// Ensure the runtime `component` is installed; returns its directory.
pub async fn ensure_runtime(
    client: &reqwest::Client,
    paths: &Paths,
    component: &str,
    progress: &ProgressFn,
) -> Result<PathBuf> {
    let dir = paths.runtimes().join(component);
    let marker = dir.join(".magiclauncher-manifest-sha1");

    let index: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> =
        download::fetch_json(client, RUNTIME_INDEX).await?;
    let entry = index
        .get(platform_key()?)
        .and_then(|p| p.get(component))
        .and_then(|v| v.first())
        .ok_or_else(|| {
            Error::other(format!(
                "Java runtime {component} unavailable for this platform"
            ))
        })?;

    // Skip re-hashing hundreds of files when the installed manifest is current.
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(entry.manifest.sha1.as_str())
        && java_binary(&dir, false).is_file()
    {
        return Ok(dir);
    }

    let bytes = download::fetch_bytes(client, &entry.manifest.url).await?;
    if download::sha1_hex(&bytes) != entry.manifest.sha1 {
        return Err(Error::other("Java runtime manifest failed verification"));
    }
    let manifest: RuntimeManifest = serde_json::from_slice(&bytes)?;

    let mut jobs = Vec::new();
    let mut executables = Vec::new();
    #[cfg(unix)]
    let mut links: Vec<(PathBuf, String)> = Vec::new();
    for (rel, f) in &manifest.files {
        let dest = crate::paths::safe_join(&dir, rel)?;
        match f {
            RuntimeFile::Directory => std::fs::create_dir_all(&dest)?,
            RuntimeFile::File {
                executable,
                downloads,
            } => {
                if *executable {
                    executables.push(dest.clone());
                }
                jobs.push(Download {
                    url: downloads.raw.url.clone(),
                    dest,
                    sha1: Some(downloads.raw.sha1.clone()),
                });
            }
            #[cfg(unix)]
            RuntimeFile::Link { target } => links.push((dest, target.clone())),
            #[cfg(not(unix))]
            RuntimeFile::Link { .. } => {} // Windows runtimes do not use links.
        }
    }
    download::download_all(client, jobs, "Downloading Java", progress).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in &executables {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))?;
        }
        for (dest, target) in links {
            let _ = std::fs::remove_file(&dest);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::os::unix::fs::symlink(target, &dest)?;
        }
    }
    #[cfg(not(unix))]
    let _ = executables;

    std::fs::write(&marker, &entry.manifest.sha1)?;
    Ok(dir)
}
