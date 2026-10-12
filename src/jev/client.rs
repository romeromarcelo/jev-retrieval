//! Async System One client.
//!
//! Why async: server latency is 0.3–0.8 s per request regardless of size, so
//! wall time is dominated by how many requests fly at once —
//! `buffer_unordered(concurrency)` turns a serial ~27 s scan into a ~2 s one.
//! The retry policy (up to 3 retries, exponential from 250 ms, on transport
//! errors and 408/429/5xx) matches the observed ~1-in-40 transient failure
//! rate; the read timeout turns a stalled connection into such an error
//! instead of a hang.
//!
//! Why a client-side response cache: TypeSafe offers no server-side prompt caching
//! or repeated-state discount — resending an identical state is billed in full
//! (docs.typesafe.ai/models), and their cookbooks ship input-keyed caches as
//! standard practice. The cache here keys on
//! sha256(serialized request body), which covers model + concept +
//! window content in one hash: any file edit changes the windows and therefore the
//! key, so entries can never go stale. Stored at user level (~/.cache) to keep
//! searched repositories free of tool artifacts.

use super::schemas::{JevRequest, JevResponse};
use anyhow::{bail, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const API_URL: &str = "https://api.typesafe.ai/v1/systemone";
const MAX_RETRIES: u32 = 3;
/// Idle limit per socket read, reset by every successful read. It bounds
/// every stall mode measured against mock servers (unroutable connect, silent
/// server, stalled body, unread upload) without cutting a slow but
/// progressing response, which a total timeout would. 10 s is the official
/// SDKs' per-operation default and >10× the slowest response observed (0.74 s).
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The single choke point every Jev request passes through: adds auth, the
/// retry policy, and the sha256-keyed response cache.
pub struct JevClient {
    http: reqwest::Client,
    key: String,
    cache_dir: Option<PathBuf>,
}

impl JevClient {
    /// `cache_dir = None` disables the response cache entirely.
    pub fn new(key: String, cache_dir: Option<PathBuf>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .read_timeout(READ_TIMEOUT)
                .build()
                .expect("TLS backend or system resolver failed to initialize"),
            key,
            cache_dir,
        }
    }

    /// Cache location for a serialized request body; exposed so tests can seed
    /// entries without touching the network.
    pub fn cache_path(&self, body_json: &str) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        let hash = Sha256::digest(body_json.as_bytes());
        let mut hex = String::with_capacity(64);
        for byte in hash {
            hex.push_str(&format!("{byte:02x}"));
        }
        Some(dir.join(format!("{hex}.json")))
    }

    /// POST one System One request, serving byte-identical bodies from the
    /// cache and retrying only transient failures (up to 3 retries on transport
    /// errors, timeouts included, and 408/429/5xx).
    pub async fn system_one<S: Serialize>(&self, body: &JevRequest<S>) -> Result<JevResponse> {
        let body_json = serde_json::to_string(body)?;
        let cache_file = self.cache_path(&body_json);
        if let Some(path) = &cache_file {
            // A corrupt or unparsable entry falls through to a re-fetch that
            // overwrites it; the cache can only ever add hits, never failures.
            if let Ok(mut file) = std::fs::File::open(path) {
                let mut text = String::new();
                if file.read_to_string(&mut text).is_ok() {
                    if let Ok(cached) = serde_json::from_str(&text) {
                        mark_used(&file);
                        return Ok(cached);
                    }
                }
            }
        }
        // Debug-gated request accounting for cost benchmarking: one stderr line
        // per NETWORK request (cache hits excluded) lets a harness count billed
        // volume without touching normal output.
        if std::env::var_os("JEVR_DEBUG").is_some() {
            eprintln!("jevr: debug: jev request");
        }
        // Offline replay: a miss fails here instead of reaching the network, so
        // replaying a benchmark cache is free and the debug line above counts
        // what a live run would bill.
        if std::env::var_os("JEVR_OFFLINE").is_some() {
            bail!("not in the response cache (JEVR_OFFLINE is set)");
        }
        let mut last = String::new();
        for retry in 0..=MAX_RETRIES {
            match self
                .http
                .post(API_URL)
                .bearer_auth(&self.key)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body_json.clone())
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    let text = resp.text().await?;
                    let parsed: JevResponse = serde_json::from_str(&text)?;
                    if let Some(path) = &cache_file {
                        write_cache(path, &text);
                    }
                    return Ok(parsed);
                }
                Ok(resp) => {
                    let status = resp.status();
                    let detail: String = resp
                        .text()
                        .await
                        .unwrap_or_default()
                        .chars()
                        .take(500)
                        .collect();
                    last = format!("HTTP {status}: {detail}");
                    let retryable = status.as_u16() == 408
                        || status.as_u16() == 429
                        || status.is_server_error();
                    if !retryable {
                        bail!(last);
                    }
                }
                Err(e) => last = e.to_string(),
            }
            if retry < MAX_RETRIES {
                tokio::time::sleep(retry_delay(retry)).await;
            }
        }
        bail!("request failed after {} attempts: {last}", MAX_RETRIES + 1)
    }
}

/// Best-effort atomic write (temp + rename); a failed write only costs a future
/// cache miss, so every error is deliberately swallowed.
fn write_cache(path: &std::path::Path, text: &str) {
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Best-effort refresh of a hit entry's mtime, at most once a day, so mtime
/// means last use and an age-based prune (docs/GUIDE.md) evicts only unused
/// entries. Skipping fresh entries keeps repeat hits free of a metadata write
/// (~24 µs each); a failure (read-only cache, foreign owner) leaves mtime as is.
fn mark_used(file: &std::fs::File) {
    let now = SystemTime::now();
    let fresh = file
        .metadata()
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| {
            now.duration_since(modified)
                .is_ok_and(|age| age < Duration::from_secs(24 * 60 * 60))
        });
    if !fresh {
        let _ = file.set_modified(now);
    }
}

fn retry_delay(retry: u32) -> Duration {
    Duration::from_millis(250 << retry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_use_exponential_backoff() {
        assert_eq!(MAX_RETRIES, 3);
        assert_eq!(retry_delay(0), Duration::from_millis(250));
        assert_eq!(retry_delay(1), Duration::from_millis(500));
        assert_eq!(retry_delay(2), Duration::from_millis(1000));
    }

    #[test]
    fn cache_path_is_deterministic_and_off_without_dir() {
        let cached = JevClient::new("k".into(), Some(PathBuf::from("/tmp/c")));
        let a = cached.cache_path("{\"x\":1}").unwrap();
        let b = cached.cache_path("{\"x\":1}").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, cached.cache_path("{\"x\":2}").unwrap());
        assert!(JevClient::new("k".into(), None).cache_path("{}").is_none());
    }

    #[test]
    fn mark_used_refreshes_only_stale_entries() {
        let path = std::env::temp_dir().join(format!("jevr-mark-used-{}", std::process::id()));
        std::fs::write(&path, "{}").unwrap();
        let file = std::fs::File::options()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let stale = SystemTime::now() - Duration::from_secs(2 * 24 * 60 * 60);
        file.set_modified(stale).unwrap();
        mark_used(&file);
        let touched = file.metadata().unwrap().modified().unwrap();
        assert!(touched > stale + Duration::from_secs(24 * 60 * 60));
        mark_used(&file);
        assert_eq!(file.metadata().unwrap().modified().unwrap(), touched);
        std::fs::remove_file(&path).unwrap();
    }
}
