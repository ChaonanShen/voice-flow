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
        let mut last_error = None;
        for attempt in 0..=1 {
            match self.send_once(&req).await {
                Ok(response) => return Ok(response),
                Err(error) if should_retry(&error) && attempt == 0 => {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }

        Err(last_error.expect("retry loop always stores a retryable error"))
    }
}

impl OpenAiCompatClient {
    async fn send_once(&self, req: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = to_compat_request(req);
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

fn should_retry(error: &LlmError) -> bool {
    matches!(
        error,
        LlmError::HttpStatus {
            status: 500..=599,
            ..
        }
    )
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
    use crate::llm::{LlmClient, ResponseFormat};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

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

    #[tokio::test]
    async fn retries_once_on_5xx() {
        let ok_body = serde_json::json!({
            "model": "mock-model",
            "choices": [{"message": {"content": "改写完成"}}],
            "usage": {"prompt_tokens": 2, "completion_tokens": 3, "total_tokens": 5}
        })
        .to_string();
        let (base_url, attempts, server) =
            spawn_responses([(500, "temporary failure".to_string()), (200, ok_body)]);
        let client = OpenAiCompatClient::new(base_url, "sk-test").unwrap();

        let response = client
            .complete(ChatRequest {
                model: "test-model".to_string(),
                system: "system".to_string(),
                user: "user".to_string(),
                response_format: ResponseFormat::Text,
                temperature: 0.3,
                max_tokens: 200,
                timeout: Duration::from_secs(2),
            })
            .await
            .unwrap();

        assert_eq!(response.content, "改写完成");
        assert_eq!(response.model.as_deref(), Some("mock-model"));
        assert_eq!(response.total_tokens, Some(5));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn does_not_retry_auth_errors() {
        let (base_url, attempts, server) = spawn_responses([(401, "bad key".to_string())]);
        let client = OpenAiCompatClient::new(base_url, "sk-test").unwrap();

        let err = client
            .complete(ChatRequest {
                model: "test-model".to_string(),
                system: "system".to_string(),
                user: "user".to_string(),
                response_format: ResponseFormat::Text,
                temperature: 0.3,
                max_tokens: 200,
                timeout: Duration::from_secs(2),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, LlmError::Auth(body) if body == "bad key"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        server.join().unwrap();
    }

    fn spawn_responses<const N: usize>(
        responses: [(u16, String); N],
    ) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let thread_attempts = Arc::clone(&attempts);
        let handle = thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                read_request(&mut stream);
                thread_attempts.fetch_add(1, Ordering::SeqCst);
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {len}\r\n\
                     Connection: close\r\n\
                     \r\n\
                     {body}",
                    reason = reason_phrase(status),
                    len = body.len(),
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (format!("http://{addr}/v1"), attempts, handle)
    }

    fn read_request(stream: &mut std::net::TcpStream) {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..read]);
            if request_complete(&buf) {
                break;
            }
        }
    }

    fn request_complete(buf: &[u8]) -> bool {
        let Some(header_end) = find_headers_end(buf) else {
            return false;
        };
        let headers = String::from_utf8_lossy(&buf[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then_some(value)
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        buf.len() >= header_end + 4 + content_length
    }

    fn find_headers_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn reason_phrase(status: u16) -> &'static str {
        match status {
            200 => "OK",
            401 => "Unauthorized",
            500 => "Internal Server Error",
            _ => "Status",
        }
    }
}
