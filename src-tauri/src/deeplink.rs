//! `imaginears://` links, so a button on the website can open the launcher.
//!
//! Any web page can send these links, so they only ever select from a fixed set of actions
//! and never start the game: `imaginears://play` opens the Play screen, and the player still
//! clicks Play. Everything else in the URL (query, extra path) is ignored. An unknown action
//! (a link made for a newer launcher) just opens the launcher.

use serde::Serialize;
use tauri::Url;

/// Registered in `tauri.conf.json` (`plugins.deep-link.desktop.schemes`).
pub const SCHEME: &str = "imaginears";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Open or focus the launcher.
    Open,
    /// Open the launcher on the Play screen, ready to join the server.
    Play,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Open => "open",
            Action::Play => "play",
        }
    }
}

/// The action of an `imaginears://` URL, or `None` for any other scheme.
pub fn parse(url: &Url) -> Option<Action> {
    if !url.scheme().eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    // `imaginears://play` puts the action in the host; `imaginears:play` in the path.
    let name = url
        .host_str()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| url.path())
        .trim_matches('/')
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    Some(match name.as_str() {
        "play" => Action::Play,
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

    #[test]
    fn other_schemes_are_ignored() {
        assert_eq!(p("https://imaginears.club/play"), None);
        assert_eq!(p("magic://play"), None);
        assert_eq!(p("file:///C:/play"), None);
    }
}
