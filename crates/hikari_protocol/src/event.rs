use serde::{Deserialize, Serialize};

use crate::ids::{CallId, SessionId, TurnId};

/// One entry in the ordered event stream yielded by a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub session_id: SessionId,
    pub seq: u64,
    pub msg: EventMsg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventMsg {
    SessionStarted,
    TurnStarted {
        turn_id: TurnId,
    },
    AgentMessageDelta {
        turn_id: TurnId,
        delta: String,
    },
    AgentMessage {
        turn_id: TurnId,
        text: String,
    },
    ToolCallRequested {
        turn_id: TurnId,
        call_id: CallId,
        tool: String,
        risk: ToolRisk,
    },
    ApprovalRequested {
        call_id: CallId,
    },
    ToolCallCompleted {
        call_id: CallId,
        ok: bool,
    },
    TurnCompleted {
        turn_id: TurnId,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    ReadOnly,
    WorkspaceWrite,
    Dangerous,
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
