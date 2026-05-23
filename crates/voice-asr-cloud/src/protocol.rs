//! Paraformer-realtime-v2 流式协议编解码。
//!
//! 协议形态（DashScope 通用流式接口）：
//! - 客户端先发 `run-task`（JSON 文本帧）开启会话
//! - 客户端按真实时序推 PCM（二进制帧）
//! - 服务器异步推 `task-started` / `result-generated` / `task-finished` /
//!   `task-failed`（JSON 文本帧）
//! - 客户端发 `finish-task` 触发服务器完成
//!
//! 本模块只负责 JSON 编解码，不涉及网络。WebSocket 接线在 PR 8.4。
//!
//! 参考：<https://help.aliyun.com/zh/dashscope/developer-reference/api-details>

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dashscope::PARAFORMER_REALTIME_V2_MODEL;
use crate::CloudAsrError;

/// 客户端 → 服务器控制事件。会被序列化为单个 JSON 文本帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    RunTask(RunTaskParams),
    FinishTask { task_id: String },
}

impl ClientEvent {
    pub fn task_id(&self) -> &str {
        match self {
            Self::RunTask(params) => &params.task_id,
            Self::FinishTask { task_id } => task_id,
        }
    }

    /// 序列化为 DashScope 通用流式接口规定的 JSON 帧。
    pub fn to_json(&self) -> Result<String, CloudAsrError> {
        let envelope = match self {
            Self::RunTask(params) => Envelope {
                header: Header {
                    action: "run-task",
                    task_id: params.task_id.clone(),
                    streaming: "duplex",
                },
                payload: Payload::RunTask(RunTaskPayload {
                    task_group: "audio",
                    task: "asr",
                    function: "recognition",
                    model: params.model.clone(),
                    input: serde_json::json!({}),
                    parameters: RunTaskParameters {
                        format: params.format.clone(),
                        sample_rate: params.sample_rate,
                    },
                }),
            },
            Self::FinishTask { task_id } => Envelope {
                header: Header {
                    action: "finish-task",
                    task_id: task_id.clone(),
                    streaming: "duplex",
                },
                payload: Payload::FinishTask {
                    input: serde_json::json!({}),
                },
            },
        };

        serde_json::to_string(&envelope)
            .map_err(|e| CloudAsrError::Protocol(format!("serialize {}: {e}", envelope.header.action)))
    }
}

/// `run-task` 参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTaskParams {
    pub task_id: String,
    pub model: String,
    /// `pcm` / `wav` / `opus` 等。
    pub format: String,
    pub sample_rate: u32,
}

impl RunTaskParams {
    /// Paraformer-realtime-v2 + 16kHz PCM 的默认参数。
    pub fn paraformer_realtime_pcm(sample_rate: u32) -> Self {
        Self {
            task_id: new_task_id(),
            model: PARAFORMER_REALTIME_V2_MODEL.to_string(),
            format: "pcm".to_string(),
            sample_rate,
        }
    }
}

/// 服务器 → 客户端事件。来自单个 JSON 文本帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// 会话已建立，可以开始推 PCM。
    TaskStarted { task_id: String },
    /// 中间或最终识别结果。`is_final` 表示是否本句已锁定。
    ResultGenerated {
        task_id: String,
        text: String,
        is_final: bool,
    },
    /// 会话完成（在收到 `finish-task` 之后，服务器把最后结果冲刷完毕）。
    TaskFinished { task_id: String },
    /// 会话失败。
    TaskFailed {
        task_id: String,
        code: String,
        message: String,
    },
}

impl ServerEvent {
    pub fn task_id(&self) -> &str {
        match self {
            Self::TaskStarted { task_id }
            | Self::ResultGenerated { task_id, .. }
            | Self::TaskFinished { task_id }
            | Self::TaskFailed { task_id, .. } => task_id,
        }
    }

    /// 反序列化服务器 JSON 文本帧。未知 event 会被映射为 [`CloudAsrError::Protocol`]，
    /// 而非静默吞掉，避免协议偏移被埋。
    pub fn from_json(text: &str) -> Result<Self, CloudAsrError> {
        let raw: RawServerFrame = serde_json::from_str(text)
            .map_err(|e| CloudAsrError::Protocol(format!("parse server frame: {e}")))?;
        let task_id = raw.header.task_id;
        match raw.header.event.as_str() {
            "task-started" => Ok(Self::TaskStarted { task_id }),
            "result-generated" => {
                let output = raw
                    .payload
                    .ok_or_else(|| CloudAsrError::Protocol("result-generated missing payload".into()))?;
                let sentence = output
                    .output
                    .as_ref()
                    .and_then(|o| o.sentence.as_ref())
                    .ok_or_else(|| {
                        CloudAsrError::Protocol("result-generated missing sentence".into())
                    })?;
                let text = sentence.text.clone().unwrap_or_default();
                let is_final = sentence
                    .sentence_end
                    .or(sentence.is_final)
                    .unwrap_or(false);
                Ok(Self::ResultGenerated {
                    task_id,
                    text,
                    is_final,
                })
            }
            "task-finished" => Ok(Self::TaskFinished { task_id }),
            "task-failed" => Ok(Self::TaskFailed {
                task_id,
                code: raw.header.error_code.unwrap_or_default(),
                message: raw.header.error_message.unwrap_or_default(),
            }),
            other => Err(CloudAsrError::Protocol(format!("unknown event: {other}"))),
        }
    }
}

pub fn new_task_id() -> String {
    Uuid::new_v4().simple().to_string()
}

// --- Wire format ----------------------------------------------------------
// 以下私有结构对齐 DashScope 流式接口的 JSON 形状，不在 crate 外暴露。

#[derive(Debug, Serialize)]
struct Envelope {
    header: Header,
    payload: Payload,
}

#[derive(Debug, Serialize)]
struct Header {
    action: &'static str,
    task_id: String,
    streaming: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Payload {
    RunTask(RunTaskPayload),
    FinishTask {
        input: serde_json::Value,
    },
}

#[derive(Debug, Serialize)]
struct RunTaskPayload {
    task_group: &'static str,
    task: &'static str,
    function: &'static str,
    model: String,
    input: serde_json::Value,
    parameters: RunTaskParameters,
}

#[derive(Debug, Serialize)]
struct RunTaskParameters {
    format: String,
    sample_rate: u32,
}

#[derive(Debug, Deserialize)]
struct RawServerFrame {
    header: RawHeader,
    #[serde(default)]
    payload: Option<RawPayload>,
}

#[derive(Debug, Deserialize)]
struct RawHeader {
    event: String,
    task_id: String,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPayload {
    #[serde(default)]
    output: Option<RawOutput>,
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    #[serde(default)]
    sentence: Option<RawSentence>,
}

#[derive(Debug, Deserialize)]
struct RawSentence {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    sentence_end: Option<bool>,
    #[serde(default)]
    is_final: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn run_task_serializes_required_fields() {
        let event = ClientEvent::RunTask(RunTaskParams {
            task_id: "tid-1".into(),
            model: PARAFORMER_REALTIME_V2_MODEL.into(),
            format: "pcm".into(),
            sample_rate: 16_000,
        });
        let json: Value = serde_json::from_str(&event.to_json().unwrap()).unwrap();

        assert_eq!(json["header"]["action"], "run-task");
        assert_eq!(json["header"]["task_id"], "tid-1");
        assert_eq!(json["header"]["streaming"], "duplex");
        assert_eq!(json["payload"]["task_group"], "audio");
        assert_eq!(json["payload"]["task"], "asr");
        assert_eq!(json["payload"]["function"], "recognition");
        assert_eq!(json["payload"]["model"], PARAFORMER_REALTIME_V2_MODEL);
        assert_eq!(json["payload"]["parameters"]["format"], "pcm");
        assert_eq!(json["payload"]["parameters"]["sample_rate"], 16_000);
    }

    #[test]
    fn finish_task_serializes_minimal_envelope() {
        let event = ClientEvent::FinishTask {
            task_id: "tid-1".into(),
        };
        let json: Value = serde_json::from_str(&event.to_json().unwrap()).unwrap();

        assert_eq!(json["header"]["action"], "finish-task");
        assert_eq!(json["header"]["task_id"], "tid-1");
        assert!(json["payload"]["input"].is_object());
    }

    #[test]
    fn parses_task_started() {
        let frame = r#"{"header":{"event":"task-started","task_id":"tid-1"}}"#;
        let event = ServerEvent::from_json(frame).unwrap();
        assert_eq!(
            event,
            ServerEvent::TaskStarted {
                task_id: "tid-1".into()
            }
        );
    }

    #[test]
    fn parses_partial_result_generated() {
        let frame = r#"{
            "header": {"event":"result-generated","task_id":"tid-1"},
            "payload": {"output":{"sentence":{"text":"你好","sentence_end":false}}}
        }"#;
        let event = ServerEvent::from_json(frame).unwrap();
        assert_eq!(
            event,
            ServerEvent::ResultGenerated {
                task_id: "tid-1".into(),
                text: "你好".into(),
                is_final: false,
            }
        );
    }

    #[test]
    fn parses_final_result_generated() {
        let frame = r#"{
            "header": {"event":"result-generated","task_id":"tid-1"},
            "payload": {"output":{"sentence":{"text":"你好世界","sentence_end":true}}}
        }"#;
        let event = ServerEvent::from_json(frame).unwrap();
        match event {
            ServerEvent::ResultGenerated { is_final, text, .. } => {
                assert!(is_final);
                assert_eq!(text, "你好世界");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn result_generated_text_defaults_to_empty() {
        let frame = r#"{
            "header": {"event":"result-generated","task_id":"tid-1"},
            "payload": {"output":{"sentence":{"sentence_end":false}}}
        }"#;
        let event = ServerEvent::from_json(frame).unwrap();
        match event {
            ServerEvent::ResultGenerated { text, is_final, .. } => {
                assert_eq!(text, "");
                assert!(!is_final);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_task_finished_and_failed() {
        let finished = ServerEvent::from_json(
            r#"{"header":{"event":"task-finished","task_id":"tid-1"}}"#,
        )
        .unwrap();
        assert_eq!(
            finished,
            ServerEvent::TaskFinished {
                task_id: "tid-1".into()
            }
        );

        let failed = ServerEvent::from_json(
            r#"{"header":{"event":"task-failed","task_id":"tid-1","error_code":"E1","error_message":"boom"}}"#,
        )
        .unwrap();
        assert_eq!(
            failed,
            ServerEvent::TaskFailed {
                task_id: "tid-1".into(),
                code: "E1".into(),
                message: "boom".into(),
            }
        );
    }

    #[test]
    fn unknown_event_is_a_protocol_error() {
        let err =
            ServerEvent::from_json(r#"{"header":{"event":"surprise","task_id":"tid-1"}}"#)
                .unwrap_err();
        assert!(matches!(err, CloudAsrError::Protocol(_)));
        assert!(err.to_string().contains("unknown event"));
    }

    #[test]
    fn malformed_frame_is_a_protocol_error() {
        let err = ServerEvent::from_json("not json").unwrap_err();
        assert!(matches!(err, CloudAsrError::Protocol(_)));
    }

    #[test]
    fn new_task_id_is_non_empty_and_unique() {
        let a = new_task_id();
        let b = new_task_id();
        assert!(!a.is_empty());
        assert_ne!(a, b);
    }

    #[test]
    fn paraformer_realtime_defaults_match_model() {
        let params = RunTaskParams::paraformer_realtime_pcm(16_000);
        assert_eq!(params.model, PARAFORMER_REALTIME_V2_MODEL);
        assert_eq!(params.format, "pcm");
        assert_eq!(params.sample_rate, 16_000);
        assert!(!params.task_id.is_empty());
    }
}
