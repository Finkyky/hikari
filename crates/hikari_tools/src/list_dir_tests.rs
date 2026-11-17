#![allow(clippy::unwrap_used)]

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::list_dir::ListDir;
use crate::{Tool, ToolContext, ToolOutput};

fn ctx(dir: &std::path::Path) -> ToolContext {
    ToolContext {
        workspace: dir.to_path_buf(),
        cancel: CancellationToken::new(),
    }
}

#[tokio::test]
async fn lists_entries_with_dir_slashes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.txt"), "").unwrap();
    std::fs::create_dir(dir.path().join("adir")).unwrap();
    let ToolOutput::Ok(out) = ListDir.run(json!({}), &ctx(dir.path())).await else {
        panic!()
    };
    assert_eq!(out, "adir/\nb.txt\n");
}

#[tokio::test]
async fn missing_dir_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let ToolOutput::Err(e) = ListDir.run(json!({"path": "nope"}), &ctx(dir.path())).await else {
        panic!()
    };
    assert!(e.contains("cannot list"));
}
