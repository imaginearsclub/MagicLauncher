//! Release channels: Release (public), Beta (public testing), Alpha (staff and trusted
//! players, gated by the signed allowlist in `allowlist.rs`).
//!
//! Each channel has its own update feed, its own game folder (so a test build can never
//! damage the Release install) and optionally its own modpack (falling back to Release's).

use serde::{Deserialize, Serialize};

/// Where launcher update feeds live. `latest.json` here is the Release feed that every
/// 0.1.x launcher already reads, so it must never change meaning.
pub const LAUNCHER_FEED_BASE: &str = "https://launcher.imaginears.club/launcher/";

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Release,
    Beta,
    Alpha,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Release, Channel::Beta, Channel::Alpha];

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Release => "release",
            Channel::Beta => "beta",
            Channel::Alpha => "alpha",
        }
    }

    /// The launcher update feed for this channel.
    pub fn feed_url(self) -> String {
        match self {
            Channel::Release => format!("{LAUNCHER_FEED_BASE}latest.json"),
            other => format!("{LAUNCHER_FEED_BASE}{}/latest.json", other.as_str()),
        }
    }

    /// Game folder name under the launcher's data directory. Release keeps the original
    /// `instance` so existing installs are untouched.
    pub fn instance_dir(self) -> &'static str {
        match self {
            Channel::Release => "instance",
            Channel::Beta => "instance-beta",
            Channel::Alpha => "instance-alpha",
        }
    }

    /// This channel's own pack manifest, derived from the Release one:
    /// `…/pack.json` → `…/beta/pack.json`. `None` for Release.
    pub fn pack_manifest_url(self, release_url: &str) -> Option<String> {
        if self == Channel::Release {
            return None;
        }
        let (dir, file) = release_url.rsplit_once('/')?;
        Some(format!("{dir}/{}/{file}", self.as_str()))
    }

    /// Which channel a launcher version belongs to: `x.y.z` Release, `-beta.N` Beta,
    /// `-alpha.N` Alpha. Any other pre-release (e.g. `-rc.1`) counts as Beta.
    pub fn of_version(v: &semver::Version) -> Channel {
        let pre = v.pre.as_str();
        if pre.is_empty() {
            Channel::Release
        } else if pre.starts_with("alpha") {
            Channel::Alpha
        } else {
            Channel::Beta
        }
    }
}

/// The channel actually used: Alpha only while the signed-in account is on the allowlist.
/// Someone removed from Alpha follows Beta (feed, pack and game folder); their Alpha folder
/// stays on disk untouched and the setting is kept, so re-adding them restores it.
pub fn effective(selected: Channel, alpha_allowed: bool) -> Channel {
    if selected == Channel::Alpha && !alpha_allowed {
        Channel::Beta
    } else {
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feeds_and_folders() {
        assert_eq!(
            Channel::Release.feed_url(),
            "https://launcher.imaginears.club/launcher/latest.json"
        );
        assert_eq!(
            Channel::Beta.feed_url(),
            "https://launcher.imaginears.club/launcher/beta/latest.json"
        );
        assert_eq!(Channel::Release.instance_dir(), "instance");
        assert_eq!(Channel::Alpha.instance_dir(), "instance-alpha");
    }

    #[test]
    fn channel_pack_urls() {
        let r = "https://launcher.imaginears.club/pack.json";
        assert_eq!(Channel::Release.pack_manifest_url(r), None);
        assert_eq!(
            Channel::Alpha.pack_manifest_url(r).as_deref(),
            Some("https://launcher.imaginears.club/alpha/pack.json")
        );
    }

    #[test]
    fn version_channels() {
        let c = |s: &str| Channel::of_version(&semver::Version::parse(s).unwrap());
        assert_eq!(c("0.2.0"), Channel::Release);
        assert_eq!(c("0.2.1-beta.1"), Channel::Beta);
        assert_eq!(c("0.3.0-alpha.10"), Channel::Alpha);
        assert_eq!(c("0.3.0-rc.1"), Channel::Beta);
    }

    #[test]
    fn removed_alpha_tester_follows_beta() {
        assert_eq!(effective(Channel::Alpha, false), Channel::Beta);
        assert_eq!(effective(Channel::Alpha, true), Channel::Alpha);
        assert_eq!(effective(Channel::Release, false), Channel::Release);
    }

    #[test]
    fn settings_value_round_trips() {
        assert_eq!(serde_json::to_string(&Channel::Beta).unwrap(), "\"beta\"");
        let c: Channel = serde_json::from_str("\"alpha\"").unwrap();
        assert_eq!(c, Channel::Alpha);
    }
}
