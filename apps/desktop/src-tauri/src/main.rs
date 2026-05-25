use std::collections::HashMap;
use std::process::Command;
use std::fs::OpenOptions;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    mpsc::{self, Receiver, Sender, TryRecvError},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use voice_asr_cloud::dashscope::DASHSCOPE_API_KEY_ENV;
use voice_asr_cloud::{CloudEngineConfig, ParaformerCloudEngine};
use voice_asr_local::{StreamingZipformer, DEFAULT_STREAMING_ZIPFORMER_DIR, MODEL_DIR_ENV};
use voice_core::asr::AsrEngine;
use voice_core::capture::AudioFormat;
use voice_core::clipboard::{ClipboardWriter, SystemClipboard};
use voice_core::config::{
    AppConfig, AsrConfig, DesktopOutputModeConfig, HotkeyConfig, RewriteConfig,
};
use voice_core::cpal_backend::CpalCapture;
use voice_core::engine::{resolve_engine_selection, EngineKind, EngineSelection};
use voice_core::hotkey::PushToTalkEvent;
use voice_core::paste::{PasteSimulator, SystemPaste};
use voice_core::push_to_talk::{PushToTalkRecorder, PushToTalkRecorderEvent};
use voice_core::state::{RealtimeState, RealtimeStateEvent};
use voice_core::text_pipeline::rewrite_settings_from_config;
use voice_rewrite::{Profile, RewriteError, RewriteProvider, RewriteResult, RewriteTrace};

const SAMPLE_RATE: u32 = 16_000;
const CHANNELS: u16 = 1;
const REWRITE_KEYRING_SERVICE: &str = "voice-flow";
const ASR_KEYRING_SERVICE: &str = "voice-flow-asr";
const TRAY_MENU_OPEN: &str = "voice-flow-open";
const TRAY_MENU_TOGGLE_PAUSE: &str = "voice-flow-toggle-pause";
const TRAY_MENU_QUIT: &str = "voice-flow-quit";

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
    paused: bool,
    output_mode: DesktopOutputMode,
    manual_input: Option<Sender<PushToTalkEvent>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum DesktopOutputMode {
    #[default]
    FloatingInput,
    VoicePad,
}

impl DesktopOutputMode {
    fn label(self) -> &'static str {
        match self {
            Self::FloatingInput => "floating_input",
            Self::VoicePad => "voice_pad",
        }
    }

    fn to_config(self) -> DesktopOutputModeConfig {
        match self {
            Self::FloatingInput => DesktopOutputModeConfig::FloatingInput,
            Self::VoicePad => DesktopOutputModeConfig::VoicePad,
        }
    }
}

impl From<DesktopOutputModeConfig> for DesktopOutputMode {
    fn from(value: DesktopOutputModeConfig) -> Self {
        match value {
            DesktopOutputModeConfig::FloatingInput => Self::FloatingInput,
            DesktopOutputModeConfig::VoicePad => Self::VoicePad,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct ErrorEvent {
    state: &'static str,
    error: String,
}

#[derive(Clone, Debug, Serialize)]
struct PasteFailureEvent {
    text: String,
    error: String,
}

#[derive(Clone, Debug, Serialize)]
struct PauseStateEvent {
    paused: bool,
}

#[derive(Clone, Debug, Serialize)]
struct RewriteResultEvent {
    profile: String,
    text: String,
    variants: HashMap<String, String>,
    fallback: bool,
    error: Option<String>,
    trace: RewriteTrace,
    timings: TimingEvent,
}

#[derive(Clone, Debug, Serialize)]
struct TimingEvent {
    asr_ms: u128,
    rewrite_ms: Option<u128>,
    paste_ms: Option<u128>,
}

#[derive(Clone, Debug, Serialize)]
struct RewriteTraceEvent {
    profile: String,
    fallback: bool,
    error: Option<String>,
    preprocess_ms: u128,
    llm_ms: Option<u128>,
    llm_called: bool,
}

#[derive(Clone, Debug, Serialize)]
struct DesktopOutputResultEvent {
    output_mode: DesktopOutputMode,
    raw_transcript: String,
    final_text: String,
    profile: String,
    variants: HashMap<String, String>,
    fallback: bool,
    error: Option<String>,
    trace: Option<RewriteTrace>,
    timings: TimingEvent,
    pasted_to_external: bool,
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

#[derive(Clone, Debug, Serialize)]
struct AsrKeyStatus {
    engine: EngineKind,
    saved: bool,
    source: &'static str,
}

#[derive(Clone, Debug, Deserialize)]
struct AsrKeyRequest {
    api_key: String,
}

#[derive(Clone, Debug, Serialize)]
struct DiagnosticsEvent {
    config_path: String,
    log_path: String,
    model_dir: Option<String>,
    model_dir_exists: bool,
    runtime_running: bool,
    restart_requested: bool,
    paused: bool,
    output_mode: DesktopOutputMode,
    asr_engine: EngineKind,
    asr_key_saved: bool,
    rewrite_enabled: bool,
    rewrite_provider: RewriteProvider,
    rewrite_key_saved: bool,
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
    validate_hotkey_config(&config.hotkey)?;
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
fn get_asr_key_status(state: State<'_, DesktopState>) -> Result<AsrKeyStatus, String> {
    let engine = state
        .runtime
        .lock()
        .expect("runtime mutex poisoned")
        .config
        .asr
        .engine;
    asr_key_status(engine).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_asr_key(
    request: AsrKeyRequest,
    state: State<'_, DesktopState>,
) -> Result<AsrKeyStatus, String> {
    let key = request.api_key.trim();
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let entry = asr_key_entry()
        .map_err(|e| format!("failed to open ASR API key store: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("failed to save ASR API key: {e}"))?;

    state
        .runtime
        .lock()
        .expect("runtime mutex poisoned")
        .restart_requested = true;

    asr_key_status(EngineKind::Cloud).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_asr_key(state: State<'_, DesktopState>) -> Result<AsrKeyStatus, String> {
    let entry = asr_key_entry()
        .map_err(|e| format!("failed to open ASR API key store: {e}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => {}
        Err(err) => return Err(format!("failed to clear ASR API key: {err}")),
    }

    state
        .runtime
        .lock()
        .expect("runtime mutex poisoned")
        .restart_requested = true;

    asr_key_status(EngineKind::Cloud).map_err(|e| e.to_string())
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
fn get_diagnostics(state: State<'_, DesktopState>) -> Result<DiagnosticsEvent, String> {
    diagnostics_event(&state).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_log_directory() -> Result<(), String> {
    let log_dir = log_path()
        .parent()
        .ok_or_else(|| "log directory is unavailable".to_string())?
        .to_path_buf();
    Command::new("explorer")
        .arg(log_dir)
        .spawn()
        .map_err(|e| format!("failed to open log directory: {e}"))?;
    Ok(())
}

#[tauri::command]
fn get_pause_state(state: State<'_, DesktopState>) -> PauseStateEvent {
    pause_state_event(&state.runtime)
}

#[tauri::command]
fn get_output_mode(state: State<'_, DesktopState>) -> DesktopOutputMode {
    current_output_mode(&state.runtime)
}

#[tauri::command]
fn set_output_mode(
    output_mode: DesktopOutputMode,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopOutputMode, String> {
    let config = {
        let mut runtime = state.runtime.lock().expect("runtime mutex poisoned");
        runtime.output_mode = output_mode;
        runtime.config.desktop.output_mode = output_mode.to_config();
        runtime.config.clone()
    };
    config
        .write_to(config_path())
        .map_err(|e| format!("failed to persist desktop output mode: {e}"))?;
    append_log(format!("output mode set to {}", output_mode.label()));
    emit_output_mode(&app, output_mode);
    Ok(output_mode)
}

#[tauri::command]
fn set_pause_state(
    paused: bool,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> PauseStateEvent {
    set_runtime_paused(&state.runtime, paused);
    append_log(if paused {
        "listener paused"
    } else {
        "listener resumed"
    });
    emit_pause_state(&app, paused);
    PauseStateEvent { paused }
}

#[tauri::command]
fn begin_manual_recording(state: State<'_, DesktopState>) -> Result<(), String> {
    send_manual_input(&state.runtime, PushToTalkEvent::Pressed)
}

#[tauri::command]
fn end_manual_recording(state: State<'_, DesktopState>) -> Result<(), String> {
    send_manual_input(&state.runtime, PushToTalkEvent::Released)
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
fn paste_text(text: String) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("paste text cannot be empty".to_string());
    }

    paste_transcript(&text).map_err(|e| format!("failed to paste text: {e:#}"))
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    append_log("exit requested from ui");
    app.exit(0);
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

fn validate_hotkey_config(config: &HotkeyConfig) -> Result<(), String> {
    let key = config.key.trim();
    if key.is_empty() {
        return Err("hotkey key cannot be empty".to_string());
    }
    if !(config.ctrl || config.alt || config.shift || config.logo) {
        return Err("hotkey requires at least one modifier".to_string());
    }
    if key.contains('+') || key.split_whitespace().count() > 1 {
        return Err("hotkey key must be a single key name, modifiers use checkboxes".to_string());
    }
    Ok(())
}

fn main() {
    let config = load_config();
    let output_mode = DesktopOutputMode::from(config.desktop.output_mode);
    let state = DesktopState {
        runtime: Arc::new(Mutex::new(RuntimeState {
            config,
            running: false,
            restart_requested: false,
            paused: false,
            output_mode,
            manual_input: None,
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
            get_asr_key_status,
            save_asr_key,
            delete_asr_key,
            get_rewrite_key_status,
            save_rewrite_key,
            delete_rewrite_key,
            get_diagnostics,
            open_log_directory,
            get_pause_state,
            get_output_mode,
            set_output_mode,
            set_pause_state,
            begin_manual_recording,
            end_manual_recording,
            copy_text,
            paste_text,
            start_runtime,
            exit_app
        ])
        .setup(|app| {
            setup_tray(app)?;
            setup_close_to_tray(app);
            maybe_prompt_deepseek_key();
            app.emit(
                "realtime-state",
                RealtimeStateEvent::new(RealtimeState::Idle),
            )?;
            app.emit("output-mode-updated", DesktopOutputMode::default())?;
            app.emit("pause-state", PauseStateEvent { paused: false })?;
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

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, TRAY_MENU_OPEN, "打开窗口", true, None::<&str>)?;
    let toggle_pause = MenuItem::with_id(
        app,
        TRAY_MENU_TOGGLE_PAUSE,
        "暂停/恢复监听",
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &toggle_pause, &separator, &quit])?;
    let runtime = app.state::<DesktopState>().runtime.clone();

    TrayIconBuilder::new()
        .tooltip("voice-flow")
        .icon(tauri::include_image!("./icons/icon.ico"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            TRAY_MENU_OPEN => open_main_window(app),
            TRAY_MENU_TOGGLE_PAUSE => {
                let paused = toggle_runtime_paused(&runtime);
                append_log(if paused {
                    "listener paused from tray"
                } else {
                    "listener resumed from tray"
                });
                emit_pause_state(app, paused);
            }
            TRAY_MENU_QUIT => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn setup_close_to_tray(app: &mut tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let window_to_hide = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window_to_hide.hide();
        }
    });
}

fn open_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn run_runtime_session(app: &AppHandle, runtime: &RuntimeHandle, config: AppConfig) -> Result<()> {
    let engine = build_desktop_engine(app, &config)?;
    let hotkey = DesktopHotkey::register(app, config.hotkey.clone())
        .with_context(|| format!("failed to register hotkey {}", config.hotkey.to_label()))?;
    append_log(format!("registered hotkey {}", config.hotkey.to_label()));
    let (manual_tx, manual_rx) = mpsc::channel();
    let _manual_input = register_manual_input(runtime, manual_tx);
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
        if is_paused(runtime) {
            if recorder.is_recording() {
                let _ = recorder.handle_hotkey_event(PushToTalkEvent::Released);
                append_log("recording cancelled because listener paused");
                emit_state(app, RealtimeStateEvent::new(RealtimeState::Idle));
            }
            drain_input_events(&hotkey, &manual_rx)?;
            thread::sleep(Duration::from_millis(20));
            continue;
        }

        if let Some(event) = next_push_to_talk_event(&hotkey, &manual_rx)? {
            match recorder
                .handle_hotkey_event(event)
                .map_err(|e| anyhow::anyhow!(e))
                .context("failed to handle push-to-talk event")?
            {
                Some(PushToTalkRecorderEvent::RecordingStarted(_)) => {
                    append_log("recording started");
                    emit_state(app, RealtimeStateEvent::new(RealtimeState::Recording));
                }
                Some(PushToTalkRecorderEvent::RecordingStopped(audio)) => {
                    append_log("recording stopped");
                    emit_state(app, RealtimeStateEvent::new(RealtimeState::Transcribing));
                    let asr_started = Instant::now();
                    let transcript = engine
                        .transcribe(&audio.samples, audio.format)
                        .context("failed to transcribe recording")?;
                    let asr_ms = asr_started.elapsed().as_millis();
                    let rewrite_started = Instant::now();
                    let rewrite = maybe_rewrite_transcript(app, &config.rewrite, &transcript)
                        .context("failed to rewrite transcript")?;
                    let rewrite_ms = if config.rewrite.enabled {
                        Some(rewrite_started.elapsed().as_millis())
                    } else {
                        None
                    };
                    let output_mode = current_output_mode(runtime);
                    let mut paste_ms = None;
                    let mut pasted_to_external = false;
                    if output_mode == DesktopOutputMode::FloatingInput {
                        let paste_started = Instant::now();
                        match paste_transcript(&rewrite.text) {
                            Ok(()) => {
                                pasted_to_external = true;
                            }
                            Err(err) => {
                                let message = format!("{err:#}");
                                append_log(format!("paste failed: {message}"));
                                emit_paste_failure(app, rewrite.text.clone(), message);
                            }
                        }
                        paste_ms = Some(paste_started.elapsed().as_millis());
                    }
                    let timings = TimingEvent {
                        asr_ms,
                        rewrite_ms,
                        paste_ms,
                    };
                    emit_desktop_output_result(
                        app,
                        output_mode,
                        &transcript,
                        &rewrite,
                        timings.clone(),
                        pasted_to_external,
                    );
                    if let Some(result) = &rewrite.result {
                        emit_rewrite_result(app, result, timings);
                    }
                    emit_state(
                        app,
                        RealtimeStateEvent::new(RealtimeState::Completed)
                            .with_transcript(rewrite.text),
                    );
                    emit_state(app, RealtimeStateEvent::new(RealtimeState::Idle));
                }
                None => {}
            }
        }

        thread::sleep(Duration::from_millis(20));
    }
}

struct ManualInputRegistration {
    runtime: RuntimeHandle,
}

impl Drop for ManualInputRegistration {
    fn drop(&mut self) {
        self.runtime
            .lock()
            .expect("runtime mutex poisoned")
            .manual_input = None;
    }
}

fn register_manual_input(
    runtime: &RuntimeHandle,
    tx: Sender<PushToTalkEvent>,
) -> ManualInputRegistration {
    runtime.lock().expect("runtime mutex poisoned").manual_input = Some(tx);
    ManualInputRegistration {
        runtime: Arc::clone(runtime),
    }
}

fn send_manual_input(runtime: &RuntimeHandle, event: PushToTalkEvent) -> Result<(), String> {
    let tx = runtime
        .lock()
        .expect("runtime mutex poisoned")
        .manual_input
        .clone()
        .ok_or_else(|| "voice runtime is not ready".to_string())?;

    tx.send(event)
        .map_err(|_| "voice runtime is not accepting manual input".to_string())
}

fn next_push_to_talk_event(
    hotkey: &DesktopHotkey,
    manual_rx: &Receiver<PushToTalkEvent>,
) -> Result<Option<PushToTalkEvent>> {
    if let Some(event) = hotkey.try_recv().context("failed to read hotkey event")? {
        return Ok(Some(event));
    }

    try_recv_manual_input(manual_rx).context("failed to read manual input event")
}

fn drain_input_events(hotkey: &DesktopHotkey, manual_rx: &Receiver<PushToTalkEvent>) -> Result<()> {
    drain_hotkey_events(hotkey)?;
    while try_recv_manual_input(manual_rx)?.is_some() {}
    Ok(())
}

fn try_recv_manual_input(manual_rx: &Receiver<PushToTalkEvent>) -> Result<Option<PushToTalkEvent>> {
    match manual_rx.try_recv() {
        Ok(event) => Ok(Some(event)),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => {
            anyhow::bail!("manual input event channel disconnected")
        }
    }
}

fn drain_hotkey_events(hotkey: &DesktopHotkey) -> Result<()> {
    while hotkey.try_recv()?.is_some() {}
    Ok(())
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

fn diagnostics_event(state: &State<'_, DesktopState>) -> Result<DiagnosticsEvent> {
    let runtime = state.runtime.lock().expect("runtime mutex poisoned");
    let model_dir_exists = runtime
        .config
        .model_dir
        .as_deref()
        .map(|path| Path::new(path).is_dir())
        .unwrap_or(false);
    let rewrite_key_saved = read_rewrite_key(runtime.config.rewrite.provider)
        .map(|key| key.is_some())
        .unwrap_or(false);
    let asr_key_saved = read_asr_key().map(|key| key.is_some()).unwrap_or(false);

    Ok(DiagnosticsEvent {
        config_path: config_path().display().to_string(),
        log_path: log_path().display().to_string(),
        model_dir: runtime.config.model_dir.clone(),
        model_dir_exists,
        runtime_running: runtime.running,
        restart_requested: runtime.restart_requested,
        paused: runtime.paused,
        output_mode: runtime.output_mode,
        asr_engine: runtime.config.asr.engine,
        asr_key_saved,
        rewrite_enabled: runtime.config.rewrite.enabled,
        rewrite_provider: runtime.config.rewrite.provider,
        rewrite_key_saved,
    })
}

fn maybe_rewrite_transcript(
    app: &AppHandle,
    rewrite: &RewriteConfig,
    transcript: &str,
) -> Result<DesktopRewriteOutput> {
    if !rewrite.enabled {
        return Ok(DesktopRewriteOutput {
            text: transcript.to_string(),
            result: None,
        });
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

    Ok(DesktopRewriteOutput {
        text: result.main.clone(),
        result: Some(result),
    })
}

struct DesktopRewriteOutput {
    text: String,
    result: Option<RewriteResult>,
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

fn asr_key_status(engine: EngineKind) -> Result<AsrKeyStatus> {
    Ok(AsrKeyStatus {
        engine,
        saved: if engine == EngineKind::Cloud {
            read_asr_key()?.is_some()
        } else {
            false
        },
        source: "keyring",
    })
}

fn read_asr_key() -> Result<Option<String>> {
    match asr_key_entry()?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(err) => Err(anyhow::anyhow!(err)),
    }
}

fn asr_key_entry() -> Result<Entry> {
    Entry::new(ASR_KEYRING_SERVICE, "dashscope").map_err(|e| anyhow::anyhow!(e))
}

fn build_desktop_engine(app: &AppHandle, config: &AppConfig) -> Result<Box<dyn AsrEngine>> {
    let selection = resolve_desktop_selection(app, &config.asr, config.model_dir.as_deref())?;
    build_engine(&selection)
}

fn resolve_desktop_selection(
    app: &AppHandle,
    asr: &AsrConfig,
    model_dir: Option<&str>,
) -> Result<EngineSelection> {
    let model_dir = model_dir
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty() && path.is_dir())
        .or_else(|| {
            std::env::var_os(MODEL_DIR_ENV)
                .map(PathBuf::from)
                .filter(|p| p.is_dir())
        })
        .or_else(|| bundled_model_dir(app))
        .or_else(|| default_model_dir().filter(|p| p.is_dir()));
    let api_key = if asr.engine == EngineKind::Cloud {
        read_asr_key()?
            .or_else(|| std::env::var(DASHSCOPE_API_KEY_ENV).ok())
            .filter(|key| !key.trim().is_empty())
    } else {
        None
    };

    resolve_engine_selection(asr.engine, model_dir, api_key).map_err(|e| match e {
        voice_core::engine::EngineSelectionError::MissingModelDir => {
            anyhow::anyhow!("missing model directory for local ASR")
        }
        voice_core::engine::EngineSelectionError::MissingApiKey => {
            anyhow::anyhow!("missing DashScope API key for cloud ASR")
        }
        other => anyhow::Error::from(other),
    })
}

fn build_engine(selection: &EngineSelection) -> Result<Box<dyn AsrEngine>> {
    match selection {
        EngineSelection::Local(params) => {
            append_log(format!("loading local ASR model from {}", params.model_dir.display()));
            let engine = StreamingZipformer::from_model_dir(&params.model_dir).with_context(|| {
                format!("failed to load model from {}", params.model_dir.display())
            })?;
            Ok(Box::new(engine))
        }
        EngineSelection::Cloud(params) => {
            append_log("initializing cloud ASR engine (dashscope)");
            let config = CloudEngineConfig::dashscope(params.api_key.clone());
            let engine = ParaformerCloudEngine::from_config(&config)
                .context("failed to initialize cloud engine")?;
            Ok(Box::new(engine))
        }
    }
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

fn pause_state_event(runtime: &RuntimeHandle) -> PauseStateEvent {
    PauseStateEvent {
        paused: is_paused(runtime),
    }
}

fn current_output_mode(runtime: &RuntimeHandle) -> DesktopOutputMode {
    runtime.lock().expect("runtime mutex poisoned").output_mode
}

fn is_paused(runtime: &RuntimeHandle) -> bool {
    runtime.lock().expect("runtime mutex poisoned").paused
}

fn set_runtime_paused(runtime: &RuntimeHandle, paused: bool) {
    runtime.lock().expect("runtime mutex poisoned").paused = paused;
}

fn toggle_runtime_paused(runtime: &RuntimeHandle) -> bool {
    let mut guard = runtime.lock().expect("runtime mutex poisoned");
    guard.paused = !guard.paused;
    guard.paused
}

fn emit_state(app: &AppHandle, event: RealtimeStateEvent) {
    let _ = app.emit("realtime-state", event);
}

fn emit_pause_state(app: &AppHandle, paused: bool) {
    let _ = app.emit("pause-state", PauseStateEvent { paused });
}

fn emit_output_mode(app: &AppHandle, output_mode: DesktopOutputMode) {
    let _ = app.emit("output-mode-updated", output_mode);
}

fn emit_rewrite_result(app: &AppHandle, result: &RewriteResult, timings: TimingEvent) {
    let _ = app.emit(
        "rewrite-result",
        RewriteResultEvent {
            profile: result.trace.profile.label().to_string(),
            text: result.main.clone(),
            variants: result.variants.clone(),
            fallback: result.trace.fallback,
            error: result.trace.error.clone(),
            trace: result.trace.clone(),
            timings,
        },
    );
    emit_rewrite_trace(app, &result.trace);
}

fn emit_rewrite_trace(app: &AppHandle, trace: &RewriteTrace) {
    let _ = app.emit(
        "rewrite-trace",
        RewriteTraceEvent {
            profile: trace.profile.label().to_string(),
            fallback: trace.fallback,
            error: trace.error.clone(),
            preprocess_ms: trace.preprocess_ms,
            llm_ms: trace.llm_ms,
            llm_called: trace.llm_called,
        },
    );
}

fn emit_desktop_output_result(
    app: &AppHandle,
    output_mode: DesktopOutputMode,
    raw_transcript: &str,
    rewrite: &DesktopRewriteOutput,
    timings: TimingEvent,
    pasted_to_external: bool,
) {
    let (profile, variants, fallback, error, trace) = match &rewrite.result {
        Some(result) => (
            result.trace.profile.label().to_string(),
            result.variants.clone(),
            result.trace.fallback,
            result.trace.error.clone(),
            Some(result.trace.clone()),
        ),
        None => (
            Profile::Off.label().to_string(),
            HashMap::new(),
            false,
            None,
            None,
        ),
    };

    let _ = app.emit(
        "desktop-output-result",
        DesktopOutputResultEvent {
            output_mode,
            raw_transcript: raw_transcript.to_string(),
            final_text: rewrite.text.clone(),
            profile,
            variants,
            fallback,
            error,
            trace,
            timings,
            pasted_to_external,
        },
    );
}

fn emit_error(app: &AppHandle, error: anyhow::Error) {
    eprintln!("runtime error: {error:#}");
    append_log(format!("runtime error: {error:#}"));
    let _ = app.emit(
        "runtime-error",
        ErrorEvent {
            state: "error",
            error: error.to_string(),
        },
    );
}

fn emit_paste_failure(app: &AppHandle, text: String, error: String) {
    let _ = app.emit("paste-failure", PasteFailureEvent { text, error });
}

fn load_config() -> AppConfig {
    let path = config_path();
    match AppConfig::read_from(&path) {
        Ok(config) => {
            let (config, migrated) = migrate_config_defaults(config);
            if migrated {
                let _ = config.write_to(&path);
            }
            config
        }
        Err(_) => {
            let config = AppConfig::default();
            let _ = config.write_to(&path);
            config
        }
    }
}

fn migrate_config_defaults(mut config: AppConfig) -> (AppConfig, bool) {
    if config.hotkey == legacy_default_hotkey() {
        config.hotkey = HotkeyConfig::default();
        return (config, true);
    }

    (config, false)
}

fn legacy_default_hotkey() -> HotkeyConfig {
    HotkeyConfig {
        ctrl: true,
        alt: true,
        shift: false,
        logo: false,
        key: "Space".to_string(),
    }
}

fn default_model_dir() -> Option<PathBuf> {
    workspace_root().map(|root| root.join("models").join(DEFAULT_STREAMING_ZIPFORMER_DIR))
}

/// 解析 Tauri installer 把模型作为 resource 打包后的真实落地路径。
///
/// `bundle.resources` 用 list 形式时，Tauri 会把每个 `../` 替换为 `_up_`，
/// 因此 `../../../models/<DIR>/encoder.onnx` 安装后变成
/// `$RESOURCE/_up_/_up_/_up_/models/<DIR>/encoder.onnx`。
/// 我们这里也按同样路径拼回 `<DIR>` 一层，传给 `StreamingZipformer`。
fn bundled_model_dir(app: &AppHandle) -> Option<PathBuf> {
    let resource_root = app.path().resource_dir().ok()?;
    let candidate = resource_root
        .join("_up_")
        .join("_up_")
        .join("_up_")
        .join("models")
        .join(DEFAULT_STREAMING_ZIPFORMER_DIR);
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
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

/// 跳过 key 弹窗的标记文件路径——用户点过"跳过"后写入，下次不再弹。
fn key_prompt_skip_marker() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voice-flow")
        .join(".skip-deepseek-prompt")
}

/// 首次启动检查：若 keyring 中没有 DeepSeek key 且环境变量也未设置，
/// 异步弹一个原生 InputBox 让用户粘 key（也可以跳过）。仅 Windows。
fn maybe_prompt_deepseek_key() {
    #[cfg(target_os = "windows")]
    {
        if std::env::var(voice_rewrite::DEEPSEEK_API_KEY_ENV)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
        {
            return;
        }
        if matches!(read_rewrite_key(RewriteProvider::DeepSeek), Ok(Some(_))) {
            return;
        }
        if key_prompt_skip_marker().exists() {
            return;
        }

        thread::Builder::new()
            .name("voice-flow-key-prompt".to_string())
            .spawn(prompt_deepseek_key_via_powershell)
            .ok();
    }
}

#[cfg(target_os = "windows")]
fn prompt_deepseek_key_via_powershell() {
    let script = r#"
[void][System.Reflection.Assembly]::LoadWithPartialName('Microsoft.VisualBasic')
$key = [Microsoft.VisualBasic.Interaction]::InputBox(
  "voice-flow 需要 DeepSeek API key 才能启用 AI 改写。`n`n请粘贴 key（留空表示跳过；评委体验请联系作者获取）。",
  "voice-flow · DeepSeek key",
  ""
)
[Console]::Out.Write($key)
"#;

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = match Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            append_log(format!("deepseek key prompt failed to launch: {err}"));
            return;
        }
    };

    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if key.is_empty() {
        let _ = std::fs::create_dir_all(
            key_prompt_skip_marker()
                .parent()
                .unwrap_or_else(|| Path::new(".")),
        );
        let _ = std::fs::File::create(key_prompt_skip_marker());
        append_log("deepseek key prompt: user skipped; marker written");
        return;
    }

    let entry = match rewrite_key_entry(RewriteProvider::DeepSeek) {
        Ok(entry) => entry,
        Err(err) => {
            append_log(format!("failed to open keyring for deepseek key: {err}"));
            return;
        }
    };
    if let Err(err) = entry.set_password(&key) {
        append_log(format!("failed to write deepseek key to keyring: {err}"));
    } else {
        append_log("deepseek key saved to keyring via first-run prompt");
    }
}

#[cfg(not(target_os = "windows"))]
fn prompt_deepseek_key_via_powershell() {}

fn log_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voice-flow")
        .join("logs")
        .join("desktop.log")
}

fn append_log(message: impl AsRef<str>) {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", message.as_ref());
    }
}

#[allow(dead_code)]
fn default_hotkey() -> HotkeyConfig {
    HotkeyConfig::default()
}
