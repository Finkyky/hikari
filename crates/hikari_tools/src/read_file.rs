use futures::future::BoxFuture;
use hikari_llm::ToolSpec;
use hikari_protocol::event::ToolRisk;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::common::{parse_args, resolve};
use crate::{Tool, ToolContext, ToolOutput};

const DEFAULT_LIMIT: usize = 2000;
const BINARY_CHECK_BYTES: usize = 8192;
const MAX_LINE_CHARS: usize = 2000;

pub struct ReadFile;

#[derive(Deserialize)]
struct Args {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

impl Tool for ReadFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_file".into(),
            description:
                "Read a text file. Returns lines prefixed with line numbers. Use offset and limit for large files."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path, relative to the workspace root."},
                    "offset": {"type": "integer", "minimum": 1, "description": "1-based first line to return."},
                    "limit": {"type": "integer", "minimum": 1, "description": "Max lines to return (default 2000)."},
                },
                "required": ["path"],
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
            let path = resolve(&ctx.workspace, &args.path);
            tokio::task::spawn_blocking(move || read(&path, &args))
                .await
                .unwrap_or_else(|e| ToolOutput::Err(format!("read task failed: {e}")))
        })
    }
}

fn read(path: &std::path::Path, args: &Args) -> ToolOutput {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return ToolOutput::Err(format!("cannot read {}: {e}", path.display())),
    };
    if bytes.iter().take(BINARY_CHECK_BYTES).any(|b| *b == 0) {
        return ToolOutput::Err(format!("{} is a binary file", path.display()));
    }
    let text = String::from_utf8_lossy(&bytes);
    let offset = args.offset.unwrap_or(1).max(1);
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT);
    let mut out = String::new();
    for (i, line) in text.lines().enumerate().skip(offset - 1).take(limit) {
        let line: String = line.chars().take(MAX_LINE_CHARS).collect();
        out.push_str(&format!("{}: {line}\n", i + 1));
    }
    if out.is_empty() {
        out.push_str("(no lines in range)\n");
    }
    ToolOutput::Ok(out)
}

#[cfg(test)]
#[path = "read_file_tests.rs"]
mod tests;
