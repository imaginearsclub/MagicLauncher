//! Fabric install. Unlike Forge-style loaders, Fabric needs no installer run: its meta
//! service serves a ready launcher profile (a version JSON with `inheritsFrom` the vanilla
//! version, Fabric's main class and its libraries as Maven coordinates + repository URL).
//! We save that profile under `versions/<id>/<id>.json` like the vanilla launcher does and
//! merge it onto the vanilla version (`mojang::merge`).

use crate::download;
use crate::mojang::{self, VersionJson};
use crate::paths::Paths;
use crate::progress::{report, ProgressFn};
use crate::{Error, Result};

const META: &str = "https://meta.fabricmc.net/v2/versions/loader";

/// The profile id Fabric meta uses, e.g. `fabric-loader-0.17.3-1.21.11`.
pub fn version_id(minecraft: &str, loader: &str) -> String {
    format!("fabric-loader-{loader}-{minecraft}")
}

pub fn profile_url(minecraft: &str, loader: &str) -> String {
    format!("{META}/{minecraft}/{loader}/profile/json")
}

/// Make sure the Fabric `loader` profile for `minecraft` is present and return it.
/// Its libraries are downloaded later by `mojang::ensure_libraries` with the vanilla ones.
pub async fn ensure_installed(
    client: &reqwest::Client,
    paths: &Paths,
    minecraft: &str,
    loader: &str,
    progress: &ProgressFn,
) -> Result<VersionJson> {
    let id = version_id(minecraft, loader);
    if paths.version_json(&id).is_file() {
        if let Ok(v) = mojang::load_version(paths, &id) {
            return Ok(v);
        }
        // Unreadable (e.g. a broken earlier download): fetch it again below.
    }

    report(progress, "Installing Fabric", 0, 1);
    let bytes = download::fetch_bytes(client, &profile_url(minecraft, loader)).await?;
    let profile = parse_profile(&bytes, minecraft, loader)?;
    let dest = paths.version_json(&id);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&dest, &bytes)?;
    report(progress, "Installing Fabric", 1, 1);
    Ok(profile)
}

/// Parse a Fabric meta profile and check it is the one asked for.
pub fn parse_profile(bytes: &[u8], minecraft: &str, loader: &str) -> Result<VersionJson> {
    let v: VersionJson = serde_json::from_slice(bytes)
        .map_err(|e| Error::other(format!("Fabric profile for {minecraft}/{loader}: {e}")))?;
    let id = version_id(minecraft, loader);
    if v.id != id || v.inherits_from.as_deref() != Some(minecraft) {
        return Err(Error::other(format!(
            "Fabric meta returned profile {} (inherits {:?}), expected {id} for {minecraft}",
            v.id, v.inherits_from
        )));
    }
    if v.main_class.is_none() {
        return Err(Error::other(format!(
            "Fabric profile {id} has no main class"
        )));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape of https://meta.fabricmc.net/v2/versions/loader/<mc>/<loader>/profile/json
    const PROFILE: &str = r#"{
      "id": "fabric-loader-0.17.3-1.21.11",
      "inheritsFrom": "1.21.11",
      "releaseTime": "2025-10-01T00:00:00+0000",
      "time": "2025-10-01T00:00:00+0000",
      "type": "release",
      "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
      "arguments": { "game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] },
      "libraries": [
        { "name": "org.ow2.asm:asm:9.9", "url": "https://maven.fabricmc.net/",
          "sha1": "0123456789abcdef0123456789abcdef01234567", "size": 126000 },
        { "name": "net.fabricmc:intermediary:1.21.11", "url": "https://maven.fabricmc.net/" },
        { "name": "net.fabricmc:fabric-loader:0.17.3", "url": "https://maven.fabricmc.net/" }
      ]
    }"#;

    #[test]
    fn parses_meta_profile_and_builds_maven_downloads() {
        let v = parse_profile(PROFILE.as_bytes(), "1.21.11", "0.17.3").unwrap();
        assert_eq!(
            v.main_class.as_deref(),
            Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
        );
        let paths = Paths::new(std::path::PathBuf::from("root"));
        let d = v.libraries[0].download(&paths).unwrap();
        assert_eq!(
            d.url,
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.9/asm-9.9.jar"
        );
        assert_eq!(
            d.sha1.as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
        let loader = v.libraries[2].download(&paths).unwrap();
        assert_eq!(
            loader.url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.17.3/fabric-loader-0.17.3.jar"
        );
        assert_eq!(loader.sha1, None);
    }

    #[test]
    fn rejects_a_profile_for_another_version() {
        let e = parse_profile(PROFILE.as_bytes(), "1.21.12", "0.17.3")
            .unwrap_err()
            .to_string();
        assert!(e.contains("expected fabric-loader-0.17.3-1.21.12"), "{e}");
    }

    #[test]
    fn merged_profile_uses_fabric_main_class_and_keeps_vanilla_libraries() {
        let fabric = parse_profile(PROFILE.as_bytes(), "1.21.11", "0.17.3").unwrap();
        let vanilla: VersionJson = serde_json::from_str(
            r#"{"id":"1.21.11","mainClass":"net.minecraft.client.main.Main",
                "arguments":{"game":["--username","${auth_player_name}"],"jvm":["-cp","${classpath}"]},
                "libraries":[{"name":"org.ow2.asm:asm:9.6"},{"name":"com.mojang:brigadier:1.3.10"}]}"#,
        )
        .unwrap();
        let m = mojang::merge(vanilla, fabric);
        assert_eq!(m.id, "fabric-loader-0.17.3-1.21.11");
        assert_eq!(
            m.main_class.as_deref(),
            Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
        );
        let names: Vec<_> = m.libraries.iter().map(|l| l.name.as_str()).collect();
        // Fabric's newer asm replaces vanilla's; vanilla-only libraries stay.
        assert!(names.contains(&"org.ow2.asm:asm:9.9"));
        assert!(!names.contains(&"org.ow2.asm:asm:9.6"));
        assert!(names.contains(&"com.mojang:brigadier:1.3.10"));
    }

    /// Network: the real Fabric meta profile parses and every library has a download.
    /// `cargo test -p launcher-core live_fabric_profile -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_fabric_profile() {
        let (mc, loader) = ("1.21.11", "0.20.1");
        let bytes = download::fetch_bytes(&crate::http_client(), &profile_url(mc, loader))
            .await
            .unwrap();
        let v = parse_profile(&bytes, mc, loader).unwrap();
        let paths = Paths::new(std::path::PathBuf::from("root"));
        for lib in &v.libraries {
            let d = lib.download(&paths).expect("library has a download");
            println!("{} -> {} sha1={:?}", lib.name, d.url, d.sha1);
        }
        assert!(!v.libraries.is_empty());
    }
}
