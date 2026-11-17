#![allow(clippy::unwrap_used)]

use serde_json::json;

use crate::event::{Event, EventMsg, TokenUsage, ToolRisk};
use crate::ids::{CallId, SessionId, TurnId};

fn turn() -> TurnId {
    TurnId("t-1".into())
}

#[test]
fn event_msg_round_trips_through_json() {
    let msgs = [
        EventMsg::SessionStarted,
        EventMsg::TurnStarted { turn_id: turn() },
        EventMsg::AgentMessageDelta {
            turn_id: turn(),
            delta: "he".into(),
        },
        EventMsg::AgentMessage {
            turn_id: turn(),
            text: "hello".into(),
        },
        EventMsg::ToolCallRequested {
            turn_id: turn(),
            call_id: CallId("c-1".into()),
            tool: "read_file".into(),
            risk: ToolRisk::ReadOnly,
            arguments: "{}".into(),
        },
        EventMsg::ApprovalRequested {
            call_id: CallId("c-1".into()),
        },
        EventMsg::ToolCallDenied {
            call_id: CallId("c-1".into()),
            reason: "denied".into(),
        },
        EventMsg::ToolCallCompleted {
            call_id: CallId("c-1".into()),
            ok: true,
            output: "out".into(),
        },
        EventMsg::TurnCompleted {
            turn_id: turn(),
            usage: Some(TokenUsage {
                input_tokens: 1,
                output_tokens: 2,
            }),
        },
        EventMsg::TurnAborted { turn_id: turn() },
        EventMsg::Error {
            message: "boom".into(),
        },
    ];
    for msg in msgs {
        let json = serde_json::to_string(&msg).unwrap();
        let back: EventMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(back, msg);
    }
}

#[test]
fn tool_call_requested_wire_shape() {
    let event = Event {
        session_id: SessionId("s-1".into()),
        seq: 7,
        msg: EventMsg::ToolCallRequested {
            turn_id: turn(),
            call_id: CallId("c-1".into()),
            tool: "shell".into(),
            risk: ToolRisk::Dangerous,
            arguments: "{}".into(),
        },
    };
    assert_eq!(
        serde_json::to_value(event).unwrap(),
        json!({
            "session_id": "s-1",
            "seq": 7,
            "msg": {
                "type": "tool_call_requested",
                "turn_id": "t-1",
                "call_id": "c-1",
                "tool": "shell",
                "risk": "dangerous",
                "arguments": "{}",
            }
        })
    );
}
