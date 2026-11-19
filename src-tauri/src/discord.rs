//! Discord Rich Presence over Discord's local IPC (no SDK files). Shows "In the launcher" or
//! "Playing Imaginears Club · Pack x" with the server's player count while the game runs.
//! Runs on its own thread; if Discord isn't running, updates are skipped and retried on the
//! next one. Players can turn it off in Settings (`discord_presence`).

use discord_rich_presence::activity::{Activity, Assets, Button, Party, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use launcher_core::config;
use std::sync::mpsc::{channel, Sender};

pub enum Presence {
    Launcher,
    Playing {
        pack_version: String,
        online: Option<(u32, u32)>,
        started: i64,
    },
    Clear,
}

#[derive(Clone)]
pub struct Discord {
    tx: Sender<Presence>,
}

const LOGO_URL: &str = "https://launcher.imaginears.club/launcher/assets/logo-512.png";
const DOWNLOAD_PAGE: &str = "https://launcher.imaginears.club/launcher/download/index.html";

impl Discord {
    pub fn start() -> Discord {
        let (tx, rx) = channel::<Presence>();
        std::thread::spawn(move || {
            let mut client = DiscordIpcClient::new(config::DISCORD_APP_ID);
            let mut connected = false;
            for p in rx {
                if !connected {
                    connected = client.connect().is_ok();
                    if !connected {
                        continue; // Discord not running: try again on the next update.
                    }
                }
                let result = match &p {
                    Presence::Clear => client.clear_activity(),
                    Presence::Launcher => client.set_activity(base().details("In the launcher")),
                    Presence::Playing {
                        pack_version,
                        online,
                        started,
                    } => {
                        let state = format!("Pack {pack_version}");
                        let mut a = base()
                            .details("Playing Imaginears Club")
                            .state(state.as_str())
                            .timestamps(Timestamps::new().start(*started));
                        if let Some((on, max)) = online {
                            if *max > 0 {
                                a = a.party(Party::new().size([(*on).max(1) as i32, *max as i32]));
                            }
                        }
                        client.set_activity(a)
                    }
                };
                if result.is_err() {
                    // Discord was closed or restarted; reconnect on the next update.
                    connected = false;
                    let _ = client.close();
                }
            }
        });
        Discord { tx }
    }

    pub fn set(&self, p: Presence) {
        let _ = self.tx.send(p);
    }
}

fn base<'a>() -> Activity<'a> {
    Activity::new()
        .assets(
            Assets::new()
                // A web image, so it works without uploading Discord art assets.
                .large_image(LOGO_URL)
                .large_text("Imaginears Club"),
        )
        .buttons(vec![Button::new("Get the launcher", DOWNLOAD_PAGE)])
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
