#![allow(clippy::unwrap_used)]

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::edit_file::EditFile;
use crate::{Tool, ToolContext, ToolOutput};

fn ctx(dir: &std::path::Path) -> ToolContext {
    ToolContext {
        workspace: dir.to_path_buf(),
        cancel: CancellationToken::new(),
    }
}

async fn run(args: serde_json::Value, dir: &std::path::Path) -> ToolOutput {
    EditFile.run(args, &ctx(dir)).await
}

#[tokio::test]
async fn replaces_unique_string() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "hello world").unwrap();
    let out = run(
        json!({"path": "f.txt", "old_string": "world", "new_string": "there"}),
        dir.path(),
    )
    .await;
    assert!(out.ok());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "hello there"
    );
}

#[tokio::test]
async fn missing_old_string_fails_without_modifying() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "abc").unwrap();
    let ToolOutput::Err(e) = run(
        json!({"path": "f.txt", "old_string": "zzz", "new_string": "q"}),
        dir.path(),
    )
    .await
    else {
        panic!()
    };
    assert!(e.contains("not found"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "abc"
    );
}

#[tokio::test]
async fn ambiguous_match_requires_replace_all() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "a a a").unwrap();
    let ToolOutput::Err(e) = run(
        json!({"path": "f.txt", "old_string": "a", "new_string": "b"}),
        dir.path(),
    )
    .await
    else {
        panic!()
    };
    assert!(e.contains("3 times"));
    let out = run(
        json!({"path": "f.txt", "old_string": "a", "new_string": "b", "replace_all": true}),
        dir.path(),
    )
    .await;
    assert!(out.ok());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "b b b"
    );
}

#[tokio::test]
async fn equal_strings_fail() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "same").unwrap();
    let ToolOutput::Err(e) = run(
        json!({"path": "f.txt", "old_string": "same", "new_string": "same"}),
        dir.path(),
    )
    .await
    else {
        panic!()
    };
    assert!(e.contains("identical"));
}

#[tokio::test]
async fn crlf_file_keeps_crlf() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "one\r\ntwo\r\nthree\r\n").unwrap();
    let out = run(
        json!({"path": "f.txt", "old_string": "one\ntwo", "new_string": "ONE\nTWO"}),
        dir.path(),
    )
    .await;
    assert!(out.ok());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "ONE\r\nTWO\r\nthree\r\n"
    );
}
