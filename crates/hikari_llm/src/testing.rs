//! Scripted `ModelClient` fakes for tests in dependent crates. Not shipped.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{self, StreamExt};

use crate::{LlmError, ModelClient, ModelRequest, ResponseStream, StreamEvent};

/// The sequence of items one `stream` call will yield.
pub type Script = Vec<Result<StreamEvent, LlmError>>;

/// Replays one script per `stream` call and records every request it receives.
pub struct ScriptedClient {
    scripts: Mutex<VecDeque<Script>>,
    delay: Option<Duration>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ScriptedClient {
    pub fn new(scripts: Vec<Script>) -> Self {
        Self {
            scripts: Mutex::new(scripts.into()),
            delay: None,
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    pub fn received_requests(&self) -> Vec<ModelRequest> {
        self.requests
            .lock()
            .expect("requests mutex poisoned")
            .clone()
    }
}

impl ModelClient for ScriptedClient {
    fn stream<'a>(
        &'a self,
        request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ResponseStream, LlmError>> {
        Box::pin(async move {
            self.requests
                .lock()
                .expect("requests mutex poisoned")
                .push(request.clone());
            let script = self
                .scripts
                .lock()
                .expect("scripts mutex poisoned")
                .pop_front()
                .ok_or_else(|| LlmError::Transport("script exhausted".to_string()))?;
            let delay = self.delay;
            let stream = stream::iter(script).then(move |item| async move {
                if let Some(d) = delay {
                    tokio::time::sleep(d).await;
                }
                item
            });
            Ok(Box::pin(stream) as ResponseStream)
        })
    }
}
