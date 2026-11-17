//! Model client trait and provider adapters.

mod openai_chat;
mod sse;
#[cfg(feature = "testing")]
pub mod testing;

use std::pin::Pin;

use futures::Stream;
use futures::future::BoxFuture;
use thiserror::Error;

pub use openai_chat::{OpenAiChatClient, RetryPolicy};

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    User {
        text: String,
    },
    Assistant {
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCall(ToolCall),
    Completed { usage: Option<Usage> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>;

#[derive(Debug, Error, PartialEq)]
pub enum LlmError {
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("transport error: {0}")]
    Transport(String),
    #[error("failed to decode provider response: {0}")]
    Decode(String),
    #[error("request cancelled")]
    Cancelled,
}

/// One streaming completion against a provider. Returned errors from `stream` are
/// failures before streaming begins; mid-stream failures surface as `Err` items.
pub trait ModelClient: Send + Sync {
    fn stream<'a>(
        &'a self,
        request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ResponseStream, LlmError>>;
}
