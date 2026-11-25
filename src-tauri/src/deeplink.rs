//! `imaginears://` links, so a button on the website can open the launcher.
//!
//! Any web page can send these links, so they only ever select from a fixed set of actions
//! and never start the game or act on the player's account by themselves:
//! `imaginears://play` opens the Play screen (the player still clicks Play), and
//! `imaginears://link/<token>` opens a prompt asking whether to link the signed-in account to
//! the website (nothing happens until the player confirms). Everything else in the URL (query,
//! extra path) is ignored. An unknown action, or a `link` with a malformed token, just opens
//! the launcher.

use launcher_core::website_link::is_valid_token;
use serde::Serialize;
use tauri::Url;

/// Registered in `tauri.conf.json` (`plugins.deep-link.desktop.schemes`).
pub const SCHEME: &str = "imaginears";

/// Sent to the UI as `{ "action": "play" }` or `{ "action": "link", "token": "…" }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Open or focus the launcher.
    Open,
    /// Open the launcher on the Play screen, ready to join the server.
    Play,
    /// Ask the player whether to verify their Minecraft account on the website.
    /// `token` is always 40 lowercase hex characters.
    Link { token: String },
}

impl Action {
    /// For telemetry: never includes the token.
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Open => "open",
            Action::Play => "play",
            Action::Link { .. } => "link",
        }
    }
}

/// `link/<token>` (nothing after the token but one optional `/`) with a valid token.
fn link_action(rest: &str) -> Action {
    let token = rest.strip_suffix('/').unwrap_or(rest);
    if is_valid_token(token) {
        Action::Link {
            token: token.to_string(),
        }
    } else {
        Action::Open
    }
}

/// The action of an `imaginears://` URL, or `None` for any other scheme.
pub fn parse(url: &Url) -> Option<Action> {
    if !url.scheme().eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    // `imaginears://play` puts the action in the host; `imaginears:play` in the path.
    // `rest` is what follows the action (`<token>` for `imaginears://link/<token>`).
    let (name, rest) = match url.host_str().filter(|h| !h.is_empty()) {
        Some(host) => (host, url.path().strip_prefix('/').unwrap_or(url.path())),
        None => {
            let path = url.path().strip_prefix('/').unwrap_or(url.path());
            path.split_once('/').unwrap_or((path, ""))
        }
    };
    Some(match name.to_ascii_lowercase().as_str() {
        "play" => Action::Play,
        "link" => link_action(rest),
        _ => Action::Open,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Option<Action> {
        parse(&Url::parse(s).unwrap())
    }

    #[test]
    fn known_actions() {
        assert_eq!(p("imaginears://play"), Some(Action::Play));
        assert_eq!(p("imaginears://play/"), Some(Action::Play));
        assert_eq!(p("IMAGINEARS://PLAY"), Some(Action::Play));
        assert_eq!(p("imaginears:play"), Some(Action::Play));
        assert_eq!(
            p("imaginears://play?server=evil.example"),
            Some(Action::Play)
        );
        assert_eq!(p("imaginears://open"), Some(Action::Open));
    }

    #[test]
    fn unknown_actions_just_open() {
        assert_eq!(p("imaginears://"), Some(Action::Open));
        assert_eq!(p("imaginears://show/fireworks"), Some(Action::Open));
        assert_eq!(p("imaginears://launch-game-now"), Some(Action::Open));
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn link_carries_a_valid_token() {
        let link = Some(Action::Link {
            token: TOKEN.to_string(),
        });
        assert_eq!(p(&format!("imaginears://link/{TOKEN}")), link);
        assert_eq!(p(&format!("imaginears://link/{TOKEN}/")), link);
        assert_eq!(p(&format!("IMAGINEARS://LINK/{TOKEN}")), link);
        assert_eq!(p(&format!("imaginears:link/{TOKEN}")), link);
        // The query is ignored: links can't pick the website or account.
        assert_eq!(
            p(&format!(
                "imaginears://link/{TOKEN}?site=https://evil.example&user=x"
            )),
            link
        );
    }

    #[test]
    fn link_with_a_bad_token_just_opens() {
        assert_eq!(p("imaginears://link"), Some(Action::Open));
        assert_eq!(p("imaginears://link/"), Some(Action::Open));
        assert_eq!(
            p(&format!("imaginears://link/{}", TOKEN.to_uppercase())),
            Some(Action::Open)
        );
        assert_eq!(
            p(&format!("imaginears://link/{}", &TOKEN[..39])),
            Some(Action::Open)
        );
        assert_eq!(
            p(&format!("imaginears://link/{TOKEN}/extra")),
            Some(Action::Open)
        );
        assert_eq!(p("imaginears://link/..%2F..%2Fapi"), Some(Action::Open));
        // Exactly one optional slash after the token, none extra before it.
        assert_eq!(
            p(&format!("imaginears://link//{TOKEN}")),
            Some(Action::Open)
        );
        assert_eq!(
            p(&format!("imaginears://link/{TOKEN}//")),
            Some(Action::Open)
        );
        assert_eq!(p(&format!("imaginears:link//{TOKEN}")), Some(Action::Open));
    }

    #[test]
    fn telemetry_name_never_contains_the_token() {
        let link = Action::Link {
            token: TOKEN.to_string(),
        };
        assert_eq!(link.as_str(), "link");
    }

    #[test]
    fn ui_receives_tagged_json() {
        let link = Action::Link {
            token: TOKEN.to_string(),
        };
        assert_eq!(
            serde_json::to_string(&link).unwrap(),
            format!(r#"{{"action":"link","token":"{TOKEN}"}}"#)
        );
        assert_eq!(
            serde_json::to_string(&Action::Play).unwrap(),
            r#"{"action":"play"}"#
        );
    }

    #[test]
    fn other_schemes_are_ignored() {
        assert_eq!(p("https://imaginears.club/play"), None);
        assert_eq!(p("magic://play"), None);
        assert_eq!(p("file:///C:/play"), None);
    }
}
