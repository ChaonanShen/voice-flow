//! ASR 引擎选择（local / cloud）。
//!
//! `voice-core` 不直接依赖具体引擎 crate，避免循环依赖；这里只定义
//! **用户配置 → 引擎种类 + 参数**的形状，由 CLI / 桌面壳负责按 [`EngineSelection`]
//! 实例化具体引擎（端侧 sherpa-onnx 或云端 DashScope）。
//!
//! 设计约束：**无静默 fallback**。用户选 cloud 但 key 缺失、网络炸了，应明确
//! 报错，而不是偷偷切本地——选 cloud 通常是为了精度而非可用性，悄悄降级反而
//! 让"识别变差"难以排查。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// ASR 引擎种类标签。仅作为序列化键和 CLI flag 取值使用，不直接代表实例。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Local,
    Cloud,
}

impl EngineKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
        }
    }
}

impl Default for EngineKind {
    fn default() -> Self {
        Self::Local
    }
}

/// 端侧引擎参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalEngineParams {
    /// sherpa-onnx 模型目录。
    pub model_dir: PathBuf,
}

/// 云端引擎参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudEngineParams {
    /// 云端 API key（明文）。桌面端持久化时应通过系统 keyring 而非 TOML。
    pub api_key: String,
}

/// 引擎选择 + 参数。CLI / 桌面壳消费这个枚举生成实际引擎。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineSelection {
    Local(LocalEngineParams),
    Cloud(CloudEngineParams),
}

impl EngineSelection {
    pub fn kind(&self) -> EngineKind {
        match self {
            Self::Local(_) => EngineKind::Local,
            Self::Cloud(_) => EngineKind::Cloud,
        }
    }

    pub fn label(&self) -> &'static str {
        self.kind().label()
    }
}

/// 解析失败原因。用户给了不完整的输入时立即报错，不静默回退。
#[derive(Debug, thiserror::Error)]
pub enum EngineSelectionError {
    #[error("local engine requires a model directory")]
    MissingModelDir,
    #[error("local engine model directory does not exist: {0}")]
    ModelDirMissing(PathBuf),
    #[error("cloud engine requires an API key")]
    MissingApiKey,
}

/// 把 CLI flag / 配置文件值解析为完整的 [`EngineSelection`]。
///
/// `model_dir` 和 `api_key` 在调用方按 kind 提供——本函数不读环境变量，
/// 让调用方决定从哪里取（CLI 从 flag/env，桌面壳从 TOML + keyring）。
pub fn resolve_engine_selection(
    kind: EngineKind,
    model_dir: Option<PathBuf>,
    api_key: Option<String>,
) -> Result<EngineSelection, EngineSelectionError> {
    match kind {
        EngineKind::Local => {
            let model_dir = model_dir.ok_or(EngineSelectionError::MissingModelDir)?;
            if !model_dir.exists() {
                return Err(EngineSelectionError::ModelDirMissing(model_dir));
            }
            Ok(EngineSelection::Local(LocalEngineParams { model_dir }))
        }
        EngineKind::Cloud => {
            let api_key = api_key
                .filter(|k| !k.trim().is_empty())
                .ok_or(EngineSelectionError::MissingApiKey)?;
            Ok(EngineSelection::Cloud(CloudEngineParams { api_key }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn labels_match_serde_keys() {
        assert_eq!(EngineKind::Local.label(), "local");
        assert_eq!(EngineKind::Cloud.label(), "cloud");
    }

    #[test]
    fn engine_kind_round_trips_through_toml() {
        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
        struct Wrap {
            engine: EngineKind,
        }
        let raw = toml::to_string(&Wrap {
            engine: EngineKind::Cloud,
        })
        .unwrap();
        let parsed: Wrap = toml::from_str(&raw).unwrap();
        assert_eq!(parsed.engine, EngineKind::Cloud);
    }

    #[test]
    fn local_selection_requires_existing_dir() {
        let dir = tempdir().unwrap();
        let sel = resolve_engine_selection(EngineKind::Local, Some(dir.path().to_path_buf()), None)
            .unwrap();
        assert_eq!(sel.kind(), EngineKind::Local);
    }

    #[test]
    fn local_selection_rejects_missing_dir() {
        let err = resolve_engine_selection(
            EngineKind::Local,
            Some(PathBuf::from("/no/such/dir/voice-flow-test")),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, EngineSelectionError::ModelDirMissing(_)));
    }

    #[test]
    fn local_selection_rejects_missing_model_dir_argument() {
        let err = resolve_engine_selection(EngineKind::Local, None, None).unwrap_err();
        assert!(matches!(err, EngineSelectionError::MissingModelDir));
    }

    #[test]
    fn cloud_selection_requires_non_empty_api_key() {
        let sel = resolve_engine_selection(EngineKind::Cloud, None, Some("sk-xxx".into())).unwrap();
        match sel {
            EngineSelection::Cloud(params) => assert_eq!(params.api_key, "sk-xxx"),
            other => panic!("expected Cloud, got {other:?}"),
        }
    }

    #[test]
    fn cloud_selection_rejects_blank_api_key() {
        let err = resolve_engine_selection(EngineKind::Cloud, None, Some("   ".into())).unwrap_err();
        assert!(matches!(err, EngineSelectionError::MissingApiKey));

        let err = resolve_engine_selection(EngineKind::Cloud, None, None).unwrap_err();
        assert!(matches!(err, EngineSelectionError::MissingApiKey));
    }

    #[test]
    fn default_engine_is_local() {
        assert_eq!(EngineKind::default(), EngineKind::Local);
    }
}
