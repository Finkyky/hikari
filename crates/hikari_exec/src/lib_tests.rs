#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use hikari_llm::testing::{Script, ScriptedClient};
use hikari_llm::{LlmError, StreamEvent, ToolCall, Usage};
use hikari_protocol::event::Event;

use crate::{ExecArgs, run_with};

fn args(prompt: &str, json: bool) -> ExecArgs {
    ExecArgs {
        prompt: prompt.into(),
        workspace: tempfile::tempdir().unwrap().keep(),
        json,
        full_auto: false,
        provider: None,
        model: None,
    }
}

fn text_done(text: &str) -> Script {
    vec![
        Ok(StreamEvent::TextDelta(text.to_string())),
        Ok(StreamEvent::Completed {
            usage: Some(Usage {
                input_tokens: 1,
                output_tokens: 1,
            }),
        }),
    ]
}

#[tokio::test]
async fn human_mode_stdout_has_only_final_text() {
    let client = Arc::new(ScriptedClient::new(vec![text_done("the answer")]));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(&args("q", false), "m".into(), client, &mut out, &mut err).await;
    assert_eq!(code, std::process::ExitCode::SUCCESS);
    assert_eq!(String::from_utf8(out).unwrap(), "the answer\n");
    assert_eq!(String::from_utf8(err).unwrap(), "");
}

#[tokio::test]
async fn progress_goes_to_stderr() {
    let client = Arc::new(ScriptedClient::new(vec![
        vec![Ok(StreamEvent::ToolCall(ToolCall {
            id: "c1".into(),
            name: "list_dir".into(),
            arguments: "{\"path\":\".\"}".into(),
        }))],
        text_done("done"),
    ]));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(&args("q", false), "m".into(), client, &mut out, &mut err).await;
    assert_eq!(code, std::process::ExitCode::SUCCESS);
    assert_eq!(String::from_utf8(out).unwrap(), "done\n");
    let progress = String::from_utf8(err).unwrap();
    assert!(progress.contains("> list_dir ."));
    assert!(progress.contains("  ok"));
}

#[tokio::test]
async fn json_mode_emits_one_event_per_line() {
    let client = Arc::new(ScriptedClient::new(vec![text_done("hi")]));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(&args("q", true), "m".into(), client, &mut out, &mut err).await;
    assert_eq!(code, std::process::ExitCode::SUCCESS);
    let stdout = String::from_utf8(out).unwrap();
    for line in stdout.lines() {
        let event: Event = serde_json::from_str(line).unwrap();
        assert!(event.seq >= 1);
    }
    assert!(stdout.contains("\"type\":\"turn_completed\""));
    assert!(!stdout.contains("> "));
}

#[tokio::test]
async fn denied_tool_call_is_reported() {
    let client = Arc::new(ScriptedClient::new(vec![
        vec![Ok(StreamEvent::ToolCall(ToolCall {
            id: "c1".into(),
            name: "shell".into(),
            arguments: "{\"command\":\"rm -rf /\"}".into(),
        }))],
        text_done("cannot do that"),
    ]));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(&args("q", false), "m".into(), client, &mut out, &mut err).await;
    assert_eq!(code, std::process::ExitCode::SUCCESS);
    let progress = String::from_utf8(err).unwrap();
    assert!(progress.contains("> shell rm -rf /"));
    assert!(progress.contains("  denied"));
}

#[tokio::test]
async fn provider_error_exits_1() {
    let client = Arc::new(ScriptedClient::new(vec![vec![Err(LlmError::Transport(
        "down".into(),
    ))]]));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(&args("q", false), "m".into(), client, &mut out, &mut err).await;
    assert_eq!(code, std::process::ExitCode::from(1));
    assert!(String::from_utf8(err).unwrap().contains("down"));
}
