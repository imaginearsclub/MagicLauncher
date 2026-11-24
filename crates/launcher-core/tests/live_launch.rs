//! End-to-end check of the real install + launch path, without the UI and without a Microsoft
//! account: serves a small Fabric pack.json from a local HTTP server, runs `pipeline::prepare`
//! (vanilla, Java runtime, Fabric, libraries, assets, pack sync), starts the game with an
//! offline session and waits until the game log shows Fabric loaded and the client finished
//! loading its resources (≈ title screen). Then it closes the game.
//!
//! Downloads ~0.5 GB the first time and opens a game window, so it's ignored by default:
//!
//! ```sh
//! MAGIC_LIVE_DIR=<scratch folder> cargo test -p launcher-core --test live_launch -- --ignored --nocapture
//! ```
//!
//! `MAGIC_LIVE_PACK_URL=https://launcher.imaginears.club/alpha/pack.json` boots a published
//! pack instead of the built-in two-mod one.

use launcher_core::auth::{Profile, Session};
use launcher_core::channel::Channel;
use launcher_core::config::Settings;
use launcher_core::paths::Paths;
use launcher_core::progress::{Progress, ProgressFn};
use launcher_core::{http_client, launch, pack, pipeline};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MC: &str = "26.3";
const FABRIC: &str = "0.20.1";

/// A pack file entry for the newest Fabric build of a Modrinth project for `MC`.
async fn modrinth_file(client: &reqwest::Client, slug: &str) -> serde_json::Value {
    let url = format!(
        "https://api.modrinth.com/v2/project/{slug}/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%22{MC}%22%5D"
    );
    let versions: Vec<serde_json::Value> =
        client.get(&url).send().await.unwrap().json().await.unwrap();
    let v = versions
        .first()
        .unwrap_or_else(|| panic!("{slug} has no Fabric build for {MC}"));
    let files = v["files"].as_array().unwrap();
    let f = files
        .iter()
        .find(|f| f["primary"].as_bool() == Some(true))
        .unwrap_or(&files[0]);
    serde_json::json!({
        "path": format!("mods/{}", f["filename"].as_str().unwrap()),
        "sha1": f["hashes"]["sha1"],
        "size": f["size"],
        "url": f["url"],
        "policy": "managed"
    })
}

/// Serve `body` for every request on 127.0.0.1:<port> (enough for one manifest fetch).
async fn serve(body: String) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}/pack.json")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn installs_and_boots_fabric_pack() {
    let root = std::env::var_os("MAGIC_LIVE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("magiclauncher-live"));
    std::fs::create_dir_all(&root).unwrap();
    let paths = Paths::new(&root);
    let client = http_client();

    let files = vec![
        modrinth_file(&client, "fabric-api").await,
        modrinth_file(&client, "sodium").await,
    ];
    let manifest = serde_json::json!({
        "format_version": 1,
        "id": "imaginears-magic-pack",
        "name": "Imaginears Magic Pack (live test)",
        "version": "0.0.1",
        "minecraft": MC,
        "loader": { "type": "fabric", "version": FABRIC },
        "recommended_memory_mb": 3072,
        "files": files
    });
    // MAGIC_LIVE_PACK_URL tests a published pack instead (e.g. the Alpha pack).
    let url = match std::env::var("MAGIC_LIVE_PACK_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => serve(manifest.to_string()).await,
    };
    // The versions to expect in the game log come from the pack actually booted, so a
    // published pack on another Minecraft version is checked against its own versions.
    let booted = pack::fetch_manifest(&client, &url)
        .await
        .expect("pack.json");
    let (mc, fabric) = (booted.minecraft, booted.loader.version);

    let progress: ProgressFn = Arc::new(|p: Progress| {
        if p.done == 0 || p.done == p.total {
            println!("[{}/{}] {}", p.done, p.total, p.stage);
        }
    });
    let settings = Settings {
        auto_join: false,
        ..Settings::default()
    };
    let prepared = pipeline::prepare(
        &client,
        &paths,
        &settings,
        Channel::Release,
        &url,
        false,
        &progress,
    )
    .await
    .expect("prepare");
    println!("Prepared pack {}; starting the game", prepared.pack_version);

    let session = Session {
        profile: Profile {
            name: "MagicTest".into(),
            uuid: "00000000000000000000000000000001".into(),
        },
        access_token: "0".into(),
        xuid: None,
        expires_at: u64::MAX,
        user_type: "legacy",
    };
    let latest = prepared.spec.game_dir.join("logs").join("latest.log");
    let _ = std::fs::remove_file(&latest);
    let mut child = launch::spawn(&paths, &prepared.spec, &session).expect("spawn");

    let fabric_line = format!("Loading Minecraft {mc} with Fabric Loader {fabric}");
    let started = Instant::now();
    let (mut saw_fabric, mut saw_loaded) = (false, false);
    while started.elapsed() < Duration::from_secs(240) {
        if let Ok(Some(status)) = child.try_wait() {
            panic!(
                "the game exited early ({status}); see {}",
                prepared.spec.game_dir.join("logs").display()
            );
        }
        let log = std::fs::read_to_string(&latest).unwrap_or_default();
        saw_fabric |= log.contains(&fabric_line);
        // Logged once the client has loaded its resources (sound engine / texture atlases).
        saw_loaded |= log.contains("Sound engine started") || log.contains("Created: 1024x");
        if saw_fabric && saw_loaded {
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(saw_fabric, "no \"{fabric_line}\" in {}", latest.display());
    assert!(
        saw_loaded,
        "the client didn't finish loading; see {}",
        latest.display()
    );
    println!(
        "Fabric {fabric} on Minecraft {mc} booted to resource load in {:?}",
        started.elapsed()
    );
}
