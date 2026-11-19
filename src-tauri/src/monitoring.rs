//! Consent-gated telemetry: Sentry for the launcher's own errors/panics, Aptabase for anonymous
//! usage events. Nothing runs until the player says yes (`Settings::telemetry == Some(true)`).
//!
//! Privacy: `send_default_pii` is off, the machine name isn't sent, and every event is scrubbed
//! of home folders (they contain the OS user name) and tokens before it leaves the PC. The Sentry
//! projects also have "Prevent Storing of IP Addresses" on.

use launcher_core::config::{self, Settings};
use launcher_core::crash;
use launcher_core::telemetry::Telemetry;
use std::sync::{Arc, Mutex};

static SENTRY: Mutex<Option<sentry::ClientInitGuard>> = Mutex::new(None);

fn sentry_options() -> sentry::ClientOptions {
    let mut o = sentry::ClientOptions::new();
    o.dsn = config::SENTRY_DSN.parse().ok();
    o.release = Some(format!("magiclauncher@{}", launcher_core::VERSION).into());
    o.environment = Some(
        if cfg!(debug_assertions) {
            "development"
        } else {
            "production"
        }
        .into(),
    );
    o.send_default_pii = false;
    // The default would be the PC's name.
    o.server_name = Some("launcher".into());
    o.attach_stacktrace = true;
    o.before_send = Some(Arc::new(|mut event| {
        if let Some(m) = event.message.as_mut() {
            *m = crash::scrub(m);
        }
        for e in event.exception.values.iter_mut() {
            if let Some(v) = e.value.as_mut() {
                *v = crash::scrub(v);
            }
        }
        event.user = None;
        Some(event)
    }));
    o
}

/// Start Sentry if it isn't running. Call before the async runtime starts when possible.
pub fn start_sentry() {
    let mut g = SENTRY.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(sentry::init(sentry_options()));
    }
}

/// Stop sending (consent withdrawn): flush and close the client.
pub fn stop_sentry() {
    let guard = SENTRY.lock().unwrap_or_else(|e| e.into_inner()).take();
    drop(guard);
    if let Some(c) = sentry::Hub::main().client() {
        c.close(Some(std::time::Duration::from_secs(2)));
    }
}

/// Startup: read consent straight from settings.json (Tauri isn't running yet).
pub fn init_from_settings() {
    let consent = config::default_data_dir()
        .map(|d| Settings::load(&d.join("settings.json")).telemetry == Some(true))
        .unwrap_or(false);
    // Dev builds can force it on to verify the setup.
    let forced = cfg!(debug_assertions) && std::env::var_os("MAGIC_SENTRY_TEST").is_some();
    if consent || forced {
        start_sentry();
    }
}

/// Report a launcher error to Sentry (no-op without consent). Tagged with where it happened
/// and its category; the message is scrubbed in `before_send`.
pub fn report_error(context: &str, e: &launcher_core::Error) {
    if SENTRY.lock().map(|g| g.is_none()).unwrap_or(true) {
        return;
    }
    sentry::with_scope(
        |scope| {
            scope.set_tag("context", context);
            scope.set_tag("category", e.category());
        },
        || sentry::capture_error(e),
    );
}

/// Anonymous usage events (Aptabase), sent in the background, only with consent.
pub struct Usage {
    telemetry: Telemetry,
    client: reqwest::Client,
}

impl Usage {
    pub fn new(client: reqwest::Client) -> Arc<Self> {
        Arc::new(Usage {
            telemetry: Telemetry::detect(),
            client,
        })
    }

    pub fn track(
        self: &Arc<Self>,
        settings_path: &std::path::Path,
        name: &'static str,
        props: serde_json::Value,
    ) {
        if Settings::load(settings_path).telemetry != Some(true) {
            return;
        }
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            me.telemetry.send(&me.client, name, props).await;
        });
    }
}
