//! Verify the signed-in Minecraft account on the Imaginears Club website.
//!
//! The website opens `imaginears://link/<token>`. After the player confirms, the launcher
//! makes a Mojang session `join` with `serverId = token` (the same call the game makes when
//! joining a server) and tells the website, which asks Mojang `hasJoined` to prove the player
//! owns the account. The Minecraft access token only ever goes to Mojang.
//!
//! Contract: imaginears-web `docs/minecraft-account-linking.md`.

use crate::auth::Session;
use crate::config::WEBSITE_URL;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::OnceLock;
use std::time::Duration;

const SESSION_JOIN: &str = "https://sessionserver.mojang.com/session/minecraft/join";
const TIMEOUT: Duration = Duration::from_secs(20);

/// These requests carry the link token or the Minecraft access token, so they never follow
/// redirects: a redirect to another host would hand it the token.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(format!("MagicLauncher/{}", crate::VERSION))
            .connect_timeout(Duration::from_secs(15))
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build HTTP client")
    })
}

/// What the confirmation prompt shows about the website side of the link.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Challenge {
    /// Display name of the website account the token belongs to.
    pub website_account: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebsiteReply {
    #[serde(default)]
    ok: bool,
    code: Option<String>,
    website_account: Option<String>,
    expires_in_seconds: Option<u64>,
    minecraft_name: Option<String>,
}

/// A token is exactly 40 lowercase hex characters (the shape of a Minecraft `serverId`).
pub fn is_valid_token(token: &str) -> bool {
    token.len() == 40
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_token(token: &str) -> Result<()> {
    if is_valid_token(token) {
        Ok(())
    } else {
        Err(Error::other(
            "This link isn't valid. Start again from the website.",
        ))
    }
}

/// The player-facing message for a failed website reply.
fn failure_message(status: u16, code: Option<&str>) -> String {
    match code {
        Some("invalid_or_expired") => {
            "This link has expired or was already used. Start again from the website.".into()
        }
        Some("not_verified") => {
            "Mojang couldn't confirm your account. Start again from the website.".into()
        }
        Some("already_linked") => {
            "This Minecraft account is already verified on another website account.".into()
        }
        Some("mojang_unavailable") => {
            "Mojang's servers didn't respond. Try again in a minute.".into()
        }
        _ if status == 429 => "Too many attempts. Wait a minute and try again.".into(),
        _ => format!("The website couldn't link your account (error {status})."),
    }
}

fn parse_reply(body: &str) -> WebsiteReply {
    serde_json::from_str(body).unwrap_or_default()
}

/// Turns the website's challenge reply into what the prompt shows.
pub(crate) fn challenge_from_reply(status: u16, body: &str) -> Result<Challenge> {
    let reply = parse_reply(body);
    match reply {
        WebsiteReply {
            ok: true,
            website_account: Some(website_account),
            expires_in_seconds,
            ..
        } if status == 200 => Ok(Challenge {
            website_account,
            expires_in_seconds: expires_in_seconds.unwrap_or(0),
        }),
        reply => Err(Error::other(failure_message(status, reply.code.as_deref()))),
    }
}

/// Turns the website's completion reply into the verified Minecraft name.
pub(crate) fn linked_name_from_reply(status: u16, body: &str) -> Result<String> {
    match parse_reply(body) {
        WebsiteReply {
            ok: true,
            minecraft_name: Some(name),
            ..
        } if status == 200 => Ok(name),
        reply => Err(Error::other(failure_message(status, reply.code.as_deref()))),
    }
}

/// Looks up which website account a link belongs to. Doesn't use the token up.
pub async fn challenge(token: &str) -> Result<Challenge> {
    check_token(token)?;
    let res = client()
        .get(format!("{WEBSITE_URL}/api/minecraft/link/challenge"))
        .query(&[("token", token)])
        .send()
        .await?;
    let status = res.status().as_u16();
    challenge_from_reply(status, &res.text().await?)
}

/// Tells Mojang this account is "joining" `server_id`, so the website's `hasJoined` check
/// can confirm it. Mojang answers 204 on success.
async fn session_join(session: &Session, server_id: &str) -> Result<()> {
    let res = client()
        .post(SESSION_JOIN)
        .json(&json!({
            "accessToken": session.access_token,
            "selectedProfile": session.profile.uuid.replace('-', ""),
            "serverId": server_id,
        }))
        .send()
        .await?;
    match res.status().as_u16() {
        200..=299 => Ok(()),
        401 | 403 => Err(Error::Auth(
            "Mojang didn't accept your session. Sign out and back in to the launcher, then try again."
                .into(),
        )),
        s => Err(Error::other(format!("Mojang's session server returned error {s}. Try again in a minute."))),
    }
}

/// The website may already have linked the account when its reply is lost, and the token is
/// used up by then, so a retry would only report it as expired.
const REPLY_LOST: &str = "Couldn't hear back from the website. Check Profile > Connections on the      website: if your account shows Verified, you're done. Otherwise start again there.";

/// Proves ownership of the signed-in account to the website and links it. Uses the token up.
/// Call only after the player confirmed the prompt.
pub async fn link(session: &Session, token: &str) -> Result<String> {
    check_token(token)?;
    session_join(session, token).await?;
    let res = client()
        .post(format!("{WEBSITE_URL}/api/minecraft/link/complete"))
        .json(&json!({ "token": token, "username": session.profile.name }))
        .send()
        .await
        .map_err(|_| Error::other(REPLY_LOST))?;
    let status = res.status().as_u16();
    let body = res.text().await.map_err(|_| Error::other(REPLY_LOST))?;
    linked_name_from_reply(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn tokens_are_40_lowercase_hex() {
        assert!(is_valid_token(TOKEN));
        assert!(!is_valid_token(&TOKEN.to_uppercase()));
        assert!(!is_valid_token(&TOKEN[..39]));
        assert!(!is_valid_token(&format!("{TOKEN}0")));
        assert!(!is_valid_token(
            "../../api/admin/users/abcdef0123456789abcd"
        ));
        assert!(!is_valid_token(""));
    }

    #[test]
    fn challenge_reply() {
        let ok = challenge_from_reply(
            200,
            r#"{"ok":true,"websiteAccount":"Walt","expiresInSeconds":540}"#,
        )
        .unwrap();
        assert_eq!(
            ok,
            Challenge {
                website_account: "Walt".into(),
                expires_in_seconds: 540
            }
        );

        let expired = challenge_from_reply(410, r#"{"ok":false,"code":"invalid_or_expired"}"#);
        assert!(expired.unwrap_err().to_string().contains("expired"));

        // An HTML error page (proxy, maintenance) must not be read as success.
        assert!(challenge_from_reply(200, "<html>oops</html>").is_err());
        assert!(challenge_from_reply(500, "").is_err());
    }

    #[test]
    fn completion_reply() {
        let name = linked_name_from_reply(200, r#"{"ok":true,"minecraftName":"Notch"}"#).unwrap();
        assert_eq!(name, "Notch");

        let taken = linked_name_from_reply(409, r#"{"ok":false,"code":"already_linked"}"#);
        assert!(taken
            .unwrap_err()
            .to_string()
            .contains("another website account"));

        let limited = linked_name_from_reply(429, r#"{"error":"Too many requests"}"#);
        assert!(limited
            .unwrap_err()
            .to_string()
            .contains("Too many attempts"));

        // "ok" without a name is not a success.
        assert!(linked_name_from_reply(200, r#"{"ok":true}"#).is_err());
    }
}
