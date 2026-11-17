#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::shell::Shell;
use crate::{Tool, ToolContext, ToolOutput};

fn ctx(dir: &std::path::Path) -> ToolContext {
    ToolContext {
        workspace: dir.to_path_buf(),
        cancel: CancellationToken::new(),
    }
}

async fn run(args: serde_json::Value, dir: &std::path::Path) -> ToolOutput {
    Shell.run(args, &ctx(dir)).await
}

#[tokio::test]
async fn reports_exit_code_and_output() {
    let dir = tempfile::tempdir().unwrap();
    let ToolOutput::Ok(out) = run(json!({"command": "echo hello"}), dir.path()).await else {
        panic!()
    };
    assert!(out.starts_with("exit code: 0"));
    assert!(out.contains("hello"));
}

#[tokio::test]
async fn nonzero_exit_reported_not_error() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let cmd = "exit 3";
    #[cfg(not(windows))]
    let cmd = "exit 3";
    let ToolOutput::Ok(out) = run(json!({"command": cmd}), dir.path()).await else {
        panic!()
    };
    assert!(out.contains("exit code: 3"));
}

#[tokio::test]
async fn timeout_kills_process() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let cmd = "Start-Sleep -Seconds 30";
    #[cfg(not(windows))]
    let cmd = "sleep 30";
    let start = Instant::now();
    let ToolOutput::Err(e) = run(json!({"command": cmd, "timeout_secs": 1}), dir.path()).await
    else {
        panic!()
    };
    assert!(start.elapsed() < Duration::from_secs(15));
    assert!(e.contains("timed out"));
}

#[tokio::test]
async fn cancel_kills_process() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let ctx = ToolContext {
        workspace: dir.path().to_path_buf(),
        cancel: cancel.clone(),
    };
    #[cfg(windows)]
    let cmd = "Start-Sleep -Seconds 30";
    #[cfg(not(windows))]
    let cmd = "sleep 30";
    let start = Instant::now();
    let task = tokio::spawn(async move { Shell.run(json!({"command": cmd}), &ctx).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel.cancel();
    let ToolOutput::Err(e) = task.await.unwrap() else {
        panic!()
    };
    assert!(start.elapsed() < Duration::from_secs(15));
    assert!(e.contains("cancelled"));
}

#[cfg(unix)]
#[tokio::test]
async fn background_grandchild_does_not_block_result() {
    let dir = tempfile::tempdir().unwrap();
    let start = std::time::Instant::now();
    let out = run(
        serde_json::json!({"command": "sleep 30 & echo started"}),
        dir.path(),
    )
    .await;
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
    assert!(out.ok());
    assert!(out.into_text().contains("started"));
}

#[cfg(windows)]
#[tokio::test]
async fn background_grandchild_does_not_block_result() {
    let dir = tempfile::tempdir().unwrap();
    let start = std::time::Instant::now();
    let out = run(
        serde_json::json!({"command": "cmd /c 'start /b ping -n 30 127.0.0.1'; echo started"}),
        dir.path(),
    )
    .await;
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
    assert!(out.ok());
    assert!(out.into_text().contains("started"));
}
