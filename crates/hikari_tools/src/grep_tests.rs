#![allow(clippy::unwrap_used)]

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::grep::Grep;
use crate::{Tool, ToolContext, ToolOutput};

fn ctx(dir: &std::path::Path) -> ToolContext {
    ToolContext {
        workspace: dir.to_path_buf(),
        cancel: CancellationToken::new(),
    }
}

async fn run(args: serde_json::Value, dir: &std::path::Path) -> ToolOutput {
    Grep.run(args, &ctx(dir)).await
}

#[tokio::test]
async fn finds_matches_with_line_numbers() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "foo\nbar\nfoo again\n").unwrap();
    let ToolOutput::Ok(out) = run(json!({"pattern": "foo"}), dir.path()).await else {
        panic!()
    };
    assert_eq!(out, "a.txt:1:foo\na.txt:3:foo again\n");
}

#[tokio::test]
async fn respects_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join(".gitignore"), "skipped.txt\n").unwrap();
    std::fs::write(dir.path().join("skipped.txt"), "hit\n").unwrap();
    std::fs::write(dir.path().join("kept.txt"), "hit\n").unwrap();
    let ToolOutput::Ok(out) = run(json!({"pattern": "hit"}), dir.path()).await else {
        panic!()
    };
    assert_eq!(out, "kept.txt:1:hit\n");
}

#[tokio::test]
async fn glob_filter_and_case_insensitive() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "HELLO\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "hello\n").unwrap();
    let ToolOutput::Ok(out) = run(
        json!({"pattern": "hello", "glob": "*.rs", "case_insensitive": true}),
        dir.path(),
    )
    .await
    else {
        panic!()
    };
    assert_eq!(out, "a.rs:1:HELLO\n");
}

#[tokio::test]
async fn caps_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "x\nx\nx\nx\nx\n").unwrap();
    let ToolOutput::Ok(out) = run(json!({"pattern": "x", "limit": 2}), dir.path()).await else {
        panic!()
    };
    assert!(out.contains("truncated at 2"));
}

#[tokio::test]
async fn invalid_regex_is_error_result() {
    let dir = tempfile::tempdir().unwrap();
    let ToolOutput::Err(e) = run(json!({"pattern": "(*"}), dir.path()).await else {
        panic!()
    };
    assert!(e.contains("invalid regex"));
}
