use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{self, StreamExt};
use serde_json::{Value, json};

use crate::sse::SseDecoder;
use crate::{
    LlmError, Message, ModelClient, ModelRequest, ResponseStream, StreamEvent, ToolCall, Usage,
};

const MAX_DELAY: Duration = Duration::from_secs(30);

/// How 429/5xx/connect failures are retried before any streamed data arrives.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: MAX_DELAY,
        }
    }
}

/// Client for OpenAI-compatible Chat Completions endpoints.
pub struct OpenAiChatClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    retry: RetryPolicy,
}

impl OpenAiChatClient {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            retry: RetryPolicy::default(),
        }
    }

    /// Test seam: replace the retry policy so tests don't sleep real backoffs.
    pub fn with_retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Test seam: replace the HTTP client (e.g. a `no_proxy` client for a local
    /// mock server when env proxies are set).
    pub fn with_http_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    async fn send(&self, request: &ModelRequest) -> Result<reqwest::Response, LlmError> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut attempt = 0u32;
        loop {
            let mut req = self.client.post(&url).json(&request_body(request));
            if let Some(key) = &self.api_key {
                req = req.bearer_auth(key);
            }
            match req.send().await {
                Ok(resp) if resp.status().is_success() => return Ok(resp),
                Ok(resp) => {
                    let status = resp.status();
                    let retry_after = retry_after_secs(&resp);
                    let message = error_message(resp).await;
                    let retryable = status.as_u16() == 429 || status.is_server_error();
                    if retryable && attempt < self.retry.max_retries {
                        attempt += 1;
                        tokio::time::sleep(self.backoff(attempt, retry_after)).await;
                        continue;
                    }
                    return Err(LlmError::Http {
                        status: status.as_u16(),
                        message,
                    });
                }
                Err(e) if e.is_connect() && attempt < self.retry.max_retries => {
                    attempt += 1;
                    tokio::time::sleep(self.backoff(attempt, None)).await;
                }
                Err(e) => return Err(LlmError::Transport(e.to_string())),
            }
        }
    }

    fn backoff(&self, attempt: u32, retry_after: Option<u64>) -> Duration {
        let delay = retry_after.map_or_else(
            || self.retry.base_delay.saturating_mul(1 << (attempt - 1)),
            Duration::from_secs,
        );
        delay.min(self.retry.max_delay)
    }
}

impl ModelClient for OpenAiChatClient {
    fn stream<'a>(
        &'a self,
        request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ResponseStream, LlmError>> {
        Box::pin(async move {
            let resp = self.send(request).await?;
            Ok(event_stream(resp))
        })
    }
}

fn retry_after_secs(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Extract the provider's error message; never includes request headers.
async fn error_message(resp: reqwest::Response) -> String {
    match resp.text().await {
        Ok(body) => serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or(body),
        Err(e) => e.to_string(),
    }
}

fn request_body(request: &ModelRequest) -> Value {
    let mut messages = Vec::with_capacity(request.messages.len() + 1);
    messages.push(json!({"role": "system", "content": request.system}));
    for msg in &request.messages {
        messages.push(message_json(msg));
    }
    let tools: Vec<Value> = request
        .tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect();
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
    });
    // Several OpenAI-compatible servers reject an empty `tools` array.
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    body
}

fn message_json(msg: &Message) -> Value {
    match msg {
        Message::User { text } => json!({"role": "user", "content": text}),
        Message::Assistant { text, tool_calls } => {
            let calls: Vec<Value> = tool_calls
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "type": "function",
                        "function": {"name": c.name, "arguments": c.arguments},
                    })
                })
                .collect();
            let mut msg = json!({"role": "assistant", "content": text});
            // OpenAI rejects an empty `tool_calls` array.
            if !calls.is_empty() {
                msg["tool_calls"] = Value::Array(calls);
            }
            msg
        }
        Message::ToolResult { call_id, content } => {
            json!({"role": "tool", "tool_call_id": call_id, "content": content})
        }
    }
}

/// Per-choice accumulation of in-flight tool-call argument fragments.
#[derive(Default)]
struct ChoiceState {
    calls: BTreeMap<u64, ToolCall>,
    emitted: bool,
}

struct StreamState {
    decoder: SseDecoder,
    body: futures::stream::BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
    queue: VecDeque<Result<StreamEvent, LlmError>>,
    choices: BTreeMap<u64, ChoiceState>,
    usage: Option<Usage>,
    done: bool,
    completed_emitted: bool,
}

fn event_stream(resp: reqwest::Response) -> ResponseStream {
    let state = StreamState {
        decoder: SseDecoder::new(),
        body: Box::pin(resp.bytes_stream()),
        queue: VecDeque::new(),
        choices: BTreeMap::new(),
        usage: None,
        done: false,
        completed_emitted: false,
    };
    Box::pin(stream::unfold(state, |mut st| async move {
        loop {
            if let Some(item) = st.queue.pop_front() {
                return Some((item, st));
            }
            if st.done {
                return None;
            }
            match st.body.next().await {
                Some(Ok(chunk)) => {
                    for payload in st.decoder.feed(&chunk) {
                        parse_chunk(&payload, &mut st);
                    }
                }
                Some(Err(e)) => {
                    st.done = true;
                    return Some((Err(LlmError::Transport(e.to_string())), st));
                }
                None => {
                    st.done = true;
                    for payload in st.decoder.finish() {
                        parse_chunk(&payload, &mut st);
                    }
                    finish(&mut st);
                }
            }
        }
    }))
}

/// Emits a choice's accumulated tool calls once. Some compatible servers omit call ids, which the
/// follow-up tool messages must reference, so a stable fallback is assigned.
fn emit_calls(
    choice_index: u64,
    choice: &mut ChoiceState,
    queue: &mut VecDeque<Result<StreamEvent, LlmError>>,
) {
    if choice.emitted {
        return;
    }
    choice.emitted = true;
    for (call_index, mut call) in std::mem::take(&mut choice.calls) {
        if call.id.is_empty() {
            call.id = format!("call_{choice_index}_{call_index}");
        }
        queue.push_back(Ok(StreamEvent::ToolCall(call)));
    }
}

fn finish(st: &mut StreamState) {
    for (&index, choice) in st.choices.iter_mut() {
        emit_calls(index, choice, &mut st.queue);
    }
    if !st.completed_emitted {
        st.completed_emitted = true;
        st.queue
            .push_back(Ok(StreamEvent::Completed { usage: st.usage }));
    }
}

fn parse_chunk(payload: &str, st: &mut StreamState) {
    if payload.trim() == "[DONE]" {
        st.done = true;
        finish(st);
        return;
    }
    let chunk: Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            st.queue
                .push_back(Err(LlmError::Decode(format!("{e}: {payload}"))));
            return;
        }
    };
    if let Some(usage) = chunk.get("usage").and_then(Value::as_object)
        && !usage.is_empty()
    {
        let input = usage
            .get("prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let output = usage
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        st.usage = Some(Usage {
            input_tokens: input,
            output_tokens: output,
        });
    }
    let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
        return;
    };
    for choice in choices {
        let index = choice.get("index").and_then(Value::as_u64).unwrap_or(0);
        let state = st.choices.entry(index).or_default();
        if let Some(delta) = choice.get("delta") {
            if let Some(content) = delta.get("content").and_then(Value::as_str)
                && !content.is_empty()
            {
                st.queue
                    .push_back(Ok(StreamEvent::TextDelta(content.to_string())));
            }
            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    accumulate_call(state, call);
                }
            }
        }
        if choice.get("finish_reason").is_some_and(|r| !r.is_null()) {
            emit_calls(index, state, &mut st.queue);
        }
    }
}

fn accumulate_call(state: &mut ChoiceState, call: &Value) {
    let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
    let entry = state.calls.entry(index).or_insert_with(|| ToolCall {
        id: String::new(),
        name: String::new(),
        arguments: String::new(),
    });
    if let Some(id) = call.get("id").and_then(Value::as_str) {
        entry.id = id.to_string();
    }
    if let Some(func) = call.get("function") {
        if let Some(name) = func.get("name").and_then(Value::as_str) {
            entry.name = name.to_string();
        }
        if let Some(args) = func.get("arguments").and_then(Value::as_str) {
            entry.arguments.push_str(args);
        }
    }
}

#[cfg(test)]
#[path = "openai_chat_tests.rs"]
mod tests;
