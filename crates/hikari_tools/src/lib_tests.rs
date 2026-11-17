#![allow(clippy::unwrap_used)]

use crate::{ToolRegistry, cap_output};

#[test]
fn cap_output_short_passthrough() {
    assert_eq!(cap_output("hello", 10), "hello");
    assert_eq!(cap_output("hello", 5), "hello");
}

#[test]
fn cap_output_keeps_head_and_tail() {
    let s: String = (0..100)
        .map(|i| char::from(b'a' + (i % 26) as u8))
        .collect();
    let capped = cap_output(&s, 20);
    assert!(capped.contains("80 characters truncated"));
    assert!(capped.starts_with(&s[..10]));
    assert!(capped.ends_with(&s[90..]));
}

#[test]
fn registry_has_all_builtins() {
    let registry = ToolRegistry::builtin();
    for name in [
        "read_file",
        "write_file",
        "edit_file",
        "list_dir",
        "glob",
        "grep",
        "shell",
    ] {
        assert!(registry.get(name).is_some(), "missing {name}");
    }
    assert_eq!(registry.specs().len(), 7);
}
