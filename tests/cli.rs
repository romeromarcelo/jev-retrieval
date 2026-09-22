//! CLI contract tests: argument validation and error rendering.
//!
//! Both cases fail before any request is built, so the binary is exercised
//! without network or TYPESAFE_API_KEY. They pin the exit codes and stderr
//! wording that agents parse.

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
