use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod openai_compat;

#[cfg(test)]
pub mod mock;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, LlmError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub user: String,
    pub response_format: ResponseFormat,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    Text,
    JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub content: String,
    pub model: Option<String>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    #[error("missing API key")]
    MissingApiKey,
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
    #[error("provider returned invalid response: {0}")]
    InvalidResponse(String),
    #[error("provider error: status {status}: {body}")]
    HttpStatus { status: u16, body: String },
}

#[derive(Debug, Serialize)]
struct CompatChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct CompatChatRequest<'a> {
    model: &'a str,
    messages: [CompatChatMessage<'a>; 2],
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<CompatResponseFormat>,
}

#[derive(Debug, Serialize)]
struct CompatResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Deserialize)]
struct CompatChatResponse {
    model: Option<String>,
    choices: Vec<CompatChoice>,
    usage: Option<CompatUsage>,
}

#[derive(Debug, Deserialize)]
struct CompatChoice {
    message: CompatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct CompatResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct CompatUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

fn to_compat_request(req: &ChatRequest) -> CompatChatRequest<'_> {
    CompatChatRequest {
        model: &req.model,
        messages: [
            CompatChatMessage {
                role: "system",
                content: &req.system,
            },
            CompatChatMessage {
                role: "user",
                content: &req.user,
            },
        ],
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        response_format: match req.response_format {
            ResponseFormat::Text => None,
            ResponseFormat::JsonObject => Some(CompatResponseFormat {
                kind: "json_object",
            }),
        },
    }
}

fn from_compat_response(resp: CompatChatResponse) -> Result<ChatResponse, LlmError> {
    let content = resp
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| LlmError::InvalidResponse("missing choices[0]".to_string()))?;
    let usage = resp.usage;
    Ok(ChatResponse {
        content,
        model: resp.model,
        prompt_tokens: usage.as_ref().and_then(|u| u.prompt_tokens),
        completion_tokens: usage.as_ref().and_then(|u| u.completion_tokens),
        total_tokens: usage.and_then(|u| u.total_tokens),
    })
}
