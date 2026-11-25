//! Core of the Imaginears Club launcher.
//!
//! Nothing in here depends on Tauri, so it can be unit-tested headless and
//! reused by other front ends (a CLI, a native UI) later.

pub mod allowlist;
pub mod auth;
pub mod channel;
pub mod config;
pub mod crash;
pub mod download;
pub mod error;
pub mod fabric;
pub mod java;
pub mod launch;
pub mod maintenance;
pub mod mojang;
pub mod pack;
pub mod paths;
pub mod pipeline;
pub mod progress;
pub mod rules;
pub mod skin;
pub mod status;
pub mod telemetry;
pub mod website_link;

pub use error::{Error, Result};

/// The launcher version, inherited from the workspace `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared HTTP client with a descriptive, contact-free User-Agent.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("MagicLauncher/{VERSION}"))
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("failed to build HTTP client")
}
