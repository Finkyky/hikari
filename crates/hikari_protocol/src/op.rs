use serde::{Deserialize, Serialize};

use crate::ids::CallId;

/// A submission from a front-end to the core session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Op {
    UserInput {
        text: String,
    },
    Interrupt,
    Approval {
        call_id: CallId,
        decision: ApprovalDecision,
    },
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approve,
    ApproveForSession,
    Deny,
}

#[cfg(test)]
#[path = "op_tests.rs"]
mod tests;
