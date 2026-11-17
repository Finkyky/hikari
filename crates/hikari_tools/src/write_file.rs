use futures::future::BoxFuture;
use hikari_llm::ToolSpec;
use hikari_protocol::event::ToolRisk;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::common::{inside_workspace, parse_args, resolve};
use crate::{Tool, ToolContext, ToolOutput};

pub struct WriteFile;

#[derive(Deserialize)]
struct Args {
    path: String,
    content: String,
}

impl Tool for WriteFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_file".into(),
            description:
                "Create or overwrite a file with the given content. Prefer edit_file for changes to existing files."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path, relative to the workspace root."},
                    "content": {"type": "string", "description": "Full content to write."},
                },
                "required": ["path", "content"],
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
            match tokio::task::spawn_blocking(move || write(&path, &args.content)).await {
                Ok(out) => out,
                Err(e) => ToolOutput::Err(format!("write task failed: {e}")),
            }
        })
    }
}

fn write(path: &std::path::Path, content: &str) -> ToolOutput {
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return ToolOutput::Err(format!("cannot create {}: {e}", parent.display()));
    }
    match std::fs::write(path, content) {
        Ok(()) => ToolOutput::Ok(format!("wrote {}", path.display())),
        Err(e) => ToolOutput::Err(format!("cannot write {}: {e}", path.display())),
    }
}

#[cfg(test)]
#[path = "write_file_tests.rs"]
mod tests;
