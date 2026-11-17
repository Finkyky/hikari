//! Headless front-end: `hikari exec`.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use hikari_config::{Config, Overrides, ProviderConfig, ProviderKind};
use hikari_core::{ApprovalPolicy, SessionConfig, SessionHandle, ToolRegistry, spawn_session};
use hikari_llm::{ModelClient, OpenAiChatClient};
use hikari_protocol::event::{Event, EventMsg};
use hikari_protocol::op::{ApprovalDecision, Op};

pub struct ExecArgs {
    pub prompt: String,
    pub workspace: PathBuf,
    pub json: bool,
    pub full_auto: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// Resolve config and the provider client, then run the task.
pub async fn run(args: ExecArgs) -> ExitCode {
    let config = match hikari_config::load(Overrides {
        provider: args.provider.clone(),
        model: args.model.clone(),
    }) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    let (model, client) = match resolve_client(&config) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(1);
        }
    };
    run_with(&args, model, client, std::io::stdout(), std::io::stderr()).await
}

fn resolve_client(config: &Config) -> Result<(String, Arc<dyn ModelClient>), String> {
    let model = config.model.clone().ok_or_else(|| {
        "no model configured; set `model` in config.toml or pass --model".to_string()
    })?;
    let provider = config.providers.get(&config.provider).ok_or_else(|| {
        format!(
            "provider `{}` is not defined; check `provider` in config.toml or --provider",
            config.provider
        )
    })?;
    let client: Arc<dyn ModelClient> = match provider {
        ProviderConfig {
            kind: ProviderKind::OpenAiChat,
            base_url,
            api_key_env,
        } => {
            let key = match api_key_env {
                Some(env) => Some(std::env::var(env).map_err(|_| {
                    format!("environment variable {env} is not set; it holds the API key")
                })?),
                None => None,
            };
            Arc::new(OpenAiChatClient::new(base_url.clone(), key))
        }
    };
    Ok((model, client))
}

/// Testable body of `run`: event loop, rendering, and exit code.
async fn run_with(
    args: &ExecArgs,
    model: String,
    client: Arc<dyn ModelClient>,
    mut out: impl Write,
    mut err: impl Write,
) -> ExitCode {
    let config = SessionConfig {
        model,
        workspace: args.workspace.clone(),
        policy: if args.full_auto {
            ApprovalPolicy::Auto
        } else {
            ApprovalPolicy::Untrusted
        },
        max_round_trips: 100,
    };
    let mut handle = spawn_session(config, client, ToolRegistry::builtin());
    let _ = handle
        .ops
        .send(Op::UserInput {
            text: args.prompt.clone(),
        })
        .await;

    let interrupt_ops = handle.ops.clone();
    let sig = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = interrupt_ops.send(Op::Interrupt).await;
            let _ = tokio::signal::ctrl_c().await;
            std::process::exit(130);
        }
    });
    let code = drain_events(&mut handle, args.json, &mut out, &mut err).await;
    sig.abort();
    let _ = handle.ops.send(Op::Shutdown).await;
    code
}

async fn drain_events(
    handle: &mut SessionHandle,
    json: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> ExitCode {
    let mut code = ExitCode::SUCCESS;
    while let Some(event) = handle.events.recv().await {
        if json {
            let _ = writeln!(out, "{}", serde_json::to_string(&event).unwrap_or_default());
        } else {
            render(&event, out, err);
        }
        match &event.msg {
            EventMsg::ApprovalRequested { call_id } => {
                let _ = handle
                    .ops
                    .send(Op::Approval {
                        call_id: call_id.clone(),
                        decision: ApprovalDecision::Deny,
                    })
                    .await;
            }
            EventMsg::Error { .. } => {
                code = ExitCode::from(1);
                break;
            }
            EventMsg::TurnCompleted { .. } => break,
            EventMsg::TurnAborted { .. } => {
                code = ExitCode::from(130);
                break;
            }
            EventMsg::SessionStarted
            | EventMsg::TurnStarted { .. }
            | EventMsg::AgentMessageDelta { .. }
            | EventMsg::AgentMessage { .. }
            | EventMsg::ToolCallRequested { .. }
            | EventMsg::ToolCallDenied { .. }
            | EventMsg::ToolCallCompleted { .. } => {}
        }
    }
    code
}

fn render(event: &Event, out: &mut impl Write, err: &mut impl Write) {
    match &event.msg {
        EventMsg::AgentMessage { text, .. } => {
            let _ = writeln!(out, "{text}");
        }
        EventMsg::ToolCallRequested {
            tool, arguments, ..
        } => {
            let _ = writeln!(err, "> {tool} {}", summarize(arguments));
        }
        EventMsg::ToolCallCompleted { ok, output, .. } => {
            if *ok {
                let _ = writeln!(err, "  ok");
            } else {
                let first = output.lines().next().unwrap_or_default();
                let _ = writeln!(err, "  error: {first}");
            }
        }
        EventMsg::ToolCallDenied { .. } => {
            let _ = writeln!(err, "  denied");
        }
        EventMsg::Error { message } => {
            let _ = writeln!(err, "error: {message}");
        }
        EventMsg::SessionStarted
        | EventMsg::TurnStarted { .. }
        | EventMsg::TurnCompleted { .. }
        | EventMsg::TurnAborted { .. }
        | EventMsg::ApprovalRequested { .. }
        | EventMsg::AgentMessageDelta { .. } => {}
    }
}

/// One-line argument summary for progress output: the `path`, `pattern`, or
/// `command` argument, truncated to 80 chars.
fn summarize(arguments: &str) -> String {
    let summary = serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|v| {
            ["path", "pattern", "command"]
                .iter()
                .find_map(|k| v.get(*k).and_then(|s| s.as_str()))
                .map(str::to_string)
        })
        .unwrap_or_default();
    summary.chars().take(80).collect()
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
