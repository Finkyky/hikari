use futures::future::BoxFuture;
use globset::Glob as GlobPattern;
use hikari_llm::ToolSpec;
use hikari_protocol::event::ToolRisk;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::common::{parse_args, resolve};
use crate::{Tool, ToolContext, ToolOutput};

const DEFAULT_LIMIT: usize = 200;

pub struct Glob;

#[derive(Deserialize)]
struct Args {
    pattern: String,
    path: Option<String>,
    limit: Option<usize>,
}

impl Tool for Glob {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "glob".into(),
            description:
                "Find files whose paths match a glob pattern such as \"src/**/*.rs\". Respects .gitignore."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Glob pattern matched against paths relative to the search root."},
                    "path": {"type": "string", "description": "Directory to search (default \".\")."},
                    "limit": {"type": "integer", "minimum": 1, "description": "Max results (default 200)."},
                },
                "required": ["pattern"],
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
            match tokio::task::spawn_blocking(move || run_glob(&path, &args)).await {
                Ok(out) => out,
                Err(e) => ToolOutput::Err(format!("glob task failed: {e}")),
            }
        })
    }
}

fn run_glob(root: &std::path::Path, args: &Args) -> ToolOutput {
    let glob = match GlobPattern::new(&args.pattern) {
        Ok(g) => g.compile_matcher(),
        Err(e) => return ToolOutput::Err(format!("invalid glob pattern: {e}")),
    };
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT);
    let mut matches = Vec::new();
    let mut truncated = false;
    for entry in crate::common::walker(root) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let rel = rel.to_string_lossy().replace('\\', "/");
        if glob.is_match(&rel) {
            matches.push(rel);
            if matches.len() > limit {
                truncated = true;
                matches.pop();
                break;
            }
        }
    }
    matches.sort();
    let mut out = matches.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    if truncated {
        out.push_str(&format!("(truncated at {limit} results)\n"));
    }
    if out.is_empty() {
        out.push_str("(no matches)\n");
    }
    ToolOutput::Ok(out)
}

#[cfg(test)]
#[path = "glob_tests.rs"]
mod tests;
