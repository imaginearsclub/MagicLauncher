//! Maintenance mode: a small hosted file (`launcher/status.json`) that disables Play with a
//! message while the server or pack is being updated. The owner toggles it with
//! `pack-publisher --maintenance "Back at 18:00"` / `--maintenance off`.
//!
//! Failing to read it never blocks players: missing file, network error or bad JSON all
//! mean "not in maintenance".

use crate::channel::LAUNCHER_FEED_BASE;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Maintenance {
    pub maintenance: bool,
    /// Shown to players, e.g. "Updating the pack, back at 18:00 CET".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Unix seconds when maintenance was switched on (informational).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
}

pub fn url() -> String {
    format!("{LAUNCHER_FEED_BASE}status.json")
}

/// The current maintenance state; `Maintenance::default()` (off) if it can't be read.
pub async fn current(client: &reqwest::Client) -> Maintenance {
    let resp = client
        .get(url())
        .header("cache-control", "no-cache")
        .timeout(Duration::from_secs(6))
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => Maintenance::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_defaults() {
        let m: Maintenance =
            serde_json::from_str(r#"{"maintenance":true,"message":"Back at 18:00"}"#).unwrap();
        assert!(m.maintenance);
        assert_eq!(m.message.as_deref(), Some("Back at 18:00"));
        let off: Maintenance = serde_json::from_str(r#"{"maintenance":false}"#).unwrap();
        assert_eq!(off, Maintenance::default());
    }
}
