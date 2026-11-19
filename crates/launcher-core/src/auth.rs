//! Microsoft → Xbox Live → XSTS → Minecraft sign-in using the OAuth device-code
//! flow (the player enters a short code at microsoft.com/link; no embedded browser).
//!
//! Several accounts are supported. Each account's Microsoft refresh token is stored in the
//! OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service) under its
//! Minecraft UUID. Only public profiles (name, UUID) are written to disk (`accounts.json`).
//! Minecraft access tokens live in memory.

use crate::config::{self, MS_CLIENT_ID};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MS_DEVICE_CODE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_AUTH: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";
const SCOPE: &str = "XboxLive.signin offline_access";
/// 0.1–0.4 stored a single login under this name; it is moved to a per-account entry on use.
const KEYRING_USER_LEGACY: &str = "microsoft-refresh-token";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: String,
}

/// Public profile, safe to show and to cache on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub uuid: String,
}

/// A usable game session.
#[derive(Debug, Clone)]
pub struct Session {
    pub profile: Profile,
    pub access_token: String,
    pub xuid: Option<String>,
    pub expires_at: u64,
    pub user_type: &'static str,
}

impl Session {
    pub fn is_expired(&self) -> bool {
        now() + 300 >= self.expires_at
    }
}

#[derive(Debug, Deserialize)]
struct MsTokens {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub async fn start_device_code(client: &reqwest::Client) -> Result<DeviceCode> {
    if MS_CLIENT_ID.is_empty() {
        return Err(Error::Auth(
            "Microsoft sign-in is not set up in this build yet (no Azure client ID)".into(),
        ));
    }
    let resp = client
        .post(MS_DEVICE_CODE)
        .form(&[("client_id", MS_CLIENT_ID), ("scope", SCOPE)])
        .send()
        .await?;
    if !resp.status().is_success() {
        let e: OAuthError = resp.json().await?;
        return Err(Error::Auth(format!("{}: {}", e.error, e.error_description)));
    }
    Ok(resp.json().await?)
}

/// Poll until the player finishes signing in, then complete the Minecraft login.
pub async fn finish_device_code(client: &reqwest::Client, dc: &DeviceCode) -> Result<Session> {
    let deadline = now() + dc.expires_in;
    let mut interval = dc.interval.max(1);
    let tokens = loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if now() > deadline {
            return Err(Error::Auth(
                "the sign-in code expired, please try again".into(),
            ));
        }
        let resp = client
            .post(MS_TOKEN)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", MS_CLIENT_ID),
                ("device_code", dc.device_code.as_str()),
            ])
            .send()
            .await?;
        if resp.status().is_success() {
            break resp.json::<MsTokens>().await?;
        }
        let e: OAuthError = resp.json().await?;
        match e.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => interval += 5,
            "authorization_declined" => return Err(Error::Auth("sign-in was cancelled".into())),
            "expired_token" => {
                return Err(Error::Auth(
                    "the sign-in code expired, please try again".into(),
                ))
            }
            _ => return Err(Error::Auth(format!("{}: {}", e.error, e.error_description))),
        }
    };
    let session = minecraft_login(client, &tokens.access_token).await?;
    store_refresh_token(Some(&session.profile.uuid), &tokens.refresh_token)?;
    Ok(session)
}

/// Silent sign-in for one account (`uuid`), or the pre-0.5 single login (`None`).
pub async fn refresh(client: &reqwest::Client, uuid: Option<&str>) -> Result<Session> {
    let refresh_token = load_refresh_token(uuid)?.ok_or(Error::NotSignedIn)?;
    let resp = client
        .post(MS_TOKEN)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", MS_CLIENT_ID),
            ("refresh_token", refresh_token.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        // Revoked or expired (≈90 days unused): require a fresh sign-in.
        let _ = delete_refresh_token(uuid);
        return Err(Error::NotSignedIn);
    }
    let tokens: MsTokens = resp.json().await?;
    let session = minecraft_login(client, &tokens.access_token).await?;
    store_refresh_token(Some(&session.profile.uuid), &tokens.refresh_token)?;
    if uuid.is_none() {
        // Migrated to the per-account entry.
        let _ = delete_refresh_token(None);
    }
    Ok(session)
}

async fn minecraft_login(client: &reqwest::Client, ms_access_token: &str) -> Result<Session> {
    // 1. Xbox Live user token
    let xbl: serde_json::Value = client
        .post(XBL_AUTH)
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let xbl_token = xbl["Token"]
        .as_str()
        .ok_or_else(|| Error::Auth("Xbox Live returned no token".into()))?;

    // 2. XSTS token for Minecraft services
    let resp = client
        .post(XSTS_AUTH)
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let v: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(Error::Auth(xsts_error(v["XErr"].as_u64().unwrap_or(0))));
    }
    let xsts: serde_json::Value = resp.error_for_status()?.json().await?;
    let xsts_token = xsts["Token"]
        .as_str()
        .ok_or_else(|| Error::Auth("XSTS returned no token".into()))?;
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| Error::Auth("XSTS returned no user hash".into()))?;

    // 3. Minecraft access token
    let mc: serde_json::Value = client
        .post(MC_LOGIN)
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await?
        .error_for_status()
        .map_err(|e| {
            Error::Auth(format!(
                "Minecraft services rejected the login ({e}). Is the Azure app approved by Mojang?"
            ))
        })?
        .json()
        .await?;
    let access_token = mc["access_token"]
        .as_str()
        .ok_or_else(|| Error::Auth("Minecraft returned no access token".into()))?
        .to_string();
    let expires_in = mc["expires_in"].as_u64().unwrap_or(86_400);

    // 4. Profile (404 = account does not own Java Edition)
    let resp = client
        .get(MC_PROFILE)
        .bearer_auth(&access_token)
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::Auth(
            "this Microsoft account does not own Minecraft: Java Edition".into(),
        ));
    }
    let p: serde_json::Value = resp.error_for_status()?.json().await?;
    let profile = Profile {
        name: p["name"].as_str().unwrap_or_default().to_string(),
        uuid: p["id"].as_str().unwrap_or_default().to_string(),
    };

    Ok(Session {
        profile,
        access_token,
        xuid: xsts["DisplayClaims"]["xui"][0]["xid"]
            .as_str()
            .map(str::to_string),
        expires_at: now() + expires_in,
        user_type: "msa",
    })
}

fn xsts_error(code: u64) -> String {
    match code {
        2148916233 => {
            "this Microsoft account has no Xbox profile yet; sign in once at xbox.com".into()
        }
        2148916235 => "Xbox Live is not available in your country".into(),
        2148916236 | 2148916237 => "this account needs adult verification on xbox.com".into(),
        2148916238 => {
            "this is a child account; it must be added to a Microsoft family by an adult".into()
        }
        other => format!("Xbox Live sign-in failed (XErr {other})"),
    }
}

fn keyring_entry(uuid: Option<&str>) -> Result<keyring::Entry> {
    let user = match uuid {
        Some(u) => format!("account:{}", crate::allowlist::normalize_uuid(u)),
        None => KEYRING_USER_LEGACY.to_string(),
    };
    keyring::Entry::new(config::APP_IDENTIFIER, &user).map_err(|e| Error::Auth(e.to_string()))
}

fn store_refresh_token(uuid: Option<&str>, token: &str) -> Result<()> {
    keyring_entry(uuid)?.set_password(token).map_err(|e| {
        Error::Auth(format!(
            "could not save sign-in to the system keychain: {e}"
        ))
    })
}

fn load_refresh_token(uuid: Option<&str>) -> Result<Option<String>> {
    match keyring_entry(uuid)?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Auth(e.to_string())),
    }
}

pub fn delete_refresh_token(uuid: Option<&str>) -> Result<()> {
    match keyring_entry(uuid)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Auth(e.to_string())),
    }
}

/// Whether a pre-0.5 single login is still waiting to be migrated.
pub fn has_legacy_login() -> bool {
    matches!(load_refresh_token(None), Ok(Some(_)))
}

/// The signed-in accounts (public profiles only) and which one is active.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Accounts {
    pub active: Option<String>,
    pub list: Vec<Profile>,
}

impl Accounts {
    pub fn load(path: &std::path::Path) -> Accounts {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// Add or update an account (e.g. a renamed player) and make it active.
    pub fn upsert_active(&mut self, p: Profile) {
        let id = crate::allowlist::normalize_uuid(&p.uuid);
        self.list
            .retain(|a| crate::allowlist::normalize_uuid(&a.uuid) != id);
        self.active = Some(p.uuid.clone());
        self.list.push(p);
    }

    /// Remove an account; the next remaining one becomes active.
    pub fn remove(&mut self, uuid: &str) {
        let id = crate::allowlist::normalize_uuid(uuid);
        self.list
            .retain(|a| crate::allowlist::normalize_uuid(&a.uuid) != id);
        if self
            .active
            .as_deref()
            .is_some_and(|a| crate::allowlist::normalize_uuid(a) == id)
        {
            self.active = self.list.first().map(|a| a.uuid.clone());
        }
    }
}

#[cfg(test)]
mod account_tests {
    use super::*;

    fn p(name: &str, uuid: &str) -> Profile {
        Profile {
            name: name.into(),
            uuid: uuid.into(),
        }
    }

    #[test]
    fn upsert_switch_and_remove() {
        let mut a = Accounts::default();
        a.upsert_active(p("Clarkcj", "aaa"));
        a.upsert_active(p("Alt", "bbb"));
        assert_eq!(a.active.as_deref(), Some("bbb"));
        a.upsert_active(p("ClarkcjRenamed", "AAA"));
        assert_eq!(a.list.len(), 2);
        assert_eq!(a.active.as_deref(), Some("AAA"));
        a.remove("aaa");
        assert_eq!(a.list.len(), 1);
        assert_eq!(a.active.as_deref(), Some("bbb"));
        a.remove("bbb");
        assert_eq!(a.active, None);
    }
}
