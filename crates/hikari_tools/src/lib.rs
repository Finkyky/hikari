//! Built-in tools and tool-call risk classification.

mod common;
mod edit_file;
mod glob;
mod grep;
mod list_dir;
mod read_file;
mod shell;
mod write_file;

use std::collections::BTreeMap;
use std::path::PathBuf;

use futures::future::BoxFuture;
use hikari_llm::ToolSpec;
use hikari_protocol::event::ToolRisk;
use tokio_util::sync::CancellationToken;

/// Context every tool call runs against.
pub struct ToolContext {
    pub workspace: PathBuf,
    pub cancel: CancellationToken,
}

/// The result sent back to the model. Tool failures are `Err` results, never aborts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolOutput {
    Ok(String),
    Err(String),
}

impl ToolOutput {
    pub fn ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn into_text(self) -> String {
        match self {
            Self::Ok(s) | Self::Err(s) => s,
        }
    }
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn risk(&self, args: &serde_json::Value, ctx: &ToolContext) -> ToolRisk;
    fn run<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> BoxFuture<'a, ToolOutput>;
}

/// Name → tool lookup plus the advertised specs.
pub struct ToolRegistry {
    tools: BTreeMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn builtin() -> Self {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(read_file::ReadFile),
            Box::new(write_file::WriteFile),
            Box::new(edit_file::EditFile),
            Box::new(list_dir::ListDir),
            Box::new(glob::Glob),
            Box::new(grep::Grep),
            Box::new(shell::Shell),
        ];
        Self {
            tools: tools.into_iter().map(|t| (t.spec().name, t)).collect(),
        }
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(AsRef::as_ref)
    }
}

/// Cap a tool result at `max_chars`, keeping the head and tail with a marker.
pub fn cap_output(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    let head_len = max_chars / 2;
    let tail_len = max_chars - head_len;
    let head: String = s.chars().take(head_len).collect();
    let tail: String = s.chars().skip(total - tail_len).collect();
    let skipped = total - head_len - tail_len;
    format!("{head}\n\u{2026}[{skipped} characters truncated]\u{2026}\n{tail}")
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
