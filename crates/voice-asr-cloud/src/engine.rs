//! `AsrEngine` 实现：DashScope Paraformer-realtime-v2 WebSocket。
//!
//! 引擎对外是同步阻塞接口（与 [`voice_core::asr::AsrEngine`] 对齐）。内部启动
//! 一个 current-thread tokio runtime，跑一遍完整的流式会话：
//!
//! ```text
//! 建连 → run-task → push PCM (binary frames, 100ms 一片) → finish-task
//!      → 收 result-generated * N → 收 task-finished → 关闭
//! ```
//!
//! 第一版只把"最终句子"拼接起来返回；中间假设和实时 UI 增量留给后续 Step。

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, http::HeaderValue, protocol::Message,
};
use voice_core::asr::{AsrEngine, AsrError};
use voice_core::capture::AudioFormat;

use crate::dashscope::DashScopeClient;
use crate::protocol::{ClientEvent, RunTaskParams, ServerEvent};
use crate::{CloudAsrError, CloudEngineConfig};

const CHUNK_DURATION: Duration = Duration::from_millis(100);

/// 单次会话级别的运行参数。
#[derive(Debug, Clone)]
pub struct ParaformerSessionOptions {
    pub model: String,
    pub format: String,
    pub request_timeout: Duration,
}

impl ParaformerSessionOptions {
    pub fn pcm() -> Self {
        Self {
            model: crate::dashscope::PARAFORMER_REALTIME_V2_MODEL.to_string(),
            format: "pcm".to_string(),
            request_timeout: Duration::from_secs(30),
        }
    }
}

/// DashScope Paraformer-realtime-v2 流式 ASR 引擎。
pub struct ParaformerCloudEngine {
    client: DashScopeClient,
    options: ParaformerSessionOptions,
}

impl ParaformerCloudEngine {
    pub fn new(client: DashScopeClient) -> Self {
        Self {
            client,
            options: ParaformerSessionOptions::pcm(),
        }
    }

    pub fn from_config(config: &CloudEngineConfig) -> Result<Self, CloudAsrError> {
        let client = DashScopeClient::from_config(config)?;
        let mut options = ParaformerSessionOptions::pcm();
        options.request_timeout = config.request_timeout;
        Ok(Self { client, options })
    }

    pub fn with_options(mut self, options: ParaformerSessionOptions) -> Self {
        self.options = options;
        self
    }

    pub fn client(&self) -> &DashScopeClient {
        &self.client
    }

    pub fn options(&self) -> &ParaformerSessionOptions {
        &self.options
    }
}

impl AsrEngine for ParaformerCloudEngine {
    fn transcribe(&self, pcm: &[i16], format: AudioFormat) -> Result<String, AsrError> {
        validate_audio(format, pcm).map_err(AsrError::from)?;
        let mono = downmix_mono(pcm, format);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| AsrError::Decode(format!("failed to build tokio runtime: {e}")))?;

        let result = runtime.block_on(run_session(
            &self.client,
            &self.options,
            &mono,
            format.sample_rate,
        ));
        result.map_err(AsrError::from)
    }
}

impl From<CloudAsrError> for AsrError {
    fn from(value: CloudAsrError) -> Self {
        match value {
            CloudAsrError::MissingApiKey => AsrError::ModelLoad("missing API key".into()),
            CloudAsrError::Auth(msg) => AsrError::ModelLoad(format!("auth failed: {msg}")),
            CloudAsrError::Network(msg) => AsrError::Network(msg),
            CloudAsrError::Timeout(d) => AsrError::Network(format!("timeout after {d:?}")),
            CloudAsrError::Protocol(msg) => AsrError::Decode(format!("protocol: {msg}")),
            CloudAsrError::Decode(msg) => AsrError::Decode(msg),
        }
    }
}

async fn run_session(
    client: &DashScopeClient,
    options: &ParaformerSessionOptions,
    pcm: &[i16],
    sample_rate: u32,
) -> Result<String, CloudAsrError> {
    let request = build_request(client)?;
    let (mut ws, _) = timeout(options.request_timeout, tokio_tungstenite::connect_async(request))
        .await
        .map_err(|_| CloudAsrError::Timeout(options.request_timeout))?
        .map_err(|e| CloudAsrError::Network(format!("websocket connect: {e}")))?;

    let params = RunTaskParams {
        task_id: crate::protocol::new_task_id(),
        model: options.model.clone(),
        format: options.format.clone(),
        sample_rate,
    };
    let task_id = params.task_id.clone();
    let run = ClientEvent::RunTask(params);
    ws.send(Message::Text(run.to_json()?))
        .await
        .map_err(|e| CloudAsrError::Network(format!("send run-task: {e}")))?;

    wait_for_started(&mut ws, &task_id, options.request_timeout).await?;

    let bytes_per_chunk =
        ((sample_rate as u128) * (CHUNK_DURATION.as_millis()) / 1000) as usize * 2;
    let bytes_per_chunk = bytes_per_chunk.max(2);

    let payload = pcm_to_le_bytes(pcm);
    for chunk in payload.chunks(bytes_per_chunk) {
        ws.send(Message::Binary(chunk.to_vec()))
            .await
            .map_err(|e| CloudAsrError::Network(format!("send pcm: {e}")))?;
    }

    let finish = ClientEvent::FinishTask {
        task_id: task_id.clone(),
    };
    ws.send(Message::Text(finish.to_json()?))
        .await
        .map_err(|e| CloudAsrError::Network(format!("send finish-task: {e}")))?;

    let text = collect_until_finished(&mut ws, &task_id, options.request_timeout).await?;
    let _ = ws.close(None).await;
    Ok(text)
}

fn build_request(
    client: &DashScopeClient,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, CloudAsrError> {
    let mut request = client
        .ws_endpoint()
        .into_client_request()
        .map_err(|e| CloudAsrError::Protocol(format!("invalid endpoint: {e}")))?;
    let headers = request.headers_mut();
    for (name, value) in client.auth_headers() {
        let header_value = HeaderValue::from_str(&value)
            .map_err(|e| CloudAsrError::Protocol(format!("invalid header {name}: {e}")))?;
        headers.insert(name, header_value);
    }
    Ok(request)
}

async fn wait_for_started<S>(
    ws: &mut S,
    task_id: &str,
    deadline: Duration,
) -> Result<(), CloudAsrError>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let msg = timeout(deadline, ws.next())
            .await
            .map_err(|_| CloudAsrError::Timeout(deadline))?
            .ok_or_else(|| CloudAsrError::Network("connection closed before task-started".into()))?
            .map_err(|e| CloudAsrError::Network(format!("read frame: {e}")))?;

        match msg {
            Message::Text(text) => match ServerEvent::from_json(&text)? {
                ServerEvent::TaskStarted { task_id: tid } if tid == task_id => return Ok(()),
                ServerEvent::TaskFailed { code, message, .. } => {
                    return Err(CloudAsrError::Decode(format!(
                        "task-failed before start: {code} {message}"
                    )));
                }
                _ => continue,
            },
            Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => continue,
            Message::Close(_) => {
                return Err(CloudAsrError::Network(
                    "server closed before task-started".into(),
                ));
            }
            Message::Frame(_) => continue,
        }
    }
}

async fn collect_until_finished<S>(
    ws: &mut S,
    task_id: &str,
    deadline: Duration,
) -> Result<String, CloudAsrError>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let mut finals: Vec<String> = Vec::new();
    let mut latest_partial = String::new();

    loop {
        let msg = timeout(deadline, ws.next())
            .await
            .map_err(|_| CloudAsrError::Timeout(deadline))?
            .ok_or_else(|| CloudAsrError::Network("connection closed before task-finished".into()))?
            .map_err(|e| CloudAsrError::Network(format!("read frame: {e}")))?;

        match msg {
            Message::Text(text) => match ServerEvent::from_json(&text)? {
                ServerEvent::ResultGenerated {
                    task_id: tid,
                    text,
                    is_final,
                } if tid == task_id => {
                    if is_final {
                        finals.push(text);
                        latest_partial.clear();
                    } else {
                        latest_partial = text;
                    }
                }
                ServerEvent::TaskFinished { task_id: tid } if tid == task_id => {
                    if finals.is_empty() && !latest_partial.is_empty() {
                        finals.push(std::mem::take(&mut latest_partial));
                    }
                    return Ok(finals.join(""));
                }
                ServerEvent::TaskFailed {
                    task_id: tid,
                    code,
                    message,
                } if tid == task_id => {
                    return Err(CloudAsrError::Decode(format!(
                        "task-failed: {code} {message}"
                    )));
                }
                _ => continue,
            },
            Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => continue,
            Message::Close(_) => {
                return Err(CloudAsrError::Network(
                    "server closed before task-finished".into(),
                ));
            }
            Message::Frame(_) => continue,
        }
    }
}

fn validate_audio(format: AudioFormat, pcm: &[i16]) -> Result<(), CloudAsrError> {
    if format.sample_rate == 0 {
        return Err(CloudAsrError::Protocol("sample rate must be positive".into()));
    }
    if format.channels == 0 {
        return Err(CloudAsrError::Protocol("channels must be positive".into()));
    }
    if pcm.len() % format.channels as usize != 0 {
        return Err(CloudAsrError::Protocol(format!(
            "{} samples is not divisible by {} channels",
            pcm.len(),
            format.channels
        )));
    }
    Ok(())
}

fn downmix_mono(pcm: &[i16], format: AudioFormat) -> Vec<i16> {
    let channels = format.channels as usize;
    if channels <= 1 {
        return pcm.to_vec();
    }
    let mut out = Vec::with_capacity(pcm.len() / channels);
    for frame in pcm.chunks_exact(channels) {
        let sum: i32 = frame.iter().map(|s| *s as i32).sum();
        let avg = sum / channels as i32;
        out.push(avg as i16);
    }
    out
}

fn pcm_to_le_bytes(pcm: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pcm.len() * 2);
    for sample in pcm {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paraformer_defaults_to_pcm_16k_model() {
        let options = ParaformerSessionOptions::pcm();
        assert_eq!(options.format, "pcm");
        assert_eq!(
            options.model,
            crate::dashscope::PARAFORMER_REALTIME_V2_MODEL
        );
    }

    #[test]
    fn cloud_error_maps_to_asr_error() {
        match AsrError::from(CloudAsrError::Auth("nope".into())) {
            AsrError::ModelLoad(msg) => assert!(msg.contains("auth failed")),
            other => panic!("expected ModelLoad, got {other:?}"),
        }
        match AsrError::from(CloudAsrError::Network("dns".into())) {
            AsrError::Network(msg) => assert_eq!(msg, "dns"),
            other => panic!("expected Network, got {other:?}"),
        }
        match AsrError::from(CloudAsrError::Timeout(Duration::from_secs(5))) {
            AsrError::Network(msg) => assert!(msg.contains("timeout")),
            other => panic!("expected Network, got {other:?}"),
        }
        match AsrError::from(CloudAsrError::Protocol("bad".into())) {
            AsrError::Decode(msg) => assert!(msg.contains("protocol")),
            other => panic!("expected Decode, got {other:?}"),
        }
        match AsrError::from(CloudAsrError::MissingApiKey) {
            AsrError::ModelLoad(msg) => assert!(msg.contains("missing API key")),
            other => panic!("expected ModelLoad, got {other:?}"),
        }
    }

    #[test]
    fn validate_audio_rejects_zero_sample_rate() {
        let err = validate_audio(
            AudioFormat {
                sample_rate: 0,
                channels: 1,
            },
            &[1, 2, 3],
        )
        .unwrap_err();
        assert!(matches!(err, CloudAsrError::Protocol(_)));
    }

    #[test]
    fn validate_audio_rejects_incomplete_frames() {
        let err = validate_audio(
            AudioFormat {
                sample_rate: 16_000,
                channels: 2,
            },
            &[1, 2, 3],
        )
        .unwrap_err();
        assert!(matches!(err, CloudAsrError::Protocol(_)));
    }

    #[test]
    fn downmix_passes_mono_through() {
        let pcm = vec![1, 2, 3, 4];
        let out = downmix_mono(
            &pcm,
            AudioFormat {
                sample_rate: 16_000,
                channels: 1,
            },
        );
        assert_eq!(out, pcm);
    }

    #[test]
    fn downmix_averages_stereo() {
        let pcm = vec![100, 200, 300, 400];
        let out = downmix_mono(
            &pcm,
            AudioFormat {
                sample_rate: 16_000,
                channels: 2,
            },
        );
        assert_eq!(out, vec![150, 350]);
    }

    #[test]
    fn pcm_to_le_bytes_is_little_endian() {
        let pcm = vec![1i16, -1];
        let bytes = pcm_to_le_bytes(&pcm);
        assert_eq!(bytes, vec![0x01, 0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn engine_constructs_from_config() {
        let config = CloudEngineConfig::dashscope("sk-test");
        let engine = ParaformerCloudEngine::from_config(&config).unwrap();
        assert_eq!(
            engine.options().model,
            crate::dashscope::PARAFORMER_REALTIME_V2_MODEL
        );
        assert_eq!(engine.options().request_timeout, config.request_timeout);
    }

    #[test]
    fn engine_rejects_empty_api_key_via_config() {
        let config = CloudEngineConfig::dashscope("");
        assert!(matches!(
            ParaformerCloudEngine::from_config(&config),
            Err(CloudAsrError::MissingApiKey)
        ));
    }
}
