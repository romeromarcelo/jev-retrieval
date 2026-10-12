//! CLI contract tests: argument validation, error rendering and offline replay.
//!
//! No case reaches the network: two fail before any request is built, and the
//! offline case fails its one request at the cache. They pin the exit codes
//! and stderr wording that agents and the benchmark harness parse.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jevr"))
}

#[test]
fn blank_query_is_a_usage_error() {
    let out = bin().args(["   ", "."]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("query is empty"), "stderr: {stderr}");
}

#[test]
fn missing_path_reports_the_path_once_with_exit_2() {
    let out = bin()
        .args(["where is anything?", "/nonexistent/dir"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.matches("/nonexistent/dir").count(),
        1,
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("no such file or directory"),
        "stderr: {stderr}"
    );
}

#[test]
fn offline_cache_miss_is_counted_and_never_sent() {
    let dir = std::env::temp_dir().join(format!("jevr-offline-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("corpus")).unwrap();
    std::fs::write(dir.join("corpus/a.md"), "The user is greeted by name.\n").unwrap();
    let out = bin()
        .args(["where is the user greeted?", "corpus"])
        .current_dir(&dir)
        .env("TYPESAFE_API_KEY", "offline")
        .env("JEVR_OFFLINE", "1")
        .env("JEVR_DEBUG", "1")
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .output()
        .unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "stderr: {stderr}");
    assert_eq!(
        stderr.matches("jevr: debug: jev request").count(),
        1,
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("not in the response cache"),
        "stderr: {stderr}"
    );
}
