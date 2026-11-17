#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use hikari_llm::testing::ScriptedClient;
use hikari_llm::{LlmError, Message, StreamEvent, ToolCall, Usage};
use hikari_protocol::event::{EventMsg, ToolRisk};
use hikari_protocol::op::{ApprovalDecision, Op};
use hikari_tools::ToolRegistry;

use crate::{ApprovalPolicy, SessionConfig, SessionHandle, spawn_session};

fn session(scripts: Vec<hikari_llm::testing::Script>, policy: ApprovalPolicy) -> SessionHandle {
    session_in(tempfile::tempdir().unwrap().keep(), scripts, policy)
}

fn session_in(
    workspace: std::path::PathBuf,
    scripts: Vec<hikari_llm::testing::Script>,
    policy: ApprovalPolicy,
) -> SessionHandle {
    let config = SessionConfig {
        model: "m".into(),
        workspace,
        policy,
        max_round_trips: 100,
    };
    spawn_session(
        config,
        Arc::new(ScriptedClient::new(scripts)),
        ToolRegistry::builtin(),
    )
}

async fn start(handle: &SessionHandle) {
    handle
        .ops
        .send(Op::UserInput { text: "go".into() })
        .await
        .unwrap();
}

async fn next_msg(handle: &mut SessionHandle) -> EventMsg {
    handle.events.recv().await.unwrap().msg
}

async fn collect_until_end(handle: &mut SessionHandle) -> Vec<EventMsg> {
    let mut msgs = Vec::new();
    loop {
        let msg = next_msg(handle).await;
        let end = matches!(
            msg,
            EventMsg::TurnCompleted { .. } | EventMsg::TurnAborted { .. } | EventMsg::Error { .. }
        );
        msgs.push(msg);
        if end {
            return msgs;
        }
    }
}

fn text_done(text: &str) -> hikari_llm::testing::Script {
    vec![
        Ok(StreamEvent::TextDelta(text.to_string())),
        Ok(StreamEvent::Completed {
            usage: Some(Usage {
                input_tokens: 1,
                output_tokens: 2,
            }),
        }),
    ]
}

#[tokio::test]
async fn text_only_turn() {
    let mut handle = session(vec![text_done("done!")], ApprovalPolicy::Untrusted);
    start(&handle).await;
    let mut msgs = collect_until_end(&mut handle).await;
    assert!(matches!(msgs.first(), Some(EventMsg::SessionStarted)));
    msgs.remove(0);
    let kinds: Vec<&str> = msgs
        .iter()
        .map(|m| match m {
            EventMsg::TurnStarted { .. } => "started",
            EventMsg::AgentMessageDelta { .. } => "delta",
            EventMsg::AgentMessage { .. } => "message",
            EventMsg::TurnCompleted { .. } => "completed",
            EventMsg::SessionStarted
            | EventMsg::ToolCallRequested { .. }
            | EventMsg::ApprovalRequested { .. }
            | EventMsg::ToolCallDenied { .. }
            | EventMsg::ToolCallCompleted { .. }
            | EventMsg::TurnAborted { .. }
            | EventMsg::Error { .. } => "other",
        })
        .collect();
    assert_eq!(kinds, ["started", "delta", "message", "completed"]);
    let EventMsg::TurnCompleted { usage, .. } = msgs.last().unwrap() else {
        panic!()
    };
    assert_eq!(usage.as_ref().unwrap().input_tokens, 1);
}

#[tokio::test]
async fn read_only_tool_runs_without_approval() {
    let mut handle = session(
        vec![
            vec![Ok(StreamEvent::ToolCall(ToolCall {
                id: "c1".into(),
                name: "list_dir".into(),
                arguments: "{}".into(),
            }))],
            text_done("listed"),
        ],
        ApprovalPolicy::Untrusted,
    );
    start(&handle).await;
    let msgs = collect_until_end(&mut handle).await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        EventMsg::ToolCallRequested { tool, risk, .. }
            if tool == "list_dir" && *risk == ToolRisk::ReadOnly
    )));
    assert!(msgs.iter().any(|m| matches!(
        m,
        EventMsg::ToolCallCompleted { call_id, ok, .. } if call_id.0 == "c1" && *ok
    )));
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, EventMsg::ApprovalRequested { .. }))
    );
}

#[tokio::test]
async fn untrusted_deny_yields_denial_result() {
    let dir = tempfile::tempdir().unwrap();
    let client = Arc::new(ScriptedClient::new(vec![
        vec![Ok(StreamEvent::ToolCall(ToolCall {
            id: "c1".into(),
            name: "write_file".into(),
            arguments: "{\"path\":\"f.txt\",\"content\":\"x\"}".into(),
        }))],
        text_done("ok then"),
    ]));
    let config = SessionConfig {
        model: "m".into(),
        workspace: dir.path().to_path_buf(),
        policy: ApprovalPolicy::Untrusted,
        max_round_trips: 100,
    };
    let mut handle = spawn_session(config, client.clone(), ToolRegistry::builtin());
    start(&handle).await;
    // Wait for the approval request, then deny.
    loop {
        if let EventMsg::ApprovalRequested { call_id } = next_msg(&mut handle).await {
            handle
                .ops
                .send(Op::Approval {
                    call_id,
                    decision: ApprovalDecision::Deny,
                })
                .await
                .unwrap();
            break;
        }
    }
    let msgs = collect_until_end(&mut handle).await;
    assert!(
        msgs.iter()
            .any(|m| matches!(m, EventMsg::ToolCallDenied { .. }))
    );
    assert!(!dir.path().join("f.txt").exists());
    // The denial text reaches the model on the next round trip.
    let requests = client.received_requests();
    let last = requests.last().unwrap();
    assert!(last.messages.iter().any(|m| matches!(
        m,
        Message::ToolResult { content, .. }
            if content.contains("--full-auto")
    )));
}

#[tokio::test]
async fn auto_runs_write_tools() {
    let dir = tempfile::tempdir().unwrap();
    let mut handle = session_in(
        dir.path().to_path_buf(),
        vec![
            vec![Ok(StreamEvent::ToolCall(ToolCall {
                id: "c1".into(),
                name: "write_file".into(),
                arguments: "{\"path\":\"f.txt\",\"content\":\"hi\"}".into(),
            }))],
            text_done("wrote"),
        ],
        ApprovalPolicy::Auto,
    );
    start(&handle).await;
    let msgs = collect_until_end(&mut handle).await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        EventMsg::ToolCallCompleted { ok, .. } if *ok
    )));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "hi"
    );
}

#[tokio::test]
async fn stops_after_max_round_trips() {
    let config = SessionConfig {
        model: "m".into(),
        workspace: tempfile::tempdir().unwrap().keep(),
        policy: ApprovalPolicy::Untrusted,
        max_round_trips: 3,
    };
    let scripts: Vec<hikari_llm::testing::Script> = (0..3)
        .map(|_| {
            vec![Ok(StreamEvent::ToolCall(ToolCall {
                id: "c".into(),
                name: "list_dir".into(),
                arguments: "{}".into(),
            }))]
        })
        .collect();
    let mut handle = spawn_session(
        config,
        Arc::new(ScriptedClient::new(scripts)),
        ToolRegistry::builtin(),
    );
    start(&handle).await;
    let msgs = collect_until_end(&mut handle).await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        EventMsg::Error { message } if message.contains("3 model round trips")
    )));
}

#[tokio::test]
async fn interrupt_during_stream_aborts_fast() {
    let config = SessionConfig {
        model: "m".into(),
        workspace: tempfile::tempdir().unwrap().keep(),
        policy: ApprovalPolicy::Untrusted,
        max_round_trips: 100,
    };
    let client = ScriptedClient::new(vec![vec![
        Ok(StreamEvent::TextDelta("slow".into())),
        Ok(StreamEvent::TextDelta("slower".into())),
    ]])
    .with_delay(Duration::from_secs(30));
    let mut handle = spawn_session(config, Arc::new(client), ToolRegistry::builtin());
    start(&handle).await;
    // First TurnStarted, then interrupt before the first (delayed) delta lands.
    let start = Instant::now();
    while !matches!(next_msg(&mut handle).await, EventMsg::TurnStarted { .. }) {}
    handle.ops.send(Op::Interrupt).await.unwrap();
    loop {
        match next_msg(&mut handle).await {
            EventMsg::TurnAborted { .. } => break,
            EventMsg::Error { message } => panic!("unexpected error: {message}"),
            EventMsg::SessionStarted
            | EventMsg::TurnStarted { .. }
            | EventMsg::AgentMessageDelta { .. }
            | EventMsg::AgentMessage { .. }
            | EventMsg::ToolCallRequested { .. }
            | EventMsg::ApprovalRequested { .. }
            | EventMsg::ToolCallDenied { .. }
            | EventMsg::ToolCallCompleted { .. }
            | EventMsg::TurnCompleted { .. } => {}
        }
    }
    assert!(start.elapsed() < Duration::from_millis(500));
}

#[tokio::test]
async fn provider_error_surfaces_as_error_event() {
    let mut handle = session(
        vec![vec![Err(LlmError::Transport("boom".into()))]],
        ApprovalPolicy::Untrusted,
    );
    start(&handle).await;
    let msgs = collect_until_end(&mut handle).await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        EventMsg::Error { message } if message.contains("boom")
    )));
}

fn sleep_command() -> String {
    if cfg!(windows) {
        "Start-Sleep -Seconds 30".into()
    } else {
        "sleep 30".into()
    }
}

fn tool_call(id: &str, name: &str, arguments: serde_json::Value) -> Result<StreamEvent, LlmError> {
    Ok(StreamEvent::ToolCall(ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: arguments.to_string(),
    }))
}

async fn wait_for(handle: &mut SessionHandle, pred: impl Fn(&EventMsg) -> bool) {
    while !pred(&next_msg(handle).await) {}
}

#[tokio::test]
async fn interrupt_during_tool_run_aborts_fast() {
    let mut handle = session(
        vec![vec![tool_call(
            "c1",
            "shell",
            serde_json::json!({"command": sleep_command()}),
        )]],
        ApprovalPolicy::Auto,
    );
    start(&handle).await;
    wait_for(&mut handle, |m| {
        matches!(m, EventMsg::ToolCallRequested { .. })
    })
    .await;
    let start = Instant::now();
    handle.ops.send(Op::Interrupt).await.unwrap();
    let msgs = collect_until_end(&mut handle).await;
    assert!(matches!(msgs.last(), Some(EventMsg::TurnAborted { .. })));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn interrupt_during_approval_wait_aborts() {
    let mut handle = session(
        vec![vec![tool_call(
            "c1",
            "write_file",
            serde_json::json!({"path": "f.txt", "content": "x"}),
        )]],
        ApprovalPolicy::Untrusted,
    );
    start(&handle).await;
    wait_for(&mut handle, |m| {
        matches!(m, EventMsg::ApprovalRequested { .. })
    })
    .await;
    handle.ops.send(Op::Interrupt).await.unwrap();
    let msgs = collect_until_end(&mut handle).await;
    assert!(matches!(msgs.last(), Some(EventMsg::TurnAborted { .. })));
}

#[tokio::test]
async fn aborted_turn_leaves_a_result_for_every_tool_call() {
    let client = Arc::new(ScriptedClient::new(vec![
        vec![
            tool_call(
                "c1",
                "shell",
                serde_json::json!({"command": sleep_command()}),
            ),
            tool_call("c2", "list_dir", serde_json::json!({})),
        ],
        text_done("next"),
    ]));
    let config = SessionConfig {
        model: "m".into(),
        workspace: tempfile::tempdir().unwrap().keep(),
        policy: ApprovalPolicy::Auto,
        max_round_trips: 100,
    };
    let mut handle = spawn_session(config, client.clone(), ToolRegistry::builtin());
    start(&handle).await;
    wait_for(&mut handle, |m| {
        matches!(m, EventMsg::ToolCallRequested { .. })
    })
    .await;
    handle.ops.send(Op::Interrupt).await.unwrap();
    wait_for(&mut handle, |m| matches!(m, EventMsg::TurnAborted { .. })).await;

    start(&handle).await;
    let msgs = collect_until_end(&mut handle).await;
    assert!(matches!(msgs.last(), Some(EventMsg::TurnCompleted { .. })));
    let requests = client.received_requests();
    let messages = &requests.last().unwrap().messages;
    for id in ["c1", "c2"] {
        assert!(
            messages.iter().any(|m| matches!(
                m,
                Message::ToolResult { call_id, content } if call_id == id && content == "Cancelled by user."
            )),
            "missing cancelled result for {id}"
        );
    }
}
