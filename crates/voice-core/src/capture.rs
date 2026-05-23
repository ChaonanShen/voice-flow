//! 音频采集抽象。
//!
//! 后端无关的 PCM 捕获接口，供 cpal / 测试 mock / 未来其他后端实现。
//! 实现见后续 PR；本 PR 只定义形状。

use std::sync::mpsc::Receiver;

/// 单个 PCM 样本：交错存储的 16-bit 有符号整数。
pub type PcmSample = i16;

/// 一帧 PCM 数据：交错存储 `channels` 个声道，长度为 `frames * channels`。
pub type PcmChunk = Vec<PcmSample>;

/// 采集后端报告的音频参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// 音频采集后端。
///
/// 典型生命周期：`start` 返回数据通道与运行中的句柄；调用方持有 `Receiver`
/// 拉取 PCM，结束时 drop 句柄触发停止。后端实现应保证 drop 时干净关闭设备。
pub trait AudioCapture {
    /// 启动采集，返回 PCM 接收端与负责停止的句柄。
    ///
    /// `format` 为请求的音频格式；若设备不支持，实现可选择返回错误或
    /// 报告实际使用的格式（具体策略由实现定义，调用方应再次读取 `format()`）。
    fn start(&mut self, format: AudioFormat) -> Result<CaptureSession, CaptureError>;
}

/// 一次采集会话。drop 即停止。
pub struct CaptureSession {
    /// 实际生效的音频格式（可能与请求不同）。
    pub format: AudioFormat,
    /// PCM chunk 接收端。
    pub rx: Receiver<PcmChunk>,
    /// 后端持有的运行中资源；drop 后停止采集。
    pub _stop: Box<dyn StopHandle>,
}

/// 采集会话的停止句柄。drop 时应停止底层流。
pub trait StopHandle: Send {}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no input device available")]
    NoDevice,
    #[error("device does not support requested format: {0}")]
    UnsupportedFormat(String),
    #[error("backend error: {0}")]
    Backend(String),
}
