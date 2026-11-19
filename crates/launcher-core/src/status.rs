//! Live server status via the Minecraft "server list ping" (what the multiplayer screen
//! shows): players online/max, MOTD, version and latency.
//!
//! The address is resolved like the game does: an explicit `host:port` is used as-is,
//! otherwise the `_minecraft._tcp.<host>` SRV record (our survival address has *only* an SRV
//! record), falling back to `host:25565`. SRV is looked up over DNS-over-HTTPS so no extra
//! resolver library is needed.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const TIMEOUT: Duration = Duration::from_secs(6);
/// Protocol number sent in the handshake; servers answer status pings for any value.
/// It doesn't need to match the server version, so it isn't tied to the pack.
const PROTOCOL: i32 = 767;

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub online: u32,
    pub max: u32,
    /// MOTD as plain text (colour codes removed), lines separated by `\n`.
    pub motd: String,
    pub version: String,
    pub latency_ms: u32,
    /// A few online player names, if the server shares them.
    pub sample: Vec<String>,
}

// ---------- address resolution ----------

#[derive(Deserialize)]
struct DohAnswer {
    #[serde(rename = "type")]
    kind: u16,
    data: String,
}

#[derive(Deserialize)]
struct DohResponse {
    #[serde(rename = "Answer", default)]
    answer: Vec<DohAnswer>,
}

/// Parse an SRV record's data: `priority weight port target.`
/// One SRV answer: `priority weight port target`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Srv {
    priority: u16,
    weight: u16,
    target: String,
    port: u16,
}

fn parse_srv(data: &str) -> Option<Srv> {
    let mut it = data.split_whitespace();
    let priority = it.next()?.parse().ok()?;
    let weight = it.next()?.parse().ok()?;
    let port = it.next()?.parse().ok()?;
    let target = it.next()?.trim_end_matches('.').to_string();
    (!target.is_empty()).then_some(Srv {
        priority,
        weight,
        target,
        port,
    })
}

/// The record a client should use: lowest priority, then highest weight (RFC 2782; weight 0
/// means "only if nothing else"). Deterministic, so the status card doesn't flip between
/// records. imaginears.club has a weight-0 record behind Cloudflare's proxy, which can't
/// carry Minecraft traffic, next to the real one.
fn pick_srv(records: impl IntoIterator<Item = Srv>) -> Option<(String, u16)> {
    records
        .into_iter()
        .min_by_key(|r| (r.priority, std::cmp::Reverse(r.weight)))
        .map(|r| (r.target, r.port))
}

async fn srv_lookup(client: &reqwest::Client, host: &str) -> Option<(String, u16)> {
    let name = format!("_minecraft._tcp.{host}");
    for resolver in [
        "https://cloudflare-dns.com/dns-query",
        "https://dns.google/resolve",
    ] {
        let resp = client
            .get(resolver)
            .query(&[("name", name.as_str()), ("type", "SRV")])
            .header("accept", "application/dns-json")
            .timeout(Duration::from_secs(4))
            .send()
            .await;
        if let Ok(r) = resp {
            if let Ok(doh) = r.json::<DohResponse>().await {
                // A valid answer (even an empty one) is authoritative; don't ask the next resolver.
                return pick_srv(
                    doh.answer
                        .iter()
                        .filter(|a| a.kind == 33)
                        .filter_map(|a| parse_srv(&a.data)),
                );
            }
        }
    }
    None
}

/// Split `host[:port]`. IPv6 literals aren't used for our servers.
fn split_address(addr: &str) -> (String, Option<u16>) {
    match addr.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().ok()),
        _ => (addr.to_string(), None),
    }
}

/// Where to connect: (host to dial, port). The handshake still names the original host.
pub async fn resolve(client: &reqwest::Client, address: &str) -> (String, u16) {
    let (host, port) = split_address(address);
    if let Some(p) = port {
        return (host, p);
    }
    srv_lookup(client, &host).await.unwrap_or((host, 25565))
}

// ---------- protocol ----------

fn write_varint(buf: &mut Vec<u8>, mut v: i32) {
    loop {
        let byte = (v & 0x7f) as u8;
        v = ((v as u32) >> 7) as i32;
        if v == 0 {
            buf.push(byte);
            return;
        }
        buf.push(byte | 0x80);
    }
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    write_varint(buf, s.len() as i32);
    buf.extend_from_slice(s.as_bytes());
}

fn packet(id: i32, body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(body.len() + 5);
    write_varint(&mut inner, id);
    inner.extend_from_slice(body);
    let mut out = Vec::with_capacity(inner.len() + 5);
    write_varint(&mut out, inner.len() as i32);
    out.extend_from_slice(&inner);
    out
}

fn handshake(host: &str, port: u16) -> Vec<u8> {
    let mut body = Vec::new();
    write_varint(&mut body, PROTOCOL);
    write_string(&mut body, host);
    body.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut body, 1); // next state: status
    packet(0x00, &body)
}

async fn read_varint(s: &mut TcpStream) -> Result<i32> {
    let mut value = 0i32;
    for i in 0..5 {
        let b = s.read_u8().await?;
        value |= ((b & 0x7f) as i32) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(Error::other("server sent a malformed VarInt"))
}

/// Flatten a chat component (string, `{text, extra}` or array) to plain text.
fn chat_text(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::String(s) => out.push_str(s),
        serde_json::Value::Array(a) => a.iter().for_each(|x| chat_text(x, out)),
        serde_json::Value::Object(o) => {
            if let Some(t) = o.get("text") {
                chat_text(t, out);
            }
            if let Some(e) = o.get("extra") {
                chat_text(e, out);
            }
        }
        _ => {}
    }
}

/// Remove legacy `§x` colour/format codes.
fn strip_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_status(json: &str, latency_ms: u32) -> Result<ServerStatus> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let mut motd = String::new();
    if let Some(d) = v.get("description") {
        chat_text(d, &mut motd);
    }
    let players = v.get("players");
    let num = |k: &str| {
        players
            .and_then(|p| p.get(k))
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as u32
    };
    let sample = players
        .and_then(|p| p.get("sample"))
        .and_then(|s| s.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
                .map(strip_codes)
                .filter(|n| !n.trim().is_empty())
                .take(12)
                .collect()
        })
        .unwrap_or_default();
    Ok(ServerStatus {
        online: num("online"),
        max: num("max"),
        motd: strip_codes(motd.trim()),
        version: v
            .pointer("/version/name")
            .and_then(|n| n.as_str())
            .map(strip_codes)
            .unwrap_or_default(),
        latency_ms,
        sample,
    })
}

async fn ping_inner(client: &reqwest::Client, address: &str) -> Result<ServerStatus> {
    let (host, _) = split_address(address);
    let (dial_host, port) = resolve(client, address).await;
    let mut s = TcpStream::connect((dial_host.as_str(), port)).await?;
    s.set_nodelay(true)?;

    // Handshake names the address players type (a proxy may route on it).
    s.write_all(&handshake(&host, port)).await?;
    s.write_all(&packet(0x00, &[])).await?;

    let _len = read_varint(&mut s).await?;
    if read_varint(&mut s).await? != 0x00 {
        return Err(Error::other("unexpected status response"));
    }
    let n = read_varint(&mut s).await?;
    if !(0..=262_144).contains(&n) {
        return Err(Error::other("status response too large"));
    }
    let mut json = vec![0u8; n as usize];
    s.read_exact(&mut json).await?;

    // Latency: ping/pong round trip.
    let started = Instant::now();
    s.write_all(&packet(0x01, &42i64.to_be_bytes())).await?;
    let _ = read_varint(&mut s).await?;
    let _ = read_varint(&mut s).await?;
    let _ = s.read_i64().await?;
    let latency = started.elapsed().as_millis().min(u32::MAX as u128) as u32;

    parse_status(&String::from_utf8_lossy(&json), latency)
}

/// Ping `address` (`host` or `host:port`). Errors mean "offline / unreachable".
pub async fn ping(client: &reqwest::Client, address: &str) -> Result<ServerStatus> {
    tokio::time::timeout(TIMEOUT, ping_inner(client, address))
        .await
        .map_err(|_| Error::other("server did not answer in time"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints() {
        let enc = |v| {
            let mut b = Vec::new();
            write_varint(&mut b, v);
            b
        };
        assert_eq!(enc(0), [0x00]);
        assert_eq!(enc(300), [0xac, 0x02]);
        assert_eq!(enc(767), [0xff, 0x05]);
        assert_eq!(enc(-1), [0xff, 0xff, 0xff, 0xff, 0x0f]);
    }

    #[test]
    fn handshake_layout() {
        let h = handshake("a.b", 25565);
        // len, id 0, protocol 767 (2 bytes), "a.b" (1+3), port (2), state 1
        assert_eq!(
            h,
            [10, 0x00, 0xff, 0x05, 3, b'a', b'.', b'b', 0x63, 0xdd, 0x01]
        );
    }

    #[test]
    fn srv_and_addresses() {
        assert_eq!(
            pick_srv(parse_srv("0 0 25565 mc.imaginears.club.")),
            Some(("mc.imaginears.club".into(), 25565))
        );
        // The real imaginears.club records: the weight-5 host wins over the weight-0 one,
        // whatever order the resolver returns them in.
        let real = || {
            [
                "0 0 25565 iears.us.",
                "0 5 25565 ns1006901.ip-135-148-53.us.",
            ]
            .into_iter()
            .filter_map(parse_srv)
        };
        assert_eq!(
            pick_srv(real()),
            Some(("ns1006901.ip-135-148-53.us".into(), 25565))
        );
        assert_eq!(
            pick_srv(real().rev()),
            Some(("ns1006901.ip-135-148-53.us".into(), 25565))
        );
        // Lower priority beats any weight.
        assert_eq!(
            pick_srv(["1 100 1 a.", "0 0 2 b."].into_iter().filter_map(parse_srv)),
            Some(("b".into(), 2))
        );
        assert_eq!(
            split_address("15.204.100.238:25563"),
            ("15.204.100.238".into(), Some(25563))
        );
        assert_eq!(
            split_address("imaginears.club"),
            ("imaginears.club".into(), None)
        );
    }

    #[test]
    fn parses_component_motd_and_players() {
        let json = r#"{"version":{"name":"Velocity 3.3"},"players":{"max":100,"online":7,
            "sample":[{"name":"§aClarkcj","id":"x"}]},
            "description":{"text":"§6Imaginears ","extra":[{"text":"Club"},"\n§7Where dreams come true"]}}"#;
        let s = parse_status(json, 16).unwrap();
        assert_eq!((s.online, s.max, s.latency_ms), (7, 100, 16));
        assert_eq!(s.motd, "Imaginears Club\nWhere dreams come true");
        assert_eq!(s.version, "Velocity 3.3");
        assert_eq!(s.sample, ["Clarkcj"]);
    }

    #[test]
    fn parses_plain_string_motd() {
        let s = parse_status(
            r#"{"description":"§cHello","players":{"max":1,"online":0}}"#,
            1,
        )
        .unwrap();
        assert_eq!(s.motd, "Hello");
    }

    /// Live check against the real server: `cargo test -p launcher-core -- --ignored live_ping --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_ping() {
        let c = crate::http_client();
        println!("{:?}", resolve(&c, crate::config::SERVER_ADDRESS).await);
        println!("{:#?}", ping(&c, crate::config::SERVER_ADDRESS).await);
    }
}
