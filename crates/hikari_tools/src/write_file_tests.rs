#![allow(clippy::unwrap_used)]

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::write_file::WriteFile;
use crate::{Tool, ToolContext};
use hikari_protocol::event::ToolRisk;

fn ctx(dir: &std::path::Path) -> ToolContext {
    ToolContext {
        workspace: dir.to_path_buf(),
        cancel: CancellationToken::new(),
    }
}

#[tokio::test]
async fn creates_parent_dirs_and_writes() {
    let dir = tempfile::tempdir().unwrap();
    let out = WriteFile
        .run(
            json!({"path": "sub/deep/f.txt", "content": "hello"}),
            &ctx(dir.path()),
        )
        .await;
    assert!(out.ok());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("sub/deep/f.txt")).unwrap(),
        "hello"
    );
}

#[tokio::test]
async fn overwrites_existing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "old").unwrap();
    let out = WriteFile
        .run(json!({"path": "f.txt", "content": "new"}), &ctx(dir.path()))
        .await;
    assert!(out.ok());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "new"
    );
}

#[test]
fn risk_inside_vs_outside() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ctx(dir.path());
    assert_eq!(
        WriteFile.risk(&json!({"path": "a.txt", "content": ""}), &ctx),
        ToolRisk::WorkspaceWrite
    );
    assert_eq!(
        WriteFile.risk(&json!({"path": "../out.txt", "content": ""}), &ctx),
        ToolRisk::Dangerous
    );
    assert_eq!(WriteFile.risk(&json!({}), &ctx), ToolRisk::Dangerous);
}
