//! Session, turn loop, context assembly, compaction, and approvals.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use hikari_llm::{Message, ModelClient, ModelRequest, StreamEvent, ToolCall, Usage};
use hikari_protocol::event::{Event, EventMsg, TokenUsage, ToolRisk};
use hikari_protocol::ids::{CallId, SessionId, TurnId};
use hikari_protocol::op::{ApprovalDecision, Op};
use hikari_tools::{ToolContext, ToolOutput, cap_output};

pub use hikari_tools::ToolRegistry;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Max characters of a single tool result sent to the model.
const RESULT_CAP: usize = 30_000;

const DENIED_RESULT: &str = "Denied: this action requires approval. Re-run with --full-auto to allow writes and shell commands.";

const CANCELLED_RESULT: &str = "Cancelled by user.";

/// Records an `Op` that arrives mid-turn or between turns; `None` means the channel closed.
fn apply_op(
    op: Option<Op>,
    cancel: &CancellationToken,
    pending: &mut HashMap<String, ApprovalDecision>,
    shutdown: &mut bool,
) {
    match op {
        Some(Op::Interrupt) => cancel.cancel(),
        Some(Op::Shutdown) | None => {
            *shutdown = true;
            cancel.cancel();
        }
        Some(Op::Approval { call_id, decision }) => {
            pending.insert(call_id.0, decision);
        }
        Some(Op::UserInput { .. }) => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalPolicy {
    Untrusted,
    Auto,
}

pub struct SessionConfig {
    pub model: String,
    pub workspace: PathBuf,
    pub policy: ApprovalPolicy,
    pub max_round_trips: usize,
}

/// Front-end handle: submit `Op`s, receive the ordered `Event` stream.
pub struct SessionHandle {
    pub ops: mpsc::Sender<Op>,
    pub events: mpsc::Receiver<Event>,
}

pub fn spawn_session(
    config: SessionConfig,
    client: Arc<dyn ModelClient>,
    tools: ToolRegistry,
) -> SessionHandle {
    let (ops, op_rx) = mpsc::channel(32);
    let (events, ev_rx) = mpsc::channel(64);
    tokio::spawn(actor(Actor::new(config, client, tools), op_rx, events));
    SessionHandle { ops, events: ev_rx }
}

struct Actor {
    config: SessionConfig,
    client: Arc<dyn ModelClient>,
    tools: ToolRegistry,
    messages: Vec<Message>,
    session_id: SessionId,
    seq: u64,
    turn: u64,
    policy: ApprovalPolicy,
    pending: HashMap<String, ApprovalDecision>,
    shutdown: bool,
}

impl Actor {
    fn new(config: SessionConfig, client: Arc<dyn ModelClient>, tools: ToolRegistry) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Self {
            policy: config.policy,
            messages: Vec::new(),
            session_id: SessionId(format!("session-{nanos:x}")),
            seq: 0,
            turn: 0,
            pending: HashMap::new(),
            shutdown: false,
            config,
            client,
            tools,
        }
    }

    async fn emit(&mut self, tx: &mpsc::Sender<Event>, msg: EventMsg) {
        self.seq += 1;
        let event = Event {
            session_id: self.session_id.clone(),
            seq: self.seq,
            msg,
        };
        let _ = tx.send(event).await;
    }

    fn on_op(&mut self, op: Option<Op>, cancel: &CancellationToken) {
        apply_op(op, cancel, &mut self.pending, &mut self.shutdown);
    }

    /// Ends the turn after cancellation. Every tool call of the last assistant message must have a
    /// result, or the next request is rejected by the provider.
    async fn abort_turn(&mut self, turn_id: TurnId, ev_tx: &mpsc::Sender<Event>) {
        if let Some(pos) = self
            .messages
            .iter()
            .rposition(|m| matches!(m, Message::Assistant { .. }))
            && let Message::Assistant { tool_calls, .. } = &self.messages[pos]
        {
            let answered: Vec<&str> = self.messages[pos + 1..]
                .iter()
                .filter_map(|m| match m {
                    Message::ToolResult { call_id, .. } => Some(call_id.as_str()),
                    Message::User { .. } | Message::Assistant { .. } => None,
                })
                .collect();
            let missing: Vec<Message> = tool_calls
                .iter()
                .filter(|c| !answered.contains(&c.id.as_str()))
                .map(|c| Message::ToolResult {
                    call_id: c.id.clone(),
                    content: CANCELLED_RESULT.to_string(),
                })
                .collect();
            self.messages.extend(missing);
        }
        self.emit(ev_tx, EventMsg::TurnAborted { turn_id }).await;
    }

    async fn run_turn(
        &mut self,
        text: String,
        op_rx: &mut mpsc::Receiver<Op>,
        ev_tx: &mpsc::Sender<Event>,
    ) {
        self.turn += 1;
        let turn_id = TurnId(format!("turn-{}", self.turn));
        self.emit(
            ev_tx,
            EventMsg::TurnStarted {
                turn_id: turn_id.clone(),
            },
        )
        .await;
        self.messages.push(Message::User { text });

        let cancel = CancellationToken::new();
        let ctx = ToolContext {
            workspace: self.config.workspace.clone(),
            cancel: cancel.clone(),
        };
        let mut usage = UsageAcc::default();

        for _round in 0..self.config.max_round_trips {
            let request = ModelRequest {
                model: self.config.model.clone(),
                system: system_prompt(&self.config.workspace),
                messages: self.messages.clone(),
                tools: self.tools.specs(),
            };
            let outcome = self
                .stream_round(&request, op_rx, ev_tx, &turn_id, &cancel)
                .await;
            let round = match outcome {
                RoundOutcome::Done(r) => r,
                RoundOutcome::Failed => return,
            };
            usage.add(round.usage);
            if cancel.is_cancelled() {
                self.abort_turn(turn_id, ev_tx).await;
                return;
            }
            if round.calls.is_empty() {
                self.messages.push(Message::Assistant {
                    text: Some(round.text.clone()),
                    tool_calls: vec![],
                });
                self.emit(
                    ev_tx,
                    EventMsg::AgentMessage {
                        turn_id: turn_id.clone(),
                        text: round.text,
                    },
                )
                .await;
                self.emit(
                    ev_tx,
                    EventMsg::TurnCompleted {
                        turn_id,
                        usage: usage.get(),
                    },
                )
                .await;
                return;
            }
            self.messages.push(Message::Assistant {
                text: (!round.text.is_empty()).then_some(round.text),
                tool_calls: round.calls.clone(),
            });
            for call in round.calls {
                self.run_call(&call, &ctx, op_rx, ev_tx, &turn_id, &cancel)
                    .await;
                if cancel.is_cancelled() {
                    break;
                }
            }
            if cancel.is_cancelled() {
                self.abort_turn(turn_id, ev_tx).await;
                return;
            }
        }
        self.emit(
            ev_tx,
            EventMsg::Error {
                message: format!(
                    "stopped after {} model round trips",
                    self.config.max_round_trips
                ),
            },
        )
        .await;
    }

    /// One model round trip; returns None-ish via Failed after emitting Error.
    async fn stream_round(
        &mut self,
        request: &ModelRequest,
        op_rx: &mut mpsc::Receiver<Op>,
        ev_tx: &mpsc::Sender<Event>,
        turn_id: &TurnId,
        cancel: &CancellationToken,
    ) -> RoundOutcome {
        let mut stream = match tokio::select! {
            r = self.client.stream(request) => r,
            _ = cancel.cancelled() => return RoundOutcome::Done(RoundResult::default()),
        } {
            Ok(s) => s,
            Err(e) => {
                self.emit(
                    ev_tx,
                    EventMsg::Error {
                        message: e.to_string(),
                    },
                )
                .await;
                return RoundOutcome::Failed;
            }
        };
        let mut result = RoundResult::default();
        loop {
            tokio::select! {
                item = stream.next() => match item {
                    None => break,
                    Some(Ok(StreamEvent::TextDelta(delta))) => {
                        result.text.push_str(&delta);
                        self.emit(ev_tx, EventMsg::AgentMessageDelta {
                            turn_id: turn_id.clone(), delta }).await;
                    }
                    Some(Ok(StreamEvent::ToolCall(call))) => result.calls.push(call),
                    Some(Ok(StreamEvent::Completed { usage })) => result.usage = usage,
                    Some(Err(e)) => {
                        self.emit(ev_tx, EventMsg::Error { message: e.to_string() }).await;
                        return RoundOutcome::Failed;
                    }
                },
                op = op_rx.recv() => self.on_op(op, cancel),
                _ = cancel.cancelled() => {
                    break;
                }
            }
            if cancel.is_cancelled() {
                break;
            }
        }
        RoundOutcome::Done(result)
    }

    /// Runs or denies one tool call. On cancellation it returns without recording a result;
    /// `abort_turn` fills it in.
    async fn run_call(
        &mut self,
        call: &ToolCall,
        ctx: &ToolContext,
        op_rx: &mut mpsc::Receiver<Op>,
        ev_tx: &mpsc::Sender<Event>,
        turn_id: &TurnId,
        cancel: &CancellationToken,
    ) {
        let call_id = CallId(call.id.clone());
        let args: serde_json::Value =
            serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null);
        let risk = self
            .tools
            .get(&call.name)
            .map_or(ToolRisk::Dangerous, |t| t.risk(&args, ctx));
        self.emit(
            ev_tx,
            EventMsg::ToolCallRequested {
                turn_id: turn_id.clone(),
                call_id: call_id.clone(),
                tool: call.name.clone(),
                risk,
                arguments: call.arguments.clone(),
            },
        )
        .await;

        if risk != ToolRisk::ReadOnly && self.policy == ApprovalPolicy::Untrusted {
            match self.await_approval(&call_id, op_rx, ev_tx, cancel).await {
                Some(ApprovalDecision::Approve) => {}
                Some(ApprovalDecision::ApproveForSession) => self.policy = ApprovalPolicy::Auto,
                None => return,
                Some(ApprovalDecision::Deny) => {
                    self.emit(
                        ev_tx,
                        EventMsg::ToolCallDenied {
                            call_id: call_id.clone(),
                            reason: "denied by approval policy".to_string(),
                        },
                    )
                    .await;
                    self.messages.push(Message::ToolResult {
                        call_id: call.id.clone(),
                        content: DENIED_RESULT.to_string(),
                    });
                    return;
                }
            }
        }

        let output = match self.tools.get(&call.name) {
            None => ToolOutput::Err(format!("unknown tool: {}", call.name)),
            Some(t) => {
                let run = t.run(args, ctx);
                tokio::pin!(run);
                loop {
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return,
                        r = &mut run => break r,
                        op = op_rx.recv() => {
                            apply_op(op, cancel, &mut self.pending, &mut self.shutdown);
                        }
                    }
                }
            }
        };
        let ok = output.ok();
        let capped = cap_output(&output.into_text(), RESULT_CAP);
        self.emit(
            ev_tx,
            EventMsg::ToolCallCompleted {
                call_id,
                ok,
                output: capped.clone(),
            },
        )
        .await;
        self.messages.push(Message::ToolResult {
            call_id: call.id.clone(),
            content: capped,
        });
    }

    async fn await_approval(
        &mut self,
        call_id: &CallId,
        op_rx: &mut mpsc::Receiver<Op>,
        ev_tx: &mpsc::Sender<Event>,
        cancel: &CancellationToken,
    ) -> Option<ApprovalDecision> {
        if let Some(d) = self.pending.remove(&call_id.0) {
            return Some(d);
        }
        self.emit(
            ev_tx,
            EventMsg::ApprovalRequested {
                call_id: call_id.clone(),
            },
        )
        .await;
        loop {
            tokio::select! {
                op = op_rx.recv() => {
                    self.on_op(op, cancel);
                    if let Some(d) = self.pending.remove(&call_id.0) {
                        return Some(d);
                    }
                    if cancel.is_cancelled() {
                        return None;
                    }
                }
                _ = cancel.cancelled() => return None,
            }
        }
    }
}

enum RoundOutcome {
    Done(RoundResult),
    Failed,
}

#[derive(Default)]
struct RoundResult {
    text: String,
    calls: Vec<ToolCall>,
    usage: Option<Usage>,
}

#[derive(Default)]
struct UsageAcc {
    input: u64,
    output: u64,
    seen: bool,
}

impl UsageAcc {
    fn add(&mut self, usage: Option<Usage>) {
        if let Some(u) = usage {
            self.seen = true;
            self.input += u.input_tokens;
            self.output += u.output_tokens;
        }
    }

    fn get(&self) -> Option<TokenUsage> {
        self.seen.then_some(TokenUsage {
            input_tokens: self.input,
            output_tokens: self.output,
        })
    }
}

fn system_prompt(workspace: &Path) -> String {
    format!(
        "You are Hikari, a coding agent running in the user's terminal. You help with software engineering\ntasks in the workspace at {workspace} on {os}.\n\n- Use the tools to inspect the codebase before answering questions about it or changing it. Do not\n  guess file contents.\n- Prefer edit_file for targeted changes and read a file before editing it.\n- Follow the conventions of the existing code. Keep changes minimal and focused on the request.\n- Run relevant build or test commands with shell when they help verify your work.\n- If a tool call is denied, do not retry it; explain what you would have done instead.\n- When done, reply with a concise summary of what you found or changed.",
        workspace = workspace.display(),
        os = std::env::consts::OS
    )
}

async fn actor(mut actor: Actor, mut op_rx: mpsc::Receiver<Op>, ev_tx: mpsc::Sender<Event>) {
    actor.emit(&ev_tx, EventMsg::SessionStarted).await;
    while let Some(op) = op_rx.recv().await {
        match op {
            Op::UserInput { text } => actor.run_turn(text, &mut op_rx, &ev_tx).await,
            Op::Interrupt | Op::Shutdown | Op::Approval { .. } => {
                actor.on_op(Some(op), &CancellationToken::new());
            }
        }
        if actor.shutdown {
            break;
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
