//! Alpha allowlist: a signed list of Minecraft accounts allowed on the Alpha channel.
//!
//! Published at `launcher/channels/alpha.json` (+ `.sig`) by `publish-allowlist.yml` from
//! `channels/alpha-testers.txt`. The format is fixed so a Discord bot or the staff web panel
//! can produce it later without launcher changes:
//!
//! ```json
//! {"format":1,"channel":"alpha","issued_at":1795000000,
//!  "members":[{"uuid":"069a79f4…","name":"Notch","added_by":"clark","added_at":"2026-11-17",
//!              "expires_at":null}]}
//! ```
//!
//! Signed with its own minisign key (not the update key); the `.sig` is what
//! `tauri signer sign` writes. The raw downloaded bytes are verified before parsing.
//! The last verified list is cached so being offline doesn't hide Alpha, and a list
//! older than the cached one (`issued_at`) is refused, so a removal can't be undone by
//! replaying an old file.

use crate::channel::LAUNCHER_FEED_BASE;
use crate::paths::Paths;
use crate::{Error, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Public half of `%USERPROFILE%\.tauri\magiclauncher-alpha-allowlist.key`
/// (GitHub secret ALLOWLIST_SIGNING_KEY).
pub const ALLOWLIST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDEwNzQ2QjRBQzEyRjNFNjcKUldSblBpL0JTbXQwRU1GVkIxOFpOaFlYZVNEdmR2d01WV3JTUXE1OEUzN2VtTDhKVFNHZEZsSkgK";

pub const FORMAT: u32 = 1;

pub fn url() -> String {
    format!("{LAUNCHER_FEED_BASE}channels/alpha.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allowlist {
    pub format: u32,
    pub channel: String,
    /// Unix seconds when the list was generated.
    pub issued_at: u64,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub added_by: Option<String>,
    #[serde(default)]
    pub added_at: Option<String>,
    /// Unix seconds; `None` = no expiry.
    #[serde(default)]
    pub expires_at: Option<u64>,
}

/// UUIDs compared as 32 lowercase hex digits (Mojang returns them without dashes,
/// people often write them with).
pub fn normalize_uuid(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

impl Allowlist {
    pub fn allows(&self, uuid: &str, now: u64) -> bool {
        let want = normalize_uuid(uuid);
        self.members
            .iter()
            .any(|m| normalize_uuid(&m.uuid) == want && m.expires_at.is_none_or(|exp| now < exp))
    }
}

fn b64(s: &str) -> Result<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| Error::other(format!("allowlist: bad base64: {e}")))?;
    String::from_utf8(bytes).map_err(|e| Error::other(format!("allowlist: {e}")))
}

/// Verify `bytes` against a `tauri signer` signature, then parse and sanity-check it.
pub fn verify(bytes: &[u8], sig_b64: &str, pubkey_b64: &str) -> Result<Allowlist> {
    let pk = minisign_verify::PublicKey::decode(&b64(pubkey_b64)?)
        .map_err(|e| Error::other(format!("allowlist public key: {e}")))?;
    let sig = minisign_verify::Signature::decode(&b64(sig_b64)?)
        .map_err(|e| Error::other(format!("allowlist signature: {e}")))?;
    pk.verify(bytes, &sig, true)
        .map_err(|e| Error::other(format!("allowlist signature check failed: {e}")))?;
    let list: Allowlist = serde_json::from_slice(bytes)?;
    if list.format != FORMAT || list.channel != "alpha" {
        return Err(Error::other(format!(
            "allowlist: unexpected format {} / channel {:?}",
            list.format, list.channel
        )));
    }
    Ok(list)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_paths(paths: &Paths) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = paths.root.join("channels");
    (dir.join("alpha.json"), dir.join("alpha.json.sig"))
}

fn load_cached(paths: &Paths) -> Option<Allowlist> {
    let (json, sig) = cache_paths(paths);
    let bytes = std::fs::read(json).ok()?;
    let sig = std::fs::read_to_string(sig).ok()?;
    verify(&bytes, &sig, ALLOWLIST_PUBKEY).ok()
}

/// `Ok(None)` when no list is published (403/404). Other failures are errors.
async fn fetch(client: &reqwest::Client) -> Result<Option<(Vec<u8>, String)>> {
    let url = url();
    let get = |u: String| async move { client.get(u).send().await };
    let resp = get(url.clone()).await?;
    if matches!(resp.status().as_u16(), 403 | 404) {
        return Ok(None);
    }
    let bytes = resp.error_for_status()?.bytes().await?.to_vec();
    let sig = get(format!("{url}.sig"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(Some((bytes, sig)))
}

/// The current allowlist: a fresh verified download when possible (cached for next
/// time), otherwise the last verified one. `None` if neither exists.
pub async fn current(client: &reqwest::Client, paths: &Paths) -> Option<Allowlist> {
    let cached = load_cached(paths);
    let fresh = match fetch(client).await {
        Ok(Some((bytes, sig))) => match verify(&bytes, &sig, ALLOWLIST_PUBKEY) {
            Ok(list) => Some((list, bytes, sig)),
            Err(e) => {
                eprintln!("ignoring alpha allowlist: {e}");
                None
            }
        },
        Ok(None) => None,
        Err(e) => {
            eprintln!("alpha allowlist download failed, using cached copy: {e}");
            None
        }
    };
    match (fresh, cached) {
        // Never step back to an older list than one already seen.
        (Some((list, _, _)), Some(c)) if list.issued_at < c.issued_at => Some(c),
        (Some((list, bytes, sig)), _) => {
            let (json, sigp) = cache_paths(paths);
            if let Some(dir) = json.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(json, &bytes);
            let _ = std::fs::write(sigp, sig);
            Some(list)
        }
        (None, c) => c,
    }
}

/// Whether `uuid` (the signed-in account) may use the Alpha channel right now.
pub async fn alpha_allowed(client: &reqwest::Client, paths: &Paths, uuid: &str) -> bool {
    current(client, paths)
        .await
        .is_some_and(|l| l.allows(uuid, now()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Signed with a throwaway key via `tauri signer sign` (the tool CI uses).
    const TEST_PUB: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc1RTI5NDU4MjUzNUI4OEYKUldTUHVEVWxXSlRpZFVLT3lvWnNSdkJQSFBNLzZ5TTZublpQWklVTFNiR0xKejBTUHQ2U1pNOUYK";
    const ALPHA_JSON: &str = r#"{"format":1,"channel":"alpha","issued_at":1795000000,"members":[{"uuid":"069a79f444e94726a5befca90e38aaf5","name":"Notch","added_by":"test","added_at":"2026-11-17","expires_at":null}]}"#;
    const ALPHA_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTUHVEVWxXSlRpZFdWb2IzZjZGQnQ2WFA3NXQ3QVVZZHM3c3JyaUNPYzd6UXhHQXFJT2xJQTV5czl6WXIvUVpGOW1id3lYUWd1TE1DRnJZVUVnbFZIY3BDMkNlaEZqYlFzPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0OTQyODkxCWZpbGU6YWxwaGEuanNvbgpXZkxTZGZreFp2SUh0Sis4K3hMc0lYRjBYTE1OVTJKTTVOTlAvdHJWM1FIY1NBTERqZEh3K0p6R1JWQWdXR1NpMDg3ckhyTFJkNVJrcDBHOEUyUk5EUT09Cg==";
    const BETA_JSON: &str = r#"{"format":1,"channel":"beta","issued_at":1795000000,"members":[]}"#;
    const BETA_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTUHVEVWxXSlRpZGFMZkpZajF3Z1RtRXRWUCtmMkE2SVdmbXM0NWdqbmxrUWpvUlF1cm9WMHFsVXhXZHRITHJIZ1JubHU1Q3FncG1oYUxHQlRubVZHdWJSRGo5VWpzcndzPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0OTQyOTA1CWZpbGU6YmV0YS5qc29uCnc0MmtuK0pSL1pxcHhvcndPMjhHbndzbVJySkJqbVVBcnJqTEE2Z2FtT1hGaW8weDR5RXhpL1dmQ1l4aGsyTFF3Wi91YmUyc3RwbFN0MldZVnVpT0RRPT0K";

    #[test]
    fn verifies_real_signature_and_matches_uuid() {
        let list = verify(ALPHA_JSON.as_bytes(), ALPHA_SIG, TEST_PUB).unwrap();
        assert_eq!(list.members.len(), 1);
        assert!(list.allows("069a79f4-44e9-4726-a5be-fca90e38aaf5", 0));
        assert!(list.allows("069A79F444E94726A5BEFCA90E38AAF5", 0));
        assert!(!list.allows("00000000000000000000000000000000", 0));
    }

    #[test]
    fn rejects_tampered_payload() {
        let tampered = ALPHA_JSON.replace("Notch", "Notch2");
        assert!(verify(tampered.as_bytes(), ALPHA_SIG, TEST_PUB).is_err());
    }

    #[test]
    fn rejects_other_key() {
        assert!(verify(ALPHA_JSON.as_bytes(), ALPHA_SIG, ALLOWLIST_PUBKEY).is_err());
    }

    #[test]
    fn rejects_validly_signed_list_for_another_channel() {
        assert!(verify(BETA_JSON.as_bytes(), BETA_SIG, TEST_PUB).is_err());
    }

    #[test]
    fn expiry_is_honoured() {
        let mut list = verify(ALPHA_JSON.as_bytes(), ALPHA_SIG, TEST_PUB).unwrap();
        list.members[0].expires_at = Some(100);
        assert!(list.allows("069a79f444e94726a5befca90e38aaf5", 99));
        assert!(!list.allows("069a79f444e94726a5befca90e38aaf5", 100));
    }

    /// End-to-end check against the production key:
    /// `ALLOWLIST_FILE=path/alpha.json cargo test -p launcher-core -- --ignored allowlist_file`
    /// (expects `path/alpha.json.sig` next to it, signed like publish-allowlist.yml does).
    #[test]
    #[ignore]
    fn allowlist_file_verifies_with_production_key() {
        let path = std::env::var("ALLOWLIST_FILE").expect("set ALLOWLIST_FILE");
        let bytes = std::fs::read(&path).unwrap();
        let sig = std::fs::read_to_string(format!("{path}.sig")).unwrap();
        let list = verify(&bytes, &sig, ALLOWLIST_PUBKEY).unwrap();
        println!("verified {} member(s)", list.members.len());
    }

    #[test]
    fn real_public_key_decodes() {
        let pk = b64(ALLOWLIST_PUBKEY).unwrap();
        assert!(minisign_verify::PublicKey::decode(&pk).is_ok());
    }
}
