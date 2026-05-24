use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;

use super::{
    from_compat_response, to_compat_request, ChatRequest, ChatResponse, CompatChatResponse,
    LlmClient, LlmError,
};

#[derive(Debug, Clone)]
pub struct OpenAiCompatClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OpenAiCompatClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self, LlmError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(LlmError::MissingApiKey);
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| LlmError::Network(format!("build http client: {e}")))?;
        Ok(Self {
            http,
            base_url: trim_base_url(base_url.into()),
            api_key,
        })
    }

    pub fn deepseek(api_key: impl Into<String>) -> Result<Self, LlmError> {
        Self::new("https://api.deepseek.com/v1", api_key)
    }

    pub fn dashscope(api_key: impl Into<String>) -> Result<Self, LlmError> {
        Self::new("https://dashscope.aliyuncs.com/compatible-mode/v1", api_key)
    }

    pub fn openai(api_key: impl Into<String>) -> Result<Self, LlmError> {
        Self::new("https://api.openai.com/v1", api_key)
    }

    fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

#[async_trait]
impl LlmClient for OpenAiCompatClient {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = to_compat_request(&req);
        let send = self
            .http
            .post(self.chat_completions_url())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send();

        let response = tokio::time::timeout(req.timeout, send)
            .await
            .map_err(|_| LlmError::Timeout(req.timeout))?
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout(req.timeout)
                } else {
                    LlmError::Network(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(map_status(status, body));
        }

        let parsed = response
            .json::<CompatChatResponse>()
            .await
            .map_err(|e| LlmError::InvalidResponse(e.to_string()))?;
        from_compat_response(parsed)
    }
}

fn map_status(status: StatusCode, body: String) -> LlmError {
    match status.as_u16() {
        401 | 403 => LlmError::Auth(body),
        429 => LlmError::RateLimited(body),
        _ => LlmError::HttpStatus {
            status: status.as_u16(),
            body,
        },
    }
}

fn trim_base_url(mut base_url: String) -> String {
    while base_url.ends_with('/') {
        base_url.pop();
    }
    base_url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_key() {
        assert!(matches!(
            OpenAiCompatClient::new("https://example.test/v1", ""),
            Err(LlmError::MissingApiKey)
        ));
    }

    #[test]
    fn trims_trailing_slash() {
        let client = OpenAiCompatClient::new("https://example.test/v1///", "sk-test").unwrap();
        assert_eq!(
            client.chat_completions_url(),
            "https://example.test/v1/chat/completions"
        );
    }
}
