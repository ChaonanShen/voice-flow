use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use super::{ChatRequest, ChatResponse, LlmClient, LlmError};

#[derive(Debug, Clone)]
pub struct MockLlmClient {
    behavior: MockBehavior,
    calls: Arc<Mutex<Vec<ChatRequest>>>,
}

impl MockLlmClient {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            behavior: MockBehavior::Ok(content.into()),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn error(error: LlmError) -> Self {
        Self {
            behavior: MockBehavior::Error(error),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().expect("mock calls mutex poisoned").len()
    }

    pub fn calls(&self) -> Vec<ChatRequest> {
        self.calls
            .lock()
            .expect("mock calls mutex poisoned")
            .clone()
    }
}

#[derive(Debug, Clone)]
enum MockBehavior {
    Ok(String),
    Error(LlmError),
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        self.calls
            .lock()
            .expect("mock calls mutex poisoned")
            .push(req);
        match &self.behavior {
            MockBehavior::Ok(content) => Ok(ChatResponse {
                content: content.clone(),
                model: Some("mock".to_string()),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            }),
            MockBehavior::Error(error) => Err(clone_error(error)),
        }
    }
}

fn clone_error(error: &LlmError) -> LlmError {
    match error {
        LlmError::MissingApiKey => LlmError::MissingApiKey,
        LlmError::Auth(msg) => LlmError::Auth(msg.clone()),
        LlmError::RateLimited(msg) => LlmError::RateLimited(msg.clone()),
        LlmError::Network(msg) => LlmError::Network(msg.clone()),
        LlmError::Timeout(duration) => LlmError::Timeout(*duration),
        LlmError::InvalidResponse(msg) => LlmError::InvalidResponse(msg.clone()),
        LlmError::HttpStatus { status, body } => LlmError::HttpStatus {
            status: *status,
            body: body.clone(),
        },
    }
}

impl Default for MockLlmClient {
    fn default() -> Self {
        Self::ok(String::new())
    }
}

#[allow(dead_code)]
fn _assert_duration_send_sync(_: Duration) {}
