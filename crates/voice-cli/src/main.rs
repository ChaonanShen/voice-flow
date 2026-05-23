use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use voice_asr_cloud::dashscope::DASHSCOPE_API_KEY_ENV;
use voice_asr_cloud::{CloudEngineConfig, ParaformerCloudEngine};
use voice_asr_local::{StreamingZipformer, MODEL_DIR_ENV};
use voice_core::asr::AsrEngine;
use voice_core::capture::{AudioCapture, AudioFormat};
use voice_core::clipboard::{ClipboardWriter, SystemClipboard};
use voice_core::cpal_backend::CpalCapture;
use voice_core::file_backend::FileCapture;
use voice_core::hotkey::{PushToTalkHotkey, PUSH_TO_TALK_HOTKEY_LABEL};
use voice_core::paste::{PasteSimulator, SystemPaste};
use voice_core::push_to_talk::{PushToTalkRecorder, PushToTalkRecorderEvent};
use voice_core::state::{RealtimeState, RealtimeStateEvent};
use voice_core::wav::{read_pcm16_wav, write_pcm16_wav};

#[derive(Parser)]
#[command(name = "voice-cli", version, about = "voice-flow voice input CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum EngineKind {
    Local,
    Cloud,
}

impl EngineKind {
    fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
        }
    }
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
    /// 识别 PCM 16-bit WAV 文件。默认走端侧；`--engine cloud` 走 DashScope。
    Transcribe {
        /// 输入 WAV 文件路径。
        input: PathBuf,
        /// 选择 ASR 引擎：`local`（默认）或 `cloud`（DashScope Paraformer-realtime-v2）。
        #[arg(long, value_enum, default_value_t = EngineKind::Local)]
        engine: EngineKind,
        /// sherpa-onnx Streaming Zipformer 模型目录（仅 `--engine local` 使用）。
        /// 未提供时读取 VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR。
        #[arg(long)]
        model_dir: Option<PathBuf>,
        /// DashScope API key（仅 `--engine cloud` 使用）。未提供时读取 DASHSCOPE_API_KEY。
        #[arg(long)]
        api_key: Option<String>,
    },
    /// 注册默认全局快捷键并打印按下/松开事件。
    ListenHotkey,
    /// 按住默认快捷键录音，松开后写入 WAV 文件。
    PushToTalkRecord {
        /// 输出 WAV 文件路径。
        output: PathBuf,
        /// 采样率（Hz）。设备不支持时会回退到最近值。
        #[arg(long, default_value_t = 16_000)]
        sample_rate: u32,
        /// 声道数。
        #[arg(long, default_value_t = 1)]
        channels: u16,
        /// 用 WAV 文件替代真实麦克风（适合无声卡环境与 demo 复现）。
        #[arg(long)]
        input: Option<PathBuf>,
    },
    /// 按住默认快捷键录音，松开后使用端侧 ASR 打印文本。
    PushToTalkTranscribe {
        /// sherpa-onnx Streaming Zipformer 模型目录。未提供时读取 VOICE_FLOW_SHERPA_ZIPFORMER_MODEL_DIR。
        #[arg(long)]
        model_dir: Option<PathBuf>,
        /// 采样率（Hz）。设备不支持时会回退到最近值。
        #[arg(long, default_value_t = 16_000)]
        sample_rate: u32,
        /// 声道数。
        #[arg(long, default_value_t = 1)]
        channels: u16,
        /// 用 WAV 文件替代真实麦克风（适合无声卡环境与 demo 复现）。
        #[arg(long)]
        input: Option<PathBuf>,
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
        Command::Transcribe {
            input,
            engine,
            model_dir,
            api_key,
        } => transcribe(input, engine, model_dir, api_key),
        Command::ListenHotkey => listen_hotkey(),
        Command::PushToTalkRecord {
            output,
            sample_rate,
            channels,
            input,
        } => push_to_talk_record(output, sample_rate, channels, input),
        Command::PushToTalkTranscribe {
            model_dir,
            sample_rate,
            channels,
            input,
        } => push_to_talk_transcribe(model_dir, sample_rate, channels, input),
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

fn transcribe(
    input: PathBuf,
    engine_kind: EngineKind,
    model_dir: Option<PathBuf>,
    api_key: Option<String>,
) -> Result<()> {
    let state = RealtimeStateEvent::new(RealtimeState::Transcribing);
    eprintln!("state: {}", state.state.label());
    let (format, samples) = read_pcm16_wav(&input)
        .with_context(|| format!("failed to read WAV from {}", input.display()))?;
    eprintln!(
        "transcribing {} ({:.2}s, {} Hz / {} ch) with engine={}",
        input.display(),
        samples.len() as f64 / (format.sample_rate as f64 * format.channels as f64),
        format.sample_rate,
        format.channels,
        engine_kind.label(),
    );

    let engine = build_engine(engine_kind, model_dir, api_key)?;
    let text = engine
        .transcribe(&samples, format)
        .context("failed to transcribe WAV")?;
    let completed = RealtimeStateEvent::new(RealtimeState::Completed).with_transcript(&text);
    eprintln!("state: {}", completed.state.label());
    println!("{text}");
    Ok(())
}

fn build_engine(
    kind: EngineKind,
    model_dir: Option<PathBuf>,
    api_key: Option<String>,
) -> Result<Box<dyn AsrEngine>> {
    match kind {
        EngineKind::Local => {
            let model_dir = model_dir
                .or_else(|| std::env::var_os(MODEL_DIR_ENV).map(PathBuf::from))
                .with_context(|| format!("missing --model-dir or {MODEL_DIR_ENV}"))?;
            eprintln!("local model dir: {}", model_dir.display());
            let engine = StreamingZipformer::from_model_dir(&model_dir)
                .with_context(|| format!("failed to load model from {}", model_dir.display()))?;
            Ok(Box::new(engine))
        }
        EngineKind::Cloud => {
            let api_key = api_key
                .or_else(|| std::env::var(DASHSCOPE_API_KEY_ENV).ok())
                .filter(|k| !k.trim().is_empty())
                .with_context(|| format!("missing --api-key or {DASHSCOPE_API_KEY_ENV}"))?;
            let config = CloudEngineConfig::dashscope(api_key);
            let engine = ParaformerCloudEngine::from_config(&config)
                .context("failed to initialize cloud engine")?;
            eprintln!("cloud provider: dashscope paraformer-realtime-v2");
            Ok(Box::new(engine))
        }
    }
}

fn listen_hotkey() -> Result<()> {
    eprintln!("registering global hotkey: {PUSH_TO_TALK_HOTKEY_LABEL}");
    eprintln!("press Ctrl+C to stop");

    let hotkey = PushToTalkHotkey::register_default().context("failed to register hotkey")?;
    loop {
        if let Some(event) = hotkey.try_recv().context("failed to read hotkey event")? {
            println!("{PUSH_TO_TALK_HOTKEY_LABEL} {event}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn push_to_talk_record(
    output: PathBuf,
    sample_rate: u32,
    channels: u16,
    input: Option<PathBuf>,
) -> Result<()> {
    let format = AudioFormat {
        sample_rate,
        channels,
    };
    let backend = capture_backend(input)?;
    let hotkey = PushToTalkHotkey::register_default().context("failed to register hotkey")?;
    let mut recorder = PushToTalkRecorder::new(backend, format);

    eprintln!(
        "hold {PUSH_TO_TALK_HOTKEY_LABEL} to record, release to write {}",
        output.display()
    );
    loop {
        recorder.poll_audio();
        if let Some(event) = hotkey.try_recv().context("failed to read hotkey event")? {
            match recorder
                .handle_hotkey_event(event)
                .context("failed to handle push-to-talk recording")?
            {
                Some(PushToTalkRecorderEvent::RecordingStarted(actual)) => {
                    eprintln!(
                        "recording started ({} Hz / {} ch)",
                        actual.sample_rate, actual.channels
                    );
                }
                Some(PushToTalkRecorderEvent::RecordingStopped(audio)) => {
                    write_pcm16_wav(&output, audio.format, &audio.samples)
                        .with_context(|| format!("failed to write WAV to {}", output.display()))?;
                    let secs = audio.samples.len() as f64
                        / (audio.format.sample_rate as f64 * audio.format.channels as f64);
                    eprintln!(
                        "recording stopped; wrote {} ({:.2}s, {} samples) to {}",
                        humansize(audio.samples.len() * 2),
                        secs,
                        audio.samples.len(),
                        output.display()
                    );
                    return Ok(());
                }
                None => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn push_to_talk_transcribe(
    model_dir: Option<PathBuf>,
    sample_rate: u32,
    channels: u16,
    input: Option<PathBuf>,
) -> Result<()> {
    let model_dir = model_dir
        .or_else(|| std::env::var_os(MODEL_DIR_ENV).map(PathBuf::from))
        .with_context(|| format!("missing --model-dir or {MODEL_DIR_ENV}"))?;
    let engine = StreamingZipformer::from_model_dir(&model_dir)
        .with_context(|| format!("failed to load model from {}", model_dir.display()))?;

    let format = AudioFormat {
        sample_rate,
        channels,
    };
    let backend = capture_backend(input)?;
    let hotkey = PushToTalkHotkey::register_default().context("failed to register hotkey")?;
    let mut recorder = PushToTalkRecorder::new(backend, format);

    eprintln!("state: {}", RealtimeState::Idle.label());
    eprintln!(
        "hold {PUSH_TO_TALK_HOTKEY_LABEL} to record, release to transcribe with {}",
        model_dir.display()
    );
    loop {
        recorder.poll_audio();
        if let Some(event) = hotkey.try_recv().context("failed to read hotkey event")? {
            match recorder
                .handle_hotkey_event(event)
                .context("failed to handle push-to-talk recording")?
            {
                Some(PushToTalkRecorderEvent::RecordingStarted(actual)) => {
                    eprintln!("state: {}", RealtimeState::Recording.label());
                    eprintln!(
                        "recording started ({} Hz / {} ch)",
                        actual.sample_rate, actual.channels
                    );
                }
                Some(PushToTalkRecorderEvent::RecordingStopped(audio)) => {
                    eprintln!("state: {}", RealtimeState::Transcribing.label());
                    let secs = audio.samples.len() as f64
                        / (audio.format.sample_rate as f64 * audio.format.channels as f64);
                    eprintln!(
                        "recording stopped; transcribing {:.2}s ({} samples)",
                        secs,
                        audio.samples.len()
                    );
                    let text = engine
                        .transcribe(&audio.samples, audio.format)
                        .context("failed to transcribe recording")?;
                    let mut clipboard = SystemClipboard::new();
                    clipboard
                        .write_text(&text)
                        .context("failed to write transcript to clipboard")?;
                    eprintln!("copied transcript to clipboard");
                    let mut paste = SystemPaste::new();
                    paste.paste().context("failed to simulate paste")?;
                    eprintln!("pasted transcript into focused app");
                    eprintln!("state: {}", RealtimeState::Completed.label());
                    println!("{text}");
                    return Ok(());
                }
                None => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn capture_backend(input: Option<PathBuf>) -> Result<Box<dyn AudioCapture>> {
    match input {
        Some(path) => {
            eprintln!("source: file {}", path.display());
            Ok(Box::new(FileCapture::from_wav(&path).with_context(
                || format!("failed to load {}", path.display()),
            )?))
        }
        None => {
            eprintln!("source: default microphone (cpal)");
            Ok(Box::new(CpalCapture::new()))
        }
    }
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
