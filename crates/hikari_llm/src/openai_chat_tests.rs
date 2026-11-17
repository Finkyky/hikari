#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener as StdListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::StreamExt;

use crate::{
    LlmError, Message, ModelClient, ModelRequest, OpenAiChatClient, RetryPolicy, StreamEvent,
    ToolSpec, Usage,
};

fn request() -> ModelRequest {
    ModelRequest {
        model: "test-model".into(),
        system: "sys".into(),
        messages: vec![Message::User { text: "hi".into() }],
        tools: vec![ToolSpec {
            name: "read_file".into(),
            description: "d".into(),
            parameters: serde_json::json!({"type": "object"}),
        }],
    }
}

fn test_client(url: &str) -> OpenAiChatClient {
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    OpenAiChatClient::new(url, Some("sk-secret-123".into()))
        .with_http_client(http)
        .with_retry_policy(RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        })
}

async fn collect(stream: crate::ResponseStream) -> Vec<Result<StreamEvent, LlmError>> {
    stream.collect().await
}

/// Hand-written mock HTTP server: accepts `responses.len()` connections, replies
/// with each in order, and counts requests.
struct MockServer {
    url: String,
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicUsize>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl MockServer {
    fn start(responses: Vec<String>) -> Self {
        let listener = StdListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicUsize::new(0));
        let (hits2, stop2) = (Arc::clone(&hits), Arc::clone(&stop));
        let handle = std::thread::spawn(move || {
            let mut responses = responses.into_iter();
            while stop2.load(Ordering::SeqCst) == 0 {
                let Some(response) = responses.next() else {
                    break;
                };
                let socket = loop {
                    if stop2.load(Ordering::SeqCst) != 0 {
                        return;
                    }
                    match listener.accept() {
                        Ok((s, _)) => break s,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => return,
                    }
                };
                hits2.fetch_add(1, Ordering::SeqCst);
                let _ = socket.set_nonblocking(false);
                let mut reader = BufReader::new(&socket);
                let mut line = String::new();
                // Consume request headers (and body for our small JSON bodies).
                while reader.read_line(&mut line).is_ok() {
                    if line.trim_end().is_empty() {
                        break;
                    }
                    line.clear();
                }
                let mut writer = std::io::BufWriter::new(&socket);
                let _ = writer.write_all(response.as_bytes());
                let _ = writer.flush();
            }
        });
        Self {
            url,
            hits,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(1, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn sse_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{}",
        body.len(),
        body
    )
}

#[test]
fn request_body_shape() {
    let body = super::request_body(&request());
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(
        body["messages"][1],
        serde_json::json!({"role": "user", "content": "hi"})
    );
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "read_file");
}

#[tokio::test]
async fn streams_text_and_usage() {
    let body = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hel\"}}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\n",
        "data: [DONE]\n\n"
    );
    let server = MockServer::start(vec![sse_response(body)]);
    let client = test_client(&server.url);
    let events = collect(client.stream(&request()).await.unwrap()).await;
    assert_eq!(events[0], Ok(StreamEvent::TextDelta("Hel".into())));
    assert_eq!(events[1], Ok(StreamEvent::TextDelta("lo".into())));
    assert_eq!(
        events[2],
        Ok(StreamEvent::Completed {
            usage: Some(Usage {
                input_tokens: 3,
                output_tokens: 2
            })
        })
    );
    assert_eq!(events.len(), 3);
}

#[tokio::test]
async fn reassembles_fragmented_tool_calls() {
    let body = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[",
        "{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"pa\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[",
        "{\"index\":0,\"function\":{\"arguments\":\"th\\\":\\\"x\\\"}\"}},",
        "{\"index\":1,\"id\":\"call_2\",\"type\":\"function\",\"function\":{\"name\":\"grep\",\"arguments\":\"{\\\"p\\\":\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[",
        "{\"index\":1,\"function\":{\"arguments\":\"\\\"x\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let server = MockServer::start(vec![sse_response(body)]);
    let client = test_client(&server.url);
    let events = collect(client.stream(&request()).await.unwrap()).await;
    let calls: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Ok(StreamEvent::ToolCall(c)) => Some(c.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(calls[0].name, "read_file");
    assert_eq!(calls[0].arguments, "{\"path\":\"x\"}");
    assert_eq!(calls[1].name, "grep");
    assert_eq!(calls[1].arguments, "{\"p\":\"x\"}");
}

#[tokio::test]
async fn retries_429_then_succeeds() {
    let server = MockServer::start(vec![
        "HTTP/1.1 429 Too Many Requests\r\nretry-after: 0\r\ncontent-length: 0\r\n\r\n".to_string(),
        sse_response("data: [DONE]\n\n"),
    ]);
    let client = test_client(&server.url);
    let stream = client.stream(&request()).await.unwrap();
    let events = collect(stream).await;
    assert_eq!(server.hits.load(Ordering::SeqCst), 2);
    assert_eq!(events, vec![Ok(StreamEvent::Completed { usage: None })]);
}

#[tokio::test]
async fn http_error_display_never_contains_key() {
    let server = MockServer::start(vec![format!(
        "HTTP/1.1 401 Unauthorized\r\ncontent-length: {}\r\n\r\n{}",
        "{\"error\":{\"message\":\"bad key\"}}".len(),
        "{\"error\":{\"message\":\"bad key\"}}"
    )]);
    let client = test_client(&server.url);
    let Err(err) = client.stream(&request()).await else {
        panic!("expected error")
    };
    let text = err.to_string();
    assert!(text.contains("401"));
    assert!(text.contains("bad key"));
    assert!(!text.contains("sk-secret-123"));
}

#[tokio::test]
async fn transport_error_after_retries() {
    // Nothing listens on this port; connect fails.
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let client = OpenAiChatClient::new("http://127.0.0.1:1", None)
        .with_http_client(http)
        .with_retry_policy(RetryPolicy {
            max_retries: 1,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(5),
        });
    let Err(err) = client.stream(&request()).await else {
        panic!("expected error")
    };
    assert!(matches!(err, LlmError::Transport(_)));
}

#[tokio::test]
async fn exhausted_retries_return_http_error() {
    let response = "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\r\n".to_string();
    let server = MockServer::start(vec![response.clone(), response.clone(), response]);
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let client = OpenAiChatClient::new(&server.url, None)
        .with_http_client(http)
        .with_retry_policy(RetryPolicy {
            max_retries: 1,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(5),
        });
    let Err(err) = client.stream(&request()).await else {
        panic!("expected error")
    };
    assert!(matches!(err, LlmError::Http { status: 500, .. }));
    assert_eq!(server.hits.load(Ordering::SeqCst), 2);
}

#[test]
fn request_body_omits_empty_tools_and_tool_calls() {
    let mut req = request();
    req.tools.clear();
    req.messages.push(Message::Assistant {
        text: Some("done".into()),
        tool_calls: vec![],
    });
    let body = super::request_body(&req);
    assert!(body.get("tools").is_none());
    assert!(body["messages"][2].get("tool_calls").is_none());
}

#[tokio::test]
async fn tool_call_without_id_gets_fallback_id() {
    let body = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"list_dir\",\"arguments\":\"{}\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let server = MockServer::start(vec![sse_response(body)]);
    let client = test_client(&server.url);
    let events = collect(client.stream(&request()).await.unwrap()).await;
    let ids: Vec<String> = events
        .into_iter()
        .filter_map(|e| match e {
            Ok(StreamEvent::ToolCall(call)) => Some(call.id),
            Ok(StreamEvent::TextDelta(_) | StreamEvent::Completed { .. }) | Err(_) => None,
        })
        .collect();
    assert_eq!(ids, ["call_0_0"]);
}
