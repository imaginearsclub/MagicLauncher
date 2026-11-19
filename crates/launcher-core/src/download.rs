//! Parallel, hash-verified downloads. Files whose SHA-1 already matches are skipped,
//! so re-running an install only fetches what changed.

use crate::progress::{report, ProgressFn};
use crate::{Error, Result};
use futures_util::{stream, StreamExt};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const PARALLEL: usize = 16;
const RETRIES: usize = 3;

#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}

pub async fn sha1_file(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<String> {
        use std::io::Read;
        let mut f = std::fs::File::open(&path)?;
        let mut h = Sha1::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(hex::encode(h.finalize()))
    })
    .await
    .map_err(|e| Error::other(e.to_string()))?
}

/// True if `path` exists and (when a hash is given) matches it.
pub async fn is_valid(path: &Path, sha1: Option<&str>) -> bool {
    if !path.is_file() {
        return false;
    }
    match sha1 {
        None => true,
        Some(want) => sha1_file(path)
            .await
            .map(|got| got.eq_ignore_ascii_case(want))
            .unwrap_or(false),
    }
}

pub async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let mut last = None;
    for attempt in 0..RETRIES {
        match client
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => last = Some(e),
            },
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1))).await;
    }
    Err(last.expect("at least one attempt").into())
}

pub async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T> {
    Ok(serde_json::from_slice(&fetch_bytes(client, url).await?)?)
}

/// Download one file unless it is already present and valid.
pub async fn download_one(client: &reqwest::Client, d: &Download) -> Result<()> {
    if is_valid(&d.dest, d.sha1.as_deref()).await {
        return Ok(());
    }
    let bytes = fetch_bytes(client, &d.url).await?;
    if let Some(want) = &d.sha1 {
        let got = sha1_hex(&bytes);
        if !got.eq_ignore_ascii_case(want) {
            return Err(Error::HashMismatch {
                url: d.url.clone(),
                expected: want.clone(),
                actual: got,
            });
        }
    }
    if let Some(dir) = d.dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    // Write to a temp file then rename, so a crash never leaves a half-written jar.
    let tmp = d.dest.with_extension("part");
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, &d.dest).await?;
    Ok(())
}

/// Download many files in parallel, reporting progress per finished file.
pub async fn download_all(
    client: &reqwest::Client,
    items: Vec<Download>,
    stage: &str,
    progress: &ProgressFn,
) -> Result<()> {
    let total = items.len() as u64;
    let done = Arc::new(AtomicU64::new(0));
    report(progress, stage, 0, total);

    let results: Vec<Result<()>> = stream::iter(items)
        .map(|d| {
            let done = done.clone();
            let progress = progress.clone();
            async move {
                let r = download_one(client, &d).await;
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                report(&progress, stage, n, total);
                r
            }
        })
        .buffer_unordered(PARALLEL)
        .collect()
        .await;

    results.into_iter().collect()
}
