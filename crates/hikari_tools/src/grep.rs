use std::path::{Path, PathBuf};

use futures::future::BoxFuture;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkMatch};
use hikari_llm::ToolSpec;
use hikari_protocol::event::ToolRisk;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::common::{parse_args, resolve, walker};
use crate::{Tool, ToolContext, ToolOutput};

const DEFAULT_LIMIT: usize = 200;

pub struct Grep;

#[derive(Deserialize)]
struct Args {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    case_insensitive: Option<bool>,
    limit: Option<usize>,
}

impl Tool for Grep {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "grep".into(),
            description:
                "Search file contents with a regular expression. Returns path:line:text matches. Respects .gitignore."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regular expression to search for."},
                    "path": {"type": "string", "description": "File or directory to search (default \".\")."},
                    "glob": {"type": "string", "description": "Only search files matching this glob."},
                    "case_insensitive": {"type": "boolean", "description": "Case-insensitive matching."},
                    "limit": {"type": "integer", "minimum": 1, "description": "Max matches (default 200)."},
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
            match tokio::task::spawn_blocking(move || run_grep(&path, &args)).await {
                Ok(out) => out,
                Err(e) => ToolOutput::Err(format!("grep task failed: {e}")),
            }
        })
    }
}

struct Collect {
    root: PathBuf,
    current: PathBuf,
    matches: Vec<String>,
    limit: usize,
}

impl Sink for Collect {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        let rel = self
            .current
            .strip_prefix(&self.root)
            .unwrap_or(&self.current);
        let rel = rel.to_string_lossy().replace('\\', "/");
        let line = String::from_utf8_lossy(mat.bytes());
        let line = line.trim_end_matches(['\r', '\n']);
        let number = mat.line_number().unwrap_or(0);
        self.matches.push(format!("{rel}:{number}:{line}"));
        Ok(self.matches.len() < self.limit)
    }
}

fn run_grep(root: &Path, args: &Args) -> ToolOutput {
    let matcher = match RegexMatcherBuilder::new()
        .case_insensitive(args.case_insensitive.unwrap_or(false))
        .build(&args.pattern)
    {
        Ok(m) => m,
        Err(e) => return ToolOutput::Err(format!("invalid regex: {e}")),
    };
    let glob = match &args.glob {
        Some(g) => match globset::Glob::new(g) {
            Ok(g) => Some(g.compile_matcher()),
            Err(e) => return ToolOutput::Err(format!("invalid glob filter: {e}")),
        },
        None => None,
    };
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT);
    // Collect one extra match so an exact-limit result isn't falsely "truncated".
    let mut sink = Collect {
        root: root.to_path_buf(),
        current: PathBuf::new(),
        matches: Vec::new(),
        limit: limit + 1,
    };
    let mut truncated = false;

    let search = |path: &Path, sink: &mut Collect| -> Result<(), Box<dyn std::error::Error>> {
        sink.current = path.to_path_buf();
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_path(&matcher, path, &mut *sink)?;
        Ok(())
    };

    if root.is_file() {
        let _ = search(root, &mut sink);
    } else {
        for entry in walker(root) {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if let Some(g) = &glob
                && !g.is_match(&rel_str)
            {
                continue;
            }
            if search(entry.path(), &mut sink).is_err() {
                continue;
            }
            if sink.matches.len() > limit {
                break;
            }
        }
    }
    if sink.matches.len() > limit {
        truncated = true;
        sink.matches.truncate(limit);
    }
    let mut out = sink.matches.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    if truncated {
        out.push_str(&format!("(truncated at {limit} matches)\n"));
    }
    if out.is_empty() {
        out.push_str("(no matches)\n");
    }
    ToolOutput::Ok(out)
}

#[cfg(test)]
#[path = "grep_tests.rs"]
mod tests;
