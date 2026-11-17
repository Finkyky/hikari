use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::future::BoxFuture;
use hikari_llm::ToolSpec;
use hikari_protocol::event::ToolRisk;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::task::JoinHandle;

use crate::common::parse_args;
use crate::{Tool, ToolContext, ToolOutput};

const DEFAULT_TIMEOUT: u64 = 120;
const MAX_TIMEOUT: u64 = 600;
/// Characters kept from the head and from the tail of each output stream.
const KEEP_CHARS: usize = 15_000;
/// How long to keep draining output after the shell process ends.
const READER_GRACE: Duration = Duration::from_secs(2);

pub struct Shell;

#[derive(Deserialize)]
struct Args {
    command: String,
    timeout_secs: Option<u64>,
}

impl Tool for Shell {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "shell".into(),
            description:
                "Run a shell command in the workspace root and return its exit code and output."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Command line passed to the shell."},
                    "timeout_secs": {"type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT, "description": "Kill after this many seconds (default 120, max 600)."},
                },
                "required": ["command"],
            }),
        }
    }

    fn risk(&self, _args: &Value, _ctx: &ToolContext) -> ToolRisk {
        ToolRisk::Dangerous
    }

    fn run<'a>(&'a self, args: Value, ctx: &'a ToolContext) -> BoxFuture<'a, ToolOutput> {
        Box::pin(async move {
            let args: Args = match parse_args(&args) {
                Ok(a) => a,
                Err(e) => return e,
            };
            run_shell(&args, ctx).await
        })
    }
}

fn shell_program() -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        if on_path("pwsh.exe") || on_path("pwsh") {
            ("pwsh".into(), vec!["-NoProfile".into(), "-Command".into()])
        } else {
            (
                "powershell.exe".into(),
                vec!["-NoProfile".into(), "-Command".into()],
            )
        }
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        (shell, vec!["-c".into()])
    }
}

#[cfg(windows)]
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
}

/// Keeps the first and last `KEEP_CHARS` bytes of a stream; never holds it all.
struct Bounded {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    truncated: bool,
}

impl Bounded {
    fn new() -> Self {
        Self {
            head: Vec::new(),
            tail: VecDeque::new(),
            truncated: false,
        }
    }

    fn push(&mut self, mut chunk: &[u8]) {
        if self.head.len() < KEEP_CHARS {
            let take = (KEEP_CHARS - self.head.len()).min(chunk.len());
            self.head.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
        }
        for &b in chunk {
            self.tail.push_back(b);
            if self.tail.len() > KEEP_CHARS {
                self.tail.pop_front();
                self.truncated = true;
            }
        }
    }

    fn into_string(self) -> String {
        let Self {
            head,
            mut tail,
            truncated,
        } = self;
        let mut s = String::from_utf8_lossy(&head).into_owned();
        if truncated {
            s.push_str("\n\u{2026}[output truncated]\u{2026}\n");
        }
        s.push_str(&String::from_utf8_lossy(tail.make_contiguous()));
        s
    }
}

type Shared = Arc<Mutex<Bounded>>;

fn lock(shared: &Shared) -> MutexGuard<'_, Bounded> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Readers write into shared buffers so output collected so far survives if the reader task is
/// aborted while a grandchild process still holds the pipe open.
fn spawn_reader(
    reader: Option<impl tokio::io::AsyncRead + Unpin + Send + 'static>,
) -> (Shared, JoinHandle<()>) {
    let shared = Arc::new(Mutex::new(Bounded::new()));
    let sink = shared.clone();
    let task = tokio::spawn(async move {
        let Some(mut reader) = reader else { return };
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => lock(&sink).push(&buf[..n]),
            }
        }
    });
    (shared, task)
}

fn take(shared: &Shared) -> String {
    std::mem::replace(&mut *lock(shared), Bounded::new()).into_string()
}

async fn run_shell(args: &Args, ctx: &ToolContext) -> ToolOutput {
    let (program, shell_args) = shell_program();
    let timeout = Duration::from_secs(
        args.timeout_secs
            .unwrap_or(DEFAULT_TIMEOUT)
            .min(MAX_TIMEOUT),
    );
    let mut command = hikari_sandbox::Command::new(&program);
    command
        .args(&shell_args)
        .arg(&args.command)
        .current_dir(&ctx.workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return ToolOutput::Err(format!("failed to spawn {program}: {e}")),
    };
    let (stdout, mut out_task) = spawn_reader(child.stdout.take());
    let (stderr, mut err_task) = spawn_reader(child.stderr.take());

    enum End {
        Exit(i32),
        Timeout,
        Cancelled,
        WaitFailed(String),
    }
    let end = tokio::select! {
        _ = ctx.cancel.cancelled() => {
            let _ = child.kill().await;
            End::Cancelled
        }
        r = tokio::time::timeout(timeout, child.wait()) => match r {
            Ok(Ok(status)) => End::Exit(status.code().unwrap_or(-1)),
            Ok(Err(e)) => End::WaitFailed(e.to_string()),
            Err(_) => {
                let _ = child.kill().await;
                End::Timeout
            }
        },
    };
    // Only the direct child is killed, so a grandchild may keep the pipes open indefinitely.
    let _ = tokio::time::timeout(READER_GRACE, async {
        let _ = (&mut out_task).await;
        let _ = (&mut err_task).await;
    })
    .await;
    out_task.abort();
    err_task.abort();
    let combined = combine(&take(&stdout), &take(&stderr));

    match end {
        End::Exit(code) => ToolOutput::Ok(format!("exit code: {code}\n{combined}")),
        End::Timeout => ToolOutput::Err(format!(
            "command timed out after {} seconds and was killed\n{combined}",
            timeout.as_secs()
        )),
        End::Cancelled => ToolOutput::Err(format!("command cancelled\n{combined}")),
        End::WaitFailed(e) => {
            ToolOutput::Err(format!("failed to wait for command: {e}\n{combined}"))
        }
    }
}

fn combine(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.to_string(),
        (true, false) => format!("stderr:\n{stderr}"),
        (false, false) => format!("{stdout}stderr:\n{stderr}"),
    }
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
