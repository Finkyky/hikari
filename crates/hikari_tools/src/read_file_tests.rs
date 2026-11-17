#![allow(clippy::unwrap_used)]

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::read_file::ReadFile;
use crate::{Tool, ToolContext, ToolOutput};

fn ctx(dir: &std::path::Path) -> ToolContext {
    ToolContext {
        workspace: dir.to_path_buf(),
        cancel: CancellationToken::new(),
    }
}

async fn run(args: serde_json::Value, dir: &std::path::Path) -> ToolOutput {
    ReadFile.run(args, &ctx(dir)).await
}

#[tokio::test]
async fn returns_numbered_lines() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    let ToolOutput::Ok(out) = run(json!({"path": "a.txt"}), dir.path()).await else {
        panic!()
    };
    assert_eq!(out, "1: one\n2: two\n3: three\n");
}

#[tokio::test]
async fn offset_and_limit() {
    let dir = tempfile::tempdir().unwrap();
    let body: String = (1..=10).map(|i| format!("line{i}\n")).collect();
    std::fs::write(dir.path().join("a.txt"), body).unwrap();
    let ToolOutput::Ok(out) = run(
        json!({"path": "a.txt", "offset": 3, "limit": 2}),
        dir.path(),
    )
    .await
    else {
        panic!()
    };
    assert_eq!(out, "3: line3\n4: line4\n");
}

#[tokio::test]
async fn truncates_long_lines() {
    let dir = tempfile::tempdir().unwrap();
    let long = "x".repeat(5000);
    std::fs::write(dir.path().join("a.txt"), &long).unwrap();
    let ToolOutput::Ok(out) = run(json!({"path": "a.txt"}), dir.path()).await else {
        panic!()
    };
    let line = out.lines().next().unwrap();
    assert_eq!(line.len(), "1: ".len() + 2000);
}

#[tokio::test]
async fn refuses_binary() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.bin"), b"\x00\x01\x02").unwrap();
    let ToolOutput::Err(e) = run(json!({"path": "b.bin"}), dir.path()).await else {
        panic!()
    };
    assert!(e.contains("binary"));
}

#[tokio::test]
async fn missing_file_is_error_result() {
    let dir = tempfile::tempdir().unwrap();
    let ToolOutput::Err(e) = run(json!({"path": "nope.txt"}), dir.path()).await else {
        panic!()
    };
    assert!(e.contains("cannot read"));
}

#[tokio::test]
async fn invalid_args_are_error_result() {
    let dir = tempfile::tempdir().unwrap();
    let ToolOutput::Err(e) = run(json!({"limit": "x"}), dir.path()).await else {
        panic!()
    };
    assert!(e.contains("invalid arguments"));
}
