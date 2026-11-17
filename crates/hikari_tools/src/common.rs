use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::ToolOutput;

/// Resolve a possibly-relative tool path against the workspace root.
pub fn resolve(workspace: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    }
}

/// Canonicalize a path that may not exist yet by walking up to the nearest
/// existing ancestor and re-appending the remaining components.
fn canonicalize_lenient(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    while !existing.exists() {
        match existing.file_name() {
            Some(name) => {
                rest.push(name.to_os_string());
                existing.pop();
            }
            None => break,
        }
    }
    let mut out = existing.canonicalize().unwrap_or(existing);
    for comp in rest.iter().rev() {
        out.push(comp);
    }
    out
}

/// Whether `target` resolves inside the workspace (symlinks honored).
pub fn inside_workspace(workspace: &Path, target: &Path) -> bool {
    canonicalize_lenient(target).starts_with(canonicalize_lenient(workspace))
}

/// A directory walker honoring `.gitignore`, `.ignore`, and hidden-file rules
/// the same way ripgrep does by default.
pub fn walker(root: &Path) -> ignore::Walk {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(true)
        .ignore(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true);
    builder.build()
}

/// Parse tool call arguments; bad JSON or schema mismatches become error results.
pub fn parse_args<T: DeserializeOwned>(args: &Value) -> Result<T, ToolOutput> {
    serde_json::from_value(args.clone())
        .map_err(|e| ToolOutput::Err(format!("invalid arguments: {e}")))
}
