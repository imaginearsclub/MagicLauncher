//! The signed-in player's skin, straight from Mojang (no third-party avatar sites).
//! The UI draws the head and hat layer from the skin image.
//!
//! The skin is cached per account in `cache/skins/` and Mojang is asked again at most every
//! [`RECHECK_SECS`]: its session server rate-limits repeated lookups, which used to make the
//! head fall back to a letter after a few home-screen refreshes.

use crate::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How long a cached skin is used before asking Mojang whether it changed.
pub const RECHECK_SECS: u64 = 6 * 60 * 60;
/// Skins are 64×64 PNGs of a few KB; refuse anything absurd.
const MAX_SKIN_BYTES: usize = 512 * 1024;

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheMeta {
    /// Texture URL the cached PNG came from (`None` = player uses a default skin).
    texture_url: Option<String>,
    /// Unix seconds of the last successful Mojang lookup.
    checked_at: u64,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_fresh(checked_at: u64, now: u64) -> bool {
    now.saturating_sub(checked_at) < RECHECK_SECS
}

fn data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// The player's skin as a `data:` URL, from the local cache when it's recent, otherwise from
/// Mojang. If Mojang can't be reached or rate-limits us, the cached skin is used anyway.
/// `None` = default skin (or nothing cached and Mojang unavailable).
pub async fn cached_skin(client: &reqwest::Client, cache_dir: &Path, uuid: &str) -> Option<String> {
    let uuid = crate::allowlist::normalize_uuid(uuid);
    let dir: PathBuf = cache_dir.join("skins");
    let png_path = dir.join(format!("{uuid}.png"));
    let meta_path = dir.join(format!("{uuid}.json"));
    let meta: Option<CacheMeta> = std::fs::read(&meta_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let cached_png = std::fs::read(&png_path).ok().filter(|b| is_png(b));
    let cached = || cached_png.as_deref().map(data_url);

    if let Some(m) = &meta {
        if is_fresh(m.checked_at, now_secs()) {
            return match m.texture_url {
                Some(_) => cached(),
                None => None,
            };
        }
    }

    let Ok(texture_url) = skin_url(client, &uuid).await else {
        return cached();
    };
    let _ = std::fs::create_dir_all(&dir);
    let save_meta = |texture_url: Option<String>| {
        let m = CacheMeta {
            texture_url,
            checked_at: now_secs(),
        };
        if let Ok(b) = serde_json::to_vec(&m) {
            let _ = write_atomic(&meta_path, &b);
        }
    };
    let Some(url) = texture_url else {
        let _ = std::fs::remove_file(&png_path);
        save_meta(None);
        return None;
    };
    let unchanged = meta.as_ref().and_then(|m| m.texture_url.as_deref()) == Some(url.as_str());
    if unchanged {
        if let Some(png) = &cached_png {
            save_meta(Some(url));
            return Some(data_url(png));
        }
    }
    match download_png(client, &url).await {
        Some(png) => {
            let _ = write_atomic(&png_path, &png);
            save_meta(Some(url));
            Some(data_url(&png))
        }
        None => cached(),
    }
}

async fn download_png(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().await.ok()?.error_for_status().ok()?;
    let bytes = resp.bytes().await.ok()?;
    (bytes.len() <= MAX_SKIN_BYTES && is_png(&bytes)).then(|| bytes.to_vec())
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct Textures {
    textures: TextureSet,
}

#[derive(Deserialize)]
struct TextureSet {
    #[serde(rename = "SKIN")]
    skin: Option<Texture>,
}

#[derive(Deserialize)]
struct Texture {
    url: String,
}

/// Skin URL from a session-server profile response (`None` = default skin).
/// Mojang returns `http://` texture URLs; the HTTPS form works and is what the UI loads.
pub fn skin_url_from_profile(json: &[u8]) -> Option<String> {
    let profile: Profile = serde_json::from_slice(json).ok()?;
    let value = profile
        .properties
        .iter()
        .find(|p| p.name == "textures")?
        .value
        .clone();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .ok()?;
    let t: Textures = serde_json::from_slice(&decoded).ok()?;
    let url = t.textures.skin?.url;
    Some(url.replacen("http://", "https://", 1))
}

pub async fn skin_url(client: &reqwest::Client, uuid: &str) -> Result<Option<String>> {
    let uuid = crate::allowlist::normalize_uuid(uuid);
    let bytes = client
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"
        ))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(skin_url_from_profile(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_window() {
        assert!(is_fresh(1000, 1000));
        assert!(is_fresh(1000, 1000 + RECHECK_SECS - 1));
        assert!(!is_fresh(1000, 1000 + RECHECK_SECS));
        // A clock that went backwards still counts as fresh rather than panicking.
        assert!(is_fresh(5000, 1000));
    }

    #[test]
    fn png_check_and_data_url() {
        let png = b"\x89PNG\r\n\x1a\nrest";
        assert!(is_png(png));
        assert!(!is_png(b"<html>"));
        assert!(data_url(png).starts_with("data:image/png;base64,iVBORw0KGg"));
    }

    #[test]
    fn extracts_https_skin_url() {
        let textures =
            r#"{"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc"}}}"#;
        let value = base64::engine::general_purpose::STANDARD.encode(textures);
        let profile = format!(
            r#"{{"id":"x","name":"Clarkcj","properties":[{{"name":"textures","value":"{value}"}}]}}"#
        );
        assert_eq!(
            skin_url_from_profile(profile.as_bytes()).as_deref(),
            Some("https://textures.minecraft.net/texture/abc")
        );
    }

    #[test]
    fn no_skin_means_default() {
        let textures = base64::engine::general_purpose::STANDARD.encode(r#"{"textures":{}}"#);
        let profile = format!(r#"{{"properties":[{{"name":"textures","value":"{textures}"}}]}}"#);
        assert_eq!(skin_url_from_profile(profile.as_bytes()), None);
    }
}
