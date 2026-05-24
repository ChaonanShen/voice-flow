use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{
    mpsc::{self, Receiver, TryRecvError},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use voice_asr_local::{StreamingZipformer, DEFAULT_STREAMING_ZIPFORMER_DIR, MODEL_DIR_ENV};
use voice_core::asr::AsrEngine;
use voice_core::capture::AudioFormat;
use voice_core::clipboard::{ClipboardWriter, SystemClipboard};
use voice_core::config::{AppConfig, HotkeyConfig, RewriteConfig};
use voice_core::cpal_backend::CpalCapture;
use voice_core::hotkey::PushToTalkEvent;
use voice_core::paste::{PasteSimulator, SystemPaste};
use voice_core::push_to_talk::{PushToTalkRecorder, PushToTalkRecorderEvent};
use voice_core::state::{RealtimeState, RealtimeStateEvent};
use voice_core::text_pipeline::rewrite_settings_from_config;
use voice_rewrite::{RewriteError, RewriteProvider, RewriteResult};

const SAMPLE_RATE: u32 = 16_000;
const CHANNELS: u16 = 1;
const REWRITE_KEYRING_SERVICE: &str = "voice-flow";

#[derive(Clone)]
struct DesktopState {
    runtime: RuntimeHandle,
}

type RuntimeHandle = Arc<Mutex<RuntimeState>>;

#[derive(Default)]
struct RuntimeState {
    config: AppConfig,
    running: bool,
    restart_requested: bool,
}

#[derive(Clone, Debug, Serialize)]
struct ErrorEvent {
    state: &'static str,
    error: String,
}

#[derive(Clone, Debug, Serialize)]
struct RewriteResultEvent {
    profile: String,
    text: String,
    variants: HashMap<String, String>,
    fallback: bool,
    error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct RewriteKeyRequest {
    provider: RewriteProvider,
    api_key: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RewriteKeyProviderRequest {
    provider: RewriteProvider,
}

#[derive(Clone, Debug, Serialize)]
struct RewriteKeyStatus {
    provider: RewriteProvider,
    saved: bool,
    source: &'static str,
}

#[tauri::command]
fn get_config(state: State<'_, DesktopState>) -> AppConfig {
    state
        .runtime
        .lock()
        .expect("runtime mutex poisoned")
        .config
        .clone()
}

#[tauri::command]
fn save_config(config: AppConfig, state: State<'_, DesktopState>) -> Result<AppConfig, String> {
    config
        .write_to(config_path())
        .map_err(|e| format!("failed to save config: {e}"))?;

    let mut runtime = state.runtime.lock().expect("runtime mutex poisoned");
    runtime.config = config.clone();
    runtime.restart_requested = true;
    Ok(config)
}

#[tauri::command]
fn get_rewrite_config(state: State<'_, DesktopState>) -> RewriteConfig {
    state
        .runtime
        .lock()
        .expect("runtime mutex poisoned")
        .config
        .rewrite
        .clone()
}

#[tauri::command]
fn save_rewrite_config(
    rewrite: RewriteConfig,
    state: State<'_, DesktopState>,
) -> Result<RewriteConfig, String> {
    let config = {
        let mut runtime = state.runtime.lock().expect("runtime mutex poisoned");
        runtime.config.rewrite = rewrite.clone();
        runtime.restart_requested = true;
        runtime.config.clone()
    };

    config
        .write_to(config_path())
        .map_err(|e| format!("failed to save rewrite config: {e}"))?;

    Ok(rewrite)
}

#[tauri::command]
fn get_rewrite_key_status(request: RewriteKeyProviderRequest) -> Result<RewriteKeyStatus, String> {
    rewrite_key_status(request.provider).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_rewrite_key(
    request: RewriteKeyRequest,
    state: State<'_, DesktopState>,
) -> Result<RewriteKeyStatus, String> {
    let key = request.api_key.trim();
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = rewrite_key_entry(request.provider)
        .map_err(|e| format!("failed to open API key store: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("failed to save API key: {e}"))?;

    state
        .runtime
        .lock()
        .expect("runtime mutex poisoned")
        .restart_requested = true;

    rewrite_key_status(request.provider).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_rewrite_key(
    request: RewriteKeyProviderRequest,
    state: State<'_, DesktopState>,
) -> Result<RewriteKeyStatus, String> {
    let entry = rewrite_key_entry(request.provider)
        .map_err(|e| format!("failed to open API key store: {e}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => {}
        Err(err) => return Err(format!("failed to clear API key: {err}")),
    }

    state
        .runtime
        .lock()
        .expect("runtime mutex poisoned")
        .restart_requested = true;

    rewrite_key_status(request.provider).map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("copy text cannot be empty".to_string());
    }

    let mut clipboard = SystemClipboard::new();
    clipboard
        .write_text(&text)
        .map_err(|e| format!("failed to copy text: {e}"))
}

#[tauri::command]
fn start_runtime(app: AppHandle, state: State<'_, DesktopState>) -> Result<(), String> {
    let runtime = state.runtime.clone();
    {
        let mut guard = runtime.lock().expect("runtime mutex poisoned");
        if guard.running {
            return Ok(());
        }
        guard.running = true;
    }

    thread::Builder::new()
        .name("voice-flow-realtime".to_string())
        .spawn(move || runtime_loop(app, runtime))
        .map_err(|e| format!("failed to start runtime: {e}"))?;

    Ok(())
}

fn main() {
    let config = load_config();
    let state = DesktopState {
        runtime: Arc::new(Mutex::new(RuntimeState {
            config,
            running: false,
            restart_requested: false,
        })),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_rewrite_config,
            save_config,
            save_rewrite_config,
            get_rewrite_key_status,
            save_rewrite_key,
            delete_rewrite_key,
            copy_text,
            start_runtime
        ])
        .setup(|app| {
            app.emit(
                "realtime-state",
                RealtimeStateEvent::new(RealtimeState::Idle),
            )?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run voice-flow desktop app");
}

fn runtime_loop(app: AppHandle, runtime: RuntimeHandle) {
    loop {
        let config = {
            let mut guard = runtime.lock().expect("runtime mutex poisoned");
            guard.restart_requested = false;
            guard.config.clone()
        };

        if let Err(err) = run_runtime_session(&app, &runtime, config) {
            emit_error(&app, err);
            thread::sleep(Duration::from_secs(1));
        }
    }
}

fn run_runtime_session(app: &AppHandle, runtime: &RuntimeHandle, config: AppConfig) -> Result<()> {
    let model_dir = resolve_model_dir(config.model_dir.as_deref())?;
    let engine = StreamingZipformer::from_model_dir(&model_dir)
        .with_context(|| format!("failed to load model from {}", model_dir.display()))?;
    let hotkey = DesktopHotkey::register(app, config.hotkey.clone())
        .with_context(|| format!("failed to register hotkey {}", config.hotkey.to_label()))?;
    let mut recorder = PushToTalkRecorder::new(
        CpalCapture::new(),
        AudioFormat {
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
        },
    );

    emit_state(app, RealtimeStateEvent::new(RealtimeState::Idle));

    loop {
        if take_restart(runtime) {
            drop(hotkey);
            return Ok(());
        }

        recorder.poll_audio();
        if let Some(event) = hotkey.try_recv().context("failed to read hotkey event")? {
            match recorder
                .handle_hotkey_event(event)
                .map_err(|e| anyhow::anyhow!(e))
                .context("failed to handle push-to-talk event")?
            {
                Some(PushToTalkRecorderEvent::RecordingStarted(_)) => {
                    emit_state(app, RealtimeStateEvent::new(RealtimeState::Recording));
                }
                Some(PushToTalkRecorderEvent::RecordingStopped(audio)) => {
                    emit_state(app, RealtimeStateEvent::new(RealtimeState::Transcribing));
                    let transcript = engine
                        .transcribe(&audio.samples, audio.format)
                        .context("failed to transcribe recording")?;
                    let text = maybe_rewrite_transcript(app, &config.rewrite, &transcript)
                        .context("failed to rewrite transcript")?;
                    paste_transcript(&text).context("failed to paste transcript")?;
                    emit_state(
                        app,
                        RealtimeStateEvent::new(RealtimeState::Completed).with_transcript(text),
                    );
                    emit_state(app, RealtimeStateEvent::new(RealtimeState::Idle));
                }
                None => {}
            }
        }

        thread::sleep(Duration::from_millis(20));
    }
}

struct DesktopHotkey {
    app: AppHandle,
    label: String,
    rx: Receiver<PushToTalkEvent>,
}

impl DesktopHotkey {
    fn register(app: &AppHandle, config: HotkeyConfig) -> Result<Self> {
        let label = config.to_label();
        let (tx, rx) = mpsc::channel();

        app.global_shortcut()
            .on_shortcut(label.as_str(), move |_app, _shortcut, event| {
                let push_event = match event.state {
                    ShortcutState::Pressed => PushToTalkEvent::Pressed,
                    ShortcutState::Released => PushToTalkEvent::Released,
                };
                let _ = tx.send(push_event);
            })
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        Ok(Self {
            app: app.clone(),
            label,
            rx,
        })
    }

    fn try_recv(&self) -> Result<Option<PushToTalkEvent>> {
        match self.rx.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                anyhow::bail!("hotkey event channel disconnected")
            }
        }
    }
}

impl Drop for DesktopHotkey {
    fn drop(&mut self) {
        let _ = self.app.global_shortcut().unregister(self.label.as_str());
    }
}

fn paste_transcript(text: &str) -> Result<()> {
    let mut clipboard = SystemClipboard::new();
    clipboard.write_text(text)?;
    let mut paste = SystemPaste::new();
    paste.paste()?;
    Ok(())
}

fn maybe_rewrite_transcript(
    app: &AppHandle,
    rewrite: &RewriteConfig,
    transcript: &str,
) -> Result<String> {
    if !rewrite.enabled {
        return Ok(transcript.to_string());
    }

    emit_state(app, RealtimeStateEvent::new(RealtimeState::Rewriting));
    let mut settings = rewrite_settings_from_config(rewrite);
    match read_rewrite_key(rewrite.provider) {
        Ok(Some(api_key)) => {
            settings = settings.with_api_key(api_key);
        }
        Ok(None) => {}
        Err(err) => {
            eprintln!("rewrite keyring read failed: {err:#}");
        }
    }
    let engine = settings
        .build_engine()
        .context("failed to initialize rewrite engine")?;
    let result = run_rewrite(engine.process(transcript))?;

    if result.trace.fallback {
        if let Some(error) = &result.trace.error {
            eprintln!("rewrite fallback: {error}");
        }
    }

    emit_rewrite_result(app, &result);
    Ok(result.main)
}

fn rewrite_key_status(provider: RewriteProvider) -> Result<RewriteKeyStatus> {
    Ok(RewriteKeyStatus {
        provider,
        saved: read_rewrite_key(provider)?.is_some(),
        source: "keyring",
    })
}

fn read_rewrite_key(provider: RewriteProvider) -> Result<Option<String>> {
    match rewrite_key_entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(err) => Err(anyhow::anyhow!(err)),
    }
}

fn rewrite_key_entry(provider: RewriteProvider) -> Result<Entry> {
    Entry::new(REWRITE_KEYRING_SERVICE, provider.label()).map_err(|e| anyhow::anyhow!(e))
}

fn run_rewrite<F>(future: F) -> Result<RewriteResult>
where
    F: Future<Output = Result<RewriteResult, RewriteError>>,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build rewrite runtime")?;
    runtime.block_on(future).context("failed to rewrite text")
}

fn take_restart(runtime: &RuntimeHandle) -> bool {
    let mut guard = runtime.lock().expect("runtime mutex poisoned");
    if guard.restart_requested {
        guard.restart_requested = false;
        true
    } else {
        false
    }
}

fn emit_state(app: &AppHandle, event: RealtimeStateEvent) {
    let _ = app.emit("realtime-state", event);
}

fn emit_rewrite_result(app: &AppHandle, result: &RewriteResult) {
    let _ = app.emit(
        "rewrite-result",
        RewriteResultEvent {
            profile: result.trace.profile.label().to_string(),
            text: result.main.clone(),
            variants: result.variants.clone(),
            fallback: result.trace.fallback,
            error: result.trace.error.clone(),
        },
    );
}

fn emit_error(app: &AppHandle, error: anyhow::Error) {
    eprintln!("runtime error: {error:#}");
    let _ = app.emit(
        "runtime-error",
        ErrorEvent {
            state: "error",
            error: error.to_string(),
        },
    );
}

fn load_config() -> AppConfig {
    match AppConfig::read_from(config_path()) {
        Ok(config) => with_default_model_dir(config),
        Err(_) => {
            let config = with_default_model_dir(AppConfig::default());
            let _ = config.write_to(config_path());
            config
        }
    }
}

fn with_default_model_dir(mut config: AppConfig) -> AppConfig {
    if config.model_dir.is_none() {
        if let Some(path) = default_model_dir() {
            config.model_dir = Some(path.display().to_string());
        }
    }
    config
}

fn resolve_model_dir(configured: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = configured.filter(|p| !p.trim().is_empty()) {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
    }

    if let Some(path) = std::env::var_os(MODEL_DIR_ENV).map(PathBuf::from) {
        if path.is_dir() {
            return Ok(path);
        }
    }

    if let Some(path) = default_model_dir() {
        if path.is_dir() {
            return Ok(path);
        }
    }

    anyhow::bail!("model directory is not configured or does not exist")
}

fn default_model_dir() -> Option<PathBuf> {
    workspace_root().map(|root| root.join("models").join(DEFAULT_STREAMING_ZIPFORMER_DIR))
}

fn workspace_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voice-flow")
        .join("app.toml")
}

#[allow(dead_code)]
fn default_hotkey() -> HotkeyConfig {
    HotkeyConfig::default()
}
