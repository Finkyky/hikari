use futures::future::BoxFuture;
use hikari_llm::ToolSpec;
use hikari_protocol::event::ToolRisk;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::common::{parse_args, resolve};
use crate::{Tool, ToolContext, ToolOutput};

pub struct ListDir;

#[derive(Deserialize)]
struct Args {
    path: Option<String>,
}

impl Tool for ListDir {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "list_dir".into(),
            description: "List entries of a directory. Directories end with a slash.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path (default \".\")."},
                },
            }),
        }
    }

    fn risk(&self, _args: &Value, _ctx: &ToolContext) -> ToolRisk {
        ToolRisk::ReadOnly
    }

    fn run<'a>(&'a self, args: Value, ctx: &'a ToolContext) -> BoxFuture<'a, ToolOutput> {
        Box::pin(async move {
            let args: Args = match parse_args(&args) {
                Ok(a) => a,
                Err(e) => return e,
            };
            let path = resolve(&ctx.workspace, args.path.as_deref().unwrap_or("."));
            match tokio::task::spawn_blocking(move || list(&path)).await {
                Ok(out) => out,
                Err(e) => ToolOutput::Err(format!("list task failed: {e}")),
            }
        })
    }
}

fn list(path: &std::path::Path) -> ToolOutput {
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => return ToolOutput::Err(format!("cannot list {}: {e}", path.display())),
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            let mut name = e.file_name().to_string_lossy().into_owned();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                name.push('/');
            }
            Some(name)
        })
        .collect();
    names.sort();
    if names.is_empty() {
        return ToolOutput::Ok("(empty directory)\n".into());
    }
    ToolOutput::Ok(names.join("\n") + "\n")
}

#[cfg(test)]
#[path = "list_dir_tests.rs"]
mod tests;
