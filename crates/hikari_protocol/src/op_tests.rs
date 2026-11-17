use serde_json::json;

use crate::ids::CallId;
use crate::op::{ApprovalDecision, Op};

#[test]
fn op_round_trips_through_json() {
    let ops = [
        Op::UserInput { text: "hi".into() },
        Op::Interrupt,
        Op::Approval {
            call_id: CallId("call-1".into()),
            decision: ApprovalDecision::ApproveForSession,
        },
        Op::Shutdown,
    ];
    for op in ops {
        let json = serde_json::to_string(&op).unwrap();
        let back: Op = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }
}

#[test]
fn user_input_wire_shape() {
    let op = Op::UserInput { text: "hi".into() };
    assert_eq!(
        serde_json::to_value(op).unwrap(),
        json!({"type": "user_input", "text": "hi"})
    );
}

#[test]
fn approval_decision_wire_shape() {
    let op = Op::Approval {
        call_id: CallId("c".into()),
        decision: ApprovalDecision::Deny,
    };
    assert_eq!(
        serde_json::to_value(op).unwrap(),
        json!({"type": "approval", "call_id": "c", "decision": "deny"})
    );
}
