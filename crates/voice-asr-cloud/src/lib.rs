//! voice-asr-cloud
//!
//! 云端 ASR 引擎实现。当前目标提供商：阿里云 DashScope
//! Paraformer-realtime-v2（流式 WebSocket）。
//!
//! 本 crate 在 PR 8.1 只搭骨架：定义共享的错误类型、provider 占位枚举、
//! 配置类型。HTTP/WebSocket 客户端、协议封装、`AsrEngine` 实现分别在
//! PR 8.2 / 8.3 / 8.4 落地。

use std::time::Duration;

pub mod dashscope;
pub mod protocol;

/// 云端 ASR 厂商。当前只有 DashScope；后续可加其他实现。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider {
    DashScope,
}

impl CloudProvider {
    pub fn label(self) -> &'static str {
        match self {
            Self::DashScope => "dashscope",
        }
    }
}

/// 云端引擎初始化参数。具体字段在后续 PR 按需扩展。
#[derive(Debug, Clone)]
pub struct CloudEngineConfig {
    pub provider: CloudProvider,
    pub api_key: String,
    pub request_timeout: Duration,
}

impl CloudEngineConfig {
    pub fn dashscope(api_key: impl Into<String>) -> Self {
        Self {
            provider: CloudProvider::DashScope,
            api_key: api_key.into(),
            request_timeout: Duration::from_secs(30),
        }
    }
}

/// 云端 ASR 错误。统一收敛 HTTP / WebSocket / 协议解析的失败模式。
///
/// 具体调用方在 PR 8.4 把这里的错误映射到 [`voice_core::asr::AsrError`]，
/// 避免把 reqwest / tungstenite 等具体依赖暴露给上层。
#[derive(Debug, thiserror::Error)]
pub enum CloudAsrError {
    #[error("missing API key")]
    MissingApiKey,
    #[error("network error: {0}")]
    Network(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashscope_label_is_stable() {
        assert_eq!(CloudProvider::DashScope.label(), "dashscope");
    }

    #[test]
    fn dashscope_config_uses_default_timeout() {
        let config = CloudEngineConfig::dashscope("sk-xxx");
        assert_eq!(config.provider, CloudProvider::DashScope);
        assert_eq!(config.api_key, "sk-xxx");
        assert_eq!(config.request_timeout, Duration::from_secs(30));
    }

    #[test]
    fn error_display_includes_context() {
        let err = CloudAsrError::Auth("invalid api key".into());
        assert_eq!(err.to_string(), "authentication failed: invalid api key");

        let err = CloudAsrError::Timeout(Duration::from_secs(15));
        assert_eq!(err.to_string(), "request timed out after 15s");
    }

    #[test]
    fn missing_key_error_message_is_actionable() {
        assert_eq!(CloudAsrError::MissingApiKey.to_string(), "missing API key");
    }
}
