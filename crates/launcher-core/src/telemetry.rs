//! Anonymous usage telemetry via Aptabase (https://aptabase.com), only with the player's
//! consent (`Settings::telemetry == Some(true)`).
//!
//! What is sent: event names (e.g. `play_finished`), small props (ok/failed, an error
//! *category*, durations, pack version, channel, crash signature), and system info
//! (OS, launcher version, locale). Never: names, UUIDs, file paths, error messages or logs.
//! There is no install ID; Aptabase counts users with a daily-rotating anonymous hash.

use crate::config::APTABASE_APP_KEY;
use serde::Serialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemProps {
    is_debug: bool,
    os_name: String,
    os_version: String,
    locale: String,
    app_version: String,
    sdk_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Event<'a> {
    timestamp: String,
    session_id: &'a str,
    event_name: &'a str,
    system_props: SystemProps,
    props: serde_json::Value,
}

/// The ingest host for an app key (`A-US-…` → us.aptabase.com, `A-EU-…` → eu.aptabase.com).
fn host(key: &str) -> Option<&'static str> {
    match key.split('-').nth(1)? {
        "US" => Some("https://us.aptabase.com"),
        "EU" => Some("https://eu.aptabase.com"),
        _ => None,
    }
}

fn os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    }
}

/// RFC 3339 UTC timestamp without pulling in a date library.
fn rfc3339(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// One Aptabase session per launcher start.
pub fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{}{:08}",
        nanos / 1_000_000_000,
        (nanos % 1_000_000_000) / 10
    )
}

pub struct Telemetry {
    session_id: String,
    os_version: String,
    locale: String,
}

impl Telemetry {
    pub fn new(os_version: String, locale: String) -> Self {
        Telemetry {
            session_id: new_session_id(),
            os_version,
            locale,
        }
    }

    /// Telemetry for this machine: OS version from the system, locale from `LANG` (or en-US).
    pub fn detect() -> Self {
        let os_version = sysinfo::System::os_version().unwrap_or_default();
        let locale = std::env::var("LANG")
            .ok()
            .and_then(|l| l.split('.').next().map(|s| s.replace('_', "-")))
            .filter(|l| !l.is_empty() && l != "C")
            .unwrap_or_else(|| "en-US".into());
        Telemetry::new(os_version, locale)
    }

    fn payload(&self, name: &str, props: serde_json::Value) -> serde_json::Value {
        serde_json::to_value([Event {
            timestamp: rfc3339(SystemTime::now()),
            session_id: &self.session_id,
            event_name: name,
            system_props: SystemProps {
                is_debug: cfg!(debug_assertions),
                os_name: os_name().into(),
                os_version: self.os_version.clone(),
                locale: self.locale.clone(),
                app_version: crate::VERSION.into(),
                sdk_version: format!("magiclauncher@{}", crate::VERSION),
            },
            props,
        }])
        .unwrap_or_default()
    }

    /// Send one event. Errors are ignored: telemetry must never affect the launcher.
    /// The caller checks consent.
    pub async fn send(&self, client: &reqwest::Client, name: &str, props: serde_json::Value) {
        let Some(host) = host(APTABASE_APP_KEY) else {
            return;
        };
        let _ = client
            .post(format!("{host}/api/v0/events"))
            .header("App-Key", APTABASE_APP_KEY)
            .json(&self.payload(name, props))
            .timeout(Duration::from_secs(10))
            .send()
            .await;
    }
}

/// Rounds a duration to whole seconds for props.
pub fn secs(d: Duration) -> u64 {
    d.as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_by_region() {
        assert_eq!(host("A-US-0000000000"), Some("https://us.aptabase.com"));
        assert_eq!(host("A-EU-1"), Some("https://eu.aptabase.com"));
        assert_eq!(host("nonsense"), None);
    }

    #[test]
    fn timestamps() {
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            rfc3339(UNIX_EPOCH + Duration::from_secs(1_795_000_000)),
            "2026-11-18T11:06:40Z"
        );
    }

    #[test]
    fn payload_shape_has_no_personal_fields() {
        let t = Telemetry::new("10.0.26220".into(), "en-US".into());
        let p = t.payload(
            "play_finished",
            serde_json::json!({"ok": true, "duration_s": 42}),
        );
        let e = &p[0];
        assert_eq!(e["eventName"], "play_finished");
        assert_eq!(e["systemProps"]["osName"], os_name());
        assert_eq!(e["props"]["duration_s"], 42);
        let text = p.to_string();
        for forbidden in ["uuid", "username", "Users"] {
            assert!(!text.contains(forbidden), "{forbidden} in {text}");
        }
    }
}
