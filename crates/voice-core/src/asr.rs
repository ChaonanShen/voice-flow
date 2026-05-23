//! ASR 引擎抽象。
//!
//! `AsrEngine` 是同步阻塞接口：吃一段交错存储的 16-bit PCM，吐识别文本。
//! 第一版只覆盖"非流式"路径——把整段 PCM 一次性送进引擎。流式路径在
//! 后续 PR 通过独立的 `StreamingSession` 类型扩展，不破坏当前 trait。
//!
//! 设计要点：
//! - PCM 输入是 `&[i16]`，与 [`crate::capture::PcmSample`] 对齐。
//!   引擎实现负责自行转换为底层 SDK 需要的 f32 / mono / 重采样等。
//! - 错误用 `AsrError` 收敛；对外不暴露具体引擎的错误类型，便于
//!   未来端侧 / 云端引擎共用同一个 trait。
//! - trait 是 dyn-safe（无泛型 / 无关联类型），方便 `Box<dyn AsrEngine>`
//!   在运行时按用户配置切换引擎。

use crate::capture::AudioFormat;

/// ASR 错误。
#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    /// 模型文件缺失或路径错误。
    #[error("model not found: {0}")]
    ModelNotFound(String),
    /// 模型加载 / 初始化失败（解析、签名不匹配等）。
    #[error("model load failed: {0}")]
    ModelLoad(String),
    /// 输入 PCM 格式不被引擎接受（采样率 / 声道数等）。
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    /// 解码运行时错误。
    #[error("decode failed: {0}")]
    Decode(String),
    /// 网络相关（云端引擎专用，端侧引擎不会用到）。
    #[error("network error: {0}")]
    Network(String),
}

/// 同步、非流式 ASR 引擎。
///
/// 实现要求 `Send + Sync`：CLI / 桌面外壳会在后台线程持有引擎，
/// 调用方可能跨线程共享同一个引擎实例（多次串行调用 `transcribe`）。
pub trait AsrEngine: Send + Sync {
    /// 把整段 PCM 转写为文本。
    ///
    /// `pcm` 是按 `format.channels` 交错存储的 16-bit 样本；
    /// 长度需为 `format.channels` 的整数倍。
    /// 实现可以选择内部缓存复用，但 trait 不强制无状态。
    fn transcribe(&self, pcm: &[i16], format: AudioFormat) -> Result<String, AsrError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_includes_context() {
        let e = AsrError::ModelNotFound("/no/such/path".to_string());
        assert_eq!(e.to_string(), "model not found: /no/such/path");

        let e = AsrError::UnsupportedFormat("48 kHz stereo".into());
        assert_eq!(e.to_string(), "unsupported audio format: 48 kHz stereo");
    }

    #[test]
    fn trait_is_object_safe() {
        // 编译期断言：能 box 起来才算 dyn-safe。
        struct Dummy;
        impl AsrEngine for Dummy {
            fn transcribe(&self, _: &[i16], _: AudioFormat) -> Result<String, AsrError> {
                Ok(String::new())
            }
        }
        let _: Box<dyn AsrEngine> = Box::new(Dummy);
    }
}
