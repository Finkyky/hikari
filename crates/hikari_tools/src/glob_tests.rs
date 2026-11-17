#![allow(clippy::unwrap_used)]

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::glob::Glob;
use crate::{Tool, ToolContext, ToolOutput};

fn ctx(dir: &std::path::Path) -> ToolContext {
    ToolContext {
        workspace: dir.to_path_buf(),
        cancel: CancellationToken::new(),
    }
}

async fn run(args: serde_json::Value, dir: &std::path::Path) -> ToolOutput {
    Glob.run(args, &ctx(dir)).await
}

fn init_git(dir: &std::path::Path) {
    std::fs::create_dir(dir.join(".git")).unwrap();
}

#[tokio::test]
async fn finds_matching_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/a.rs"), "").unwrap();
    std::fs::write(dir.path().join("src/b.txt"), "").unwrap();
    let ToolOutput::Ok(out) = run(json!({"pattern": "src/**/*.rs"}), dir.path()).await else {
        panic!()
    };
    assert_eq!(out, "src/a.rs\n");
}

#[tokio::test]
async fn respects_gitignore_and_hidden_files() {
    let dir = tempfile::tempdir().unwrap();
    init_git(dir.path());
    std::fs::write(dir.path().join(".gitignore"), "ignored/\n").unwrap();
    std::fs::create_dir(dir.path().join("ignored")).unwrap();
    std::fs::write(dir.path().join("ignored/x.rs"), "").unwrap();
    std::fs::write(dir.path().join(".hidden.rs"), "").unwrap();
    std::fs::write(dir.path().join("seen.rs"), "").unwrap();
    let ToolOutput::Ok(out) = run(json!({"pattern": "**/*.rs"}), dir.path()).await else {
        panic!()
    };
    assert_eq!(out, "seen.rs\n");
}

#[tokio::test]
async fn caps_results_with_note() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..5 {
        std::fs::write(dir.path().join(format!("f{i}.txt")), "").unwrap();
    }
    let ToolOutput::Ok(out) = run(json!({"pattern": "*.txt", "limit": 3}), dir.path()).await else {
        panic!()
    };
    assert_eq!(out.lines().count(), 4);
    assert!(out.contains("truncated at 3"));
}
