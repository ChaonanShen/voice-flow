//! DashScope client setup: API key、端点 URL、auth 头构造。
//!
//! Paraformer-realtime-v2 是 WebSocket 流式接口；本模块只负责认证 / 端点 /
//! 错误映射等"非网络"工作，握手与协议封装在 PR 8.3 落地。
//!
//! 参考：<https://help.aliyun.com/zh/dashscope/developer-reference/api-details>

use crate::{CloudAsrError, CloudEngineConfig, CloudProvider};

/// 与 DashScope 官方 SDK 一致的 API key 环境变量名。
pub const DASHSCOPE_API_KEY_ENV: &str = "DASHSCOPE_API_KEY";

/// 流式推理 WebSocket 端点。
pub const DASHSCOPE_DEFAULT_WS_ENDPOINT: &str =
    "wss://dashscope.aliyuncs.com/api-ws/v1/inference";

/// Paraformer-realtime-v2 模型 ID。
pub const PARAFORMER_REALTIME_V2_MODEL: &str = "paraformer-realtime-v2";

/// 认证头名称。
pub const AUTH_HEADER: &str = "Authorization";
pub const DATA_INSPECTION_HEADER: &str = "X-DashScope-DataInspection";

/// 一个已配置好的 DashScope 客户端，提供端点与认证头给后续 WebSocket 握手使用。
#[derive(Debug, Clone)]
pub struct DashScopeClient {
    api_key: String,
    ws_endpoint: String,
}

impl DashScopeClient {
    /// 用显式 API key 构造客户端。空 key 会被立即拒绝，避免在握手时才报错。
    pub fn new(api_key: impl Into<String>) -> Result<Self, CloudAsrError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(CloudAsrError::MissingApiKey);
        }
        Ok(Self {
            api_key,
            ws_endpoint: DASHSCOPE_DEFAULT_WS_ENDPOINT.to_string(),
        })
    }

    /// 从 [`DASHSCOPE_API_KEY_ENV`] 读取 API key。
    pub fn from_env() -> Result<Self, CloudAsrError> {
        let key = std::env::var(DASHSCOPE_API_KEY_ENV).map_err(|_| CloudAsrError::MissingApiKey)?;
        Self::new(key)
    }

    /// 从 [`CloudEngineConfig`] 还原客户端；仅在 provider 为 DashScope 时合法。
    pub fn from_config(config: &CloudEngineConfig) -> Result<Self, CloudAsrError> {
        if config.provider != CloudProvider::DashScope {
            return Err(CloudAsrError::Protocol(format!(
                "expected dashscope provider, got {}",
                config.provider.label()
            )));
        }
        Self::new(config.api_key.clone())
    }

    /// 覆盖默认 WebSocket 端点（用于自定义网关 / mock 测试）。
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.ws_endpoint = endpoint.into();
        self
    }

    pub fn ws_endpoint(&self) -> &str {
        &self.ws_endpoint
    }

    /// WebSocket 握手必须携带的请求头列表。
    pub fn auth_headers(&self) -> Vec<(&'static str, String)> {
        vec![
            (AUTH_HEADER, format!("Bearer {}", self.api_key)),
            (DATA_INSPECTION_HEADER, "enable".to_string()),
        ]
    }
}

/// 把 HTTP 状态码映射成统一 [`CloudAsrError`]，供 WebSocket 握手失败路径复用。
pub fn map_http_status(status: u16, body: &str) -> CloudAsrError {
    match status {
        401 | 403 => CloudAsrError::Auth(format!("status {status}: {body}")),
        408 | 504 => CloudAsrError::Network(format!("status {status}: {body}")),
        429 => CloudAsrError::Network(format!("status {status} (rate limited): {body}")),
        500..=599 => CloudAsrError::Network(format!("status {status}: {body}")),
        400 => CloudAsrError::Protocol(format!("status {status}: {body}")),
        _ => CloudAsrError::Decode(format!("status {status}: {body}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_api_key() {
        assert!(matches!(
            DashScopeClient::new(""),
            Err(CloudAsrError::MissingApiKey)
        ));
        assert!(matches!(
            DashScopeClient::new("   "),
            Err(CloudAsrError::MissingApiKey)
        ));
    }

    #[test]
    fn accepts_non_empty_key_with_default_endpoint() {
        let client = DashScopeClient::new("sk-test").unwrap();
        assert_eq!(client.ws_endpoint(), DASHSCOPE_DEFAULT_WS_ENDPOINT);
    }

    #[test]
    fn auth_headers_contain_bearer_and_inspection() {
        let client = DashScopeClient::new("sk-test").unwrap();
        let headers = client.auth_headers();

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0, AUTH_HEADER);
        assert_eq!(headers[0].1, "Bearer sk-test");
        assert_eq!(headers[1].0, DATA_INSPECTION_HEADER);
        assert_eq!(headers[1].1, "enable");
    }

    #[test]
    fn with_endpoint_overrides_default() {
        let client = DashScopeClient::new("sk-test")
            .unwrap()
            .with_endpoint("wss://example.test/ws");
        assert_eq!(client.ws_endpoint(), "wss://example.test/ws");
    }

    #[test]
    fn from_config_requires_dashscope_provider() {
        let config = CloudEngineConfig::dashscope("sk-test");
        assert!(DashScopeClient::from_config(&config).is_ok());
    }

    #[test]
    fn from_config_rejects_empty_api_key() {
        let config = CloudEngineConfig::dashscope("");
        assert!(matches!(
            DashScopeClient::from_config(&config),
            Err(CloudAsrError::MissingApiKey)
        ));
    }

    #[test]
    fn http_status_maps_auth_failures() {
        assert!(matches!(
            map_http_status(401, "Unauthorized"),
            CloudAsrError::Auth(_)
        ));
        assert!(matches!(
            map_http_status(403, "Forbidden"),
            CloudAsrError::Auth(_)
        ));
    }

    #[test]
    fn http_status_maps_5xx_as_network() {
        assert!(matches!(
            map_http_status(500, "boom"),
            CloudAsrError::Network(_)
        ));
        assert!(matches!(
            map_http_status(503, "down"),
            CloudAsrError::Network(_)
        ));
    }

    #[test]
    fn http_status_maps_400_as_protocol() {
        assert!(matches!(
            map_http_status(400, "bad json"),
            CloudAsrError::Protocol(_)
        ));
    }

    #[test]
    fn http_status_maps_429_as_network_rate_limited() {
        let err = map_http_status(429, "Too Many Requests");
        match err {
            CloudAsrError::Network(msg) => assert!(msg.contains("rate limited")),
            other => panic!("expected Network, got {other:?}"),
        }
    }
}
