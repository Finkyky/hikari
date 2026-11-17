use futures::future::BoxFuture;
use hikari_llm::ToolSpec;
use hikari_protocol::event::ToolRisk;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::common::{inside_workspace, parse_args, resolve};
use crate::{Tool, ToolContext, ToolOutput};

pub struct EditFile;

#[derive(Deserialize)]
struct Args {
    path: String,
    old_string: String,
    new_string: String,
    replace_all: Option<bool>,
}

impl Tool for EditFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "edit_file".into(),
            description:
                "Replace an exact string in a file. old_string must match exactly once unless replace_all is true. Read the file first."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path, relative to the workspace root."},
                    "old_string": {"type": "string", "description": "Exact text to replace."},
                    "new_string": {"type": "string", "description": "Replacement text."},
                    "replace_all": {"type": "boolean", "description": "Replace every occurrence."},
                },
                "required": ["path", "old_string", "new_string"],
            }),
        }
    }

    fn risk(&self, args: &Value, ctx: &ToolContext) -> ToolRisk {
        match args.get("path").and_then(Value::as_str) {
            Some(p) if inside_workspace(&ctx.workspace, &resolve(&ctx.workspace, p)) => {
                ToolRisk::WorkspaceWrite
            }
            _ => ToolRisk::Dangerous,
        }
    }

    fn run<'a>(&'a self, args: Value, ctx: &'a ToolContext) -> BoxFuture<'a, ToolOutput> {
        Box::pin(async move {
            let args: Args = match parse_args(&args) {
                Ok(a) => a,
                Err(e) => return e,
            };
            let path = resolve(&ctx.workspace, &args.path);
            match tokio::task::spawn_blocking(move || edit(&path, &args)).await {
                Ok(out) => out,
                Err(e) => ToolOutput::Err(format!("edit task failed: {e}")),
            }
        })
    }
}

fn edit(path: &std::path::Path, args: &Args) -> ToolOutput {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return ToolOutput::Err(format!("cannot read {}: {e}", path.display())),
    };
    // A file using CRLF keeps CRLF when the model supplies LF strings.
    let (old, new) = if content.contains("\r\n") && !args.old_string.contains('\r') {
        (
            args.old_string.replace('\n', "\r\n"),
            args.new_string.replace('\n', "\r\n"),
        )
    } else {
        (args.old_string.clone(), args.new_string.clone())
    };
    if old == new {
        return ToolOutput::Err("old_string and new_string are identical".into());
    }
    let matches = content.matches(&old).count();
    if matches == 0 {
        return ToolOutput::Err("old_string not found in file".into());
    }
    if matches > 1 && !args.replace_all.unwrap_or(false) {
        return ToolOutput::Err(format!(
            "old_string matches {matches} times; set replace_all to replace all"
        ));
    }
    let updated = content.replacen(&old, &new, if matches > 1 { usize::MAX } else { 1 });
    match std::fs::write(path, &updated) {
        Ok(()) => ToolOutput::Ok(format!("edited {}", path.display())),
        Err(e) => ToolOutput::Err(format!("cannot write {}: {e}", path.display())),
    }
}

#[cfg(test)]
#[path = "edit_file_tests.rs"]
mod tests;
