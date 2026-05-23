//! 文件回放采集后端。
//!
//! 把一段 WAV 文件按真实采样率节奏切成 PCM chunk，通过通道发送，
//! 模拟实时录音。用于无声卡环境下的端到端测试与可复现 demo。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use crate::capture::{
    AudioCapture, AudioFormat, CaptureError, CaptureSession, PcmChunk, StopHandle,
};
use crate::wav::{read_pcm16_wav, WavError};

/// 默认 chunk 时长：100ms。与 cpal 默认 buffer 数量级接近，便于流式 ASR。
const DEFAULT_CHUNK_MS: u32 = 100;

/// 从 WAV 文件回放的采集后端。
pub struct FileCapture {
    samples: Vec<i16>,
    format: AudioFormat,
    chunk_ms: u32,
}

impl FileCapture {
    /// 加载一个 PCM 16-bit WAV 文件作为回放源。
    pub fn from_wav(path: impl AsRef<Path>) -> Result<Self, CaptureError> {
        let (format, samples) = read_pcm16_wav(path).map_err(wav_err)?;
        Ok(Self {
            samples,
            format,
            chunk_ms: DEFAULT_CHUNK_MS,
        })
    }

    /// 自定义 chunk 长度（毫秒）。最短 1ms，过短会在低频设备上凑不出整数样本。
    pub fn with_chunk_ms(mut self, ms: u32) -> Self {
        self.chunk_ms = ms.max(1);
        self
    }
}

impl AudioCapture for FileCapture {
    fn start(&mut self, requested: AudioFormat) -> Result<CaptureSession, CaptureError> {
        if requested.channels != self.format.channels {
            return Err(CaptureError::UnsupportedFormat(format!(
                "file has {} channels, requested {}",
                self.format.channels, requested.channels
            )));
        }
        if requested.sample_rate != self.format.sample_rate {
            return Err(CaptureError::UnsupportedFormat(format!(
                "file is {} Hz, requested {} Hz (no resampling)",
                self.format.sample_rate, requested.sample_rate
            )));
        }

        let format = self.format;
        let frames_per_chunk =
            ((format.sample_rate as u64 * self.chunk_ms as u64) / 1000).max(1) as usize;
        let samples_per_chunk = frames_per_chunk * format.channels as usize;
        let chunk_dur = Duration::from_millis(self.chunk_ms as u64);

        let stop_flag = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<PcmChunk>();
        let samples = std::mem::take(&mut self.samples);
        let stop_for_thread = Arc::clone(&stop_flag);

        thread::spawn(move || {
            let mut next_send = Instant::now();
            for chunk in samples.chunks(samples_per_chunk) {
                if stop_for_thread.load(Ordering::Relaxed) {
                    return;
                }
                let now = Instant::now();
                if next_send > now {
                    thread::sleep(next_send - now);
                }
                if tx.send(chunk.to_vec()).is_err() {
                    return;
                }
                next_send += chunk_dur;
            }
        });

        Ok(CaptureSession {
            format,
            rx,
            stop: Box::new(FileStopHandle { flag: stop_flag }),
        })
    }
}

struct FileStopHandle {
    flag: Arc<AtomicBool>,
}

impl Drop for FileStopHandle {
    fn drop(&mut self) {
        self.flag.store(true, Ordering::Relaxed);
    }
}

impl StopHandle for FileStopHandle {}

fn wav_err(e: WavError) -> CaptureError {
    match e {
        WavError::Io(io) => CaptureError::Backend(io.to_string()),
        other => CaptureError::UnsupportedFormat(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::write_pcm16_wav_to;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_wav(format: AudioFormat, samples: &[i16]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        let mut buf = Vec::new();
        write_pcm16_wav_to(&mut buf, format, samples).unwrap();
        f.write_all(&buf).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn streams_all_samples_in_order() {
        let format = AudioFormat {
            sample_rate: 16_000,
            channels: 1,
        };
        let samples: Vec<i16> = (0..4_800).map(|i| i as i16).collect();
        let file = write_temp_wav(format, &samples);

        let mut cap = FileCapture::from_wav(file.path())
            .unwrap()
            .with_chunk_ms(10);
        let session = cap.start(format).unwrap();

        let mut received = Vec::new();
        while let Ok(chunk) = session.rx.recv_timeout(Duration::from_secs(2)) {
            received.extend(chunk);
        }
        assert_eq!(received, samples);
    }

    #[test]
    fn rejects_format_mismatch() {
        let format = AudioFormat {
            sample_rate: 16_000,
            channels: 1,
        };
        let file = write_temp_wav(format, &[0i16; 16]);
        let mut cap = FileCapture::from_wav(file.path()).unwrap();
        let res = cap.start(AudioFormat {
            sample_rate: 48_000,
            channels: 1,
        });
        assert!(matches!(res, Err(CaptureError::UnsupportedFormat(_))));
    }

    #[test]
    fn drop_stops_playback_early() {
        let format = AudioFormat {
            sample_rate: 16_000,
            channels: 1,
        };
        // 1 秒音频
        let samples: Vec<i16> = vec![0; 16_000];
        let file = write_temp_wav(format, &samples);
        let mut cap = FileCapture::from_wav(file.path())
            .unwrap()
            .with_chunk_ms(100);
        let session = cap.start(format).unwrap();
        // 收一个 chunk 就主动 drop，不应阻塞
        let _first = session.rx.recv_timeout(Duration::from_millis(500)).unwrap();
        drop(session);
    }
}
