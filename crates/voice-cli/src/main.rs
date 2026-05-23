use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use voice_asr_local::{StreamingZipformer, MODEL_DIR_ENV};
use voice_core::asr::AsrEngine;
use voice_core::capture::{AudioCapture, AudioFormat};
use voice_core::cpal_backend::CpalCapture;
use voice_core::file_backend::FileCapture;
use voice_core::wav::{read_pcm16_wav, write_pcm16_wav};

#[derive(Parser)]
#[command(name = "voice-cli", version, about = "xengineer voice input CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 录音到 WAV 文件。
    Record {
        /// 输出 WAV 文件路径。
        output: PathBuf,
        /// 录制时长，例如 `5s` / `1500ms` / `1m`。
        #[arg(long, default_value = "5s")]
        duration: humantime::Duration,
        /// 采样率（Hz）。设备不支持时会回退到最近值。
        #[arg(long, default_value_t = 16_000)]
        sample_rate: u32,
        /// 声道数。
        #[arg(long, default_value_t = 1)]
        channels: u16,
        /// 用 WAV 文件替代真实麦克风（适合无声卡环境与 demo 复现）。
        /// 提供该参数时 `--sample-rate`/`--channels` 必须与文件一致。
        #[arg(long)]
        input: Option<PathBuf>,
    },
    /// 使用端侧 ASR 识别 PCM 16-bit WAV 文件。
    Transcribe {
        /// 输入 WAV 文件路径。
        input: PathBuf,
        /// sherpa-onnx Streaming Zipformer 模型目录。未提供时读取 XENGINEER_SHERPA_ZIPFORMER_MODEL_DIR。
        #[arg(long)]
        model_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Record {
            output,
            duration,
            sample_rate,
            channels,
            input,
        } => record(output, duration.into(), sample_rate, channels, input),
        Command::Transcribe { input, model_dir } => transcribe(input, model_dir),
    }
}

fn record(
    output: PathBuf,
    duration: Duration,
    sample_rate: u32,
    channels: u16,
    input: Option<PathBuf>,
) -> Result<()> {
    let mut backend: Box<dyn AudioCapture> = match input {
        Some(path) => {
            eprintln!("source: file {}", path.display());
            Box::new(
                FileCapture::from_wav(&path)
                    .with_context(|| format!("failed to load {}", path.display()))?,
            )
        }
        None => {
            eprintln!("source: default microphone (cpal)");
            Box::new(CpalCapture::new())
        }
    };
    let session = backend
        .start(AudioFormat {
            sample_rate,
            channels,
        })
        .context("failed to start audio capture")?;

    let actual = session.format;
    if actual.sample_rate != sample_rate || actual.channels != channels {
        eprintln!(
            "backend negotiated {} Hz / {} ch (requested {} Hz / {} ch)",
            actual.sample_rate, actual.channels, sample_rate, channels
        );
    }
    eprintln!("recording for {duration:?}, press Ctrl+C to abort early");

    let mut samples: Vec<i16> = Vec::with_capacity(
        (actual.sample_rate as usize) * (actual.channels as usize) * duration.as_secs() as usize,
    );
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match session.rx.recv_timeout(remaining) {
            Ok(chunk) => samples.extend_from_slice(&chunk),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // 对 FileCapture，这意味着回放结束，是正常路径。
                break;
            }
        }
    }

    drop(session);

    write_pcm16_wav(&output, actual, &samples)
        .with_context(|| format!("failed to write WAV to {}", output.display()))?;

    let secs = samples.len() as f64 / (actual.sample_rate as f64 * actual.channels as f64);
    eprintln!(
        "wrote {} ({:.2}s, {} samples) to {}",
        humansize(samples.len() * 2),
        secs,
        samples.len(),
        output.display()
    );
    Ok(())
}

fn transcribe(input: PathBuf, model_dir: Option<PathBuf>) -> Result<()> {
    let model_dir = model_dir
        .or_else(|| std::env::var_os(MODEL_DIR_ENV).map(PathBuf::from))
        .with_context(|| format!("missing --model-dir or {MODEL_DIR_ENV}"))?;

    let (format, samples) = read_pcm16_wav(&input)
        .with_context(|| format!("failed to read WAV from {}", input.display()))?;
    eprintln!(
        "transcribing {} ({:.2}s, {} Hz / {} ch) with {}",
        input.display(),
        samples.len() as f64 / (format.sample_rate as f64 * format.channels as f64),
        format.sample_rate,
        format.channels,
        model_dir.display()
    );

    let engine = StreamingZipformer::from_model_dir(&model_dir)
        .with_context(|| format!("failed to load model from {}", model_dir.display()))?;
    let text = engine
        .transcribe(&samples, format)
        .context("failed to transcribe WAV")?;
    println!("{text}");
    Ok(())
}

fn humansize(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    if bytes >= MB {
        format!("{:.2} MiB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KiB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
