//! cpal 后端实现。
//!
//! 选择默认输入设备，按目标 `AudioFormat` 协商最接近的设备配置；
//! 输入流回调中将 f32/i16/u16 三种样本格式统一转成 i16 PCM，
//! 通过 `mpsc` 通道发往消费者。Stream 由 `CpalStopHandle` 持有，
//! drop 时随之停止。

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::mpsc;

use crate::capture::{
    AudioCapture, AudioFormat, CaptureError, CaptureSession, PcmChunk, StopHandle,
};

/// 基于 cpal 默认 host + 默认输入设备的采集后端。
#[derive(Default)]
pub struct CpalCapture;

impl CpalCapture {
    pub fn new() -> Self {
        Self
    }
}

impl AudioCapture for CpalCapture {
    fn start(&mut self, format: AudioFormat) -> Result<CaptureSession, CaptureError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(CaptureError::NoDevice)?;

        let supported = pick_supported_config(&device, format)?;
        let sample_format = supported.sample_format();
        let actual = AudioFormat {
            sample_rate: supported.sample_rate().0,
            channels: supported.channels(),
        };
        let stream_config: StreamConfig = supported.into();

        let (tx, rx) = mpsc::channel::<PcmChunk>();
        let err_fn = |e| eprintln!("cpal stream error: {e}");

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let chunk: PcmChunk = data.iter().map(f32_to_i16).collect();
                    let _ = tx.send(chunk);
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let _ = tx.send(data.to_vec());
                },
                err_fn,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let chunk: PcmChunk = data.iter().map(|&s| u16_to_i16(s)).collect();
                    let _ = tx.send(chunk);
                },
                err_fn,
                None,
            ),
            other => {
                return Err(CaptureError::UnsupportedFormat(format!(
                    "sample format {other:?}"
                )));
            }
        }
        .map_err(|e| CaptureError::Backend(e.to_string()))?;

        stream
            .play()
            .map_err(|e| CaptureError::Backend(e.to_string()))?;

        Ok(CaptureSession {
            format: actual,
            rx,
            stop: Box::new(CpalStopHandle { _stream: stream }),
        })
    }
}

/// 从设备支持的配置里挑一个最贴近请求的：通道数完全匹配，采样率落在
/// `[min, max]` 内则用请求值，否则取最近边界。挑不到则返回错误。
fn pick_supported_config(
    device: &cpal::Device,
    requested: AudioFormat,
) -> Result<cpal::SupportedStreamConfig, CaptureError> {
    let configs = device
        .supported_input_configs()
        .map_err(|e| CaptureError::Backend(e.to_string()))?;

    let mut best: Option<cpal::SupportedStreamConfigRange> = None;
    for cfg in configs {
        if cfg.channels() != requested.channels {
            continue;
        }
        if !matches!(
            cfg.sample_format(),
            SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
        ) {
            continue;
        }
        best = Some(cfg);
        break;
    }

    let range = best.ok_or_else(|| {
        CaptureError::UnsupportedFormat(format!(
            "no input config matches channels={}",
            requested.channels
        ))
    })?;

    let req_rate = cpal::SampleRate(requested.sample_rate);
    let chosen_rate = if req_rate >= range.min_sample_rate() && req_rate <= range.max_sample_rate()
    {
        req_rate
    } else if req_rate < range.min_sample_rate() {
        range.min_sample_rate()
    } else {
        range.max_sample_rate()
    };

    Ok(range.with_sample_rate(chosen_rate))
}

fn f32_to_i16(s: &f32) -> i16 {
    let clamped = s.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32) as i16
}

fn u16_to_i16(s: u16) -> i16 {
    (s as i32 - 32768) as i16
}

struct CpalStopHandle {
    _stream: Stream,
}

impl StopHandle for CpalStopHandle {}
