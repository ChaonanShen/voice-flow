//! Minimal TOML config for the realtime path.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::engine::EngineKind;
use voice_rewrite::{Profile, RewriteProvider, DEFAULT_REWRITE_MODEL, DEFAULT_REWRITE_TIMEOUT};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DesktopOutputModeConfig {
    #[default]
    FloatingInput,
    VoicePad,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub model_dir: Option<String>,
    pub hotkey: HotkeyConfig,
    pub asr: AsrConfig,
    pub desktop: DesktopConfig,
    pub rewrite: RewriteConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model_dir: None,
            hotkey: HotkeyConfig::default(),
            asr: AsrConfig::default(),
            desktop: DesktopConfig::default(),
            rewrite: RewriteConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    pub key: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            ctrl: true,
            alt: true,
            shift: false,
            logo: false,
            key: "Space".to_string(),
        }
    }
}

impl HotkeyConfig {
    pub fn to_label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.logo {
            parts.push("Super".to_string());
        }
        parts.push(self.key.clone());
        parts.join("+")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AsrConfig {
    pub engine: EngineKind,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            engine: EngineKind::Local,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopConfig {
    pub output_mode: DesktopOutputModeConfig,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            output_mode: DesktopOutputModeConfig::FloatingInput,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RewriteConfig {
    pub enabled: bool,
    pub provider: RewriteProvider,
    pub model: Option<String>,
    pub default_profile: Profile,
    pub timeout_ms: u64,
    pub user_dictionary: HashMap<String, String>,
}

impl Default for RewriteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: RewriteProvider::DeepSeek,
            model: Some(DEFAULT_REWRITE_MODEL.to_string()),
            default_profile: Profile::Clean,
            timeout_ms: DEFAULT_REWRITE_TIMEOUT.as_millis() as u64,
            user_dictionary: HashMap::new(),
        }
    }
}

impl AppConfig {
    pub fn read_from(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let text = toml::to_string_pretty(self).map_err(|source| ConfigError::Serialize {
            path: path.to_path_buf(),
            source,
        })?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(path, text).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config from {path}: {source}")]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config from {path}: {source}")]
    Parse {
        path: std::path::PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize config for {path}: {source}")]
    Serialize {
        path: std::path::PathBuf,
        #[source]
        source: toml::ser::Error,
    },
    #[error("failed to write config to {path}: {source}")]
    Write {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn defaults_match_windows_first_hotkey() {
        let config = AppConfig::default();
        assert_eq!(config.model_dir, None);
        assert_eq!(config.hotkey.ctrl, true);
        assert_eq!(config.hotkey.alt, true);
        assert_eq!(config.hotkey.shift, false);
        assert_eq!(config.hotkey.logo, false);
        assert_eq!(config.hotkey.key, "Space");
        assert_eq!(config.hotkey.to_label(), "Ctrl+Alt+Space");
        assert_eq!(config.asr, AsrConfig::default());
        assert_eq!(config.desktop, DesktopConfig::default());
        assert_eq!(config.rewrite, RewriteConfig::default());
    }

    #[test]
    fn read_write_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.toml");

        let config = AppConfig {
            model_dir: Some("C:/models".to_string()),
            hotkey: HotkeyConfig {
                ctrl: true,
                alt: false,
                shift: true,
                logo: false,
                key: "M".to_string(),
            },
            asr: AsrConfig {
                engine: EngineKind::Cloud,
            },
            desktop: DesktopConfig {
                output_mode: DesktopOutputModeConfig::VoicePad,
            },
            rewrite: RewriteConfig {
                enabled: true,
                provider: RewriteProvider::DashScope,
                model: Some("qwen-plus".to_string()),
                default_profile: Profile::Email,
                timeout_ms: 3_000,
                user_dictionary: HashMap::from([("克劳德".to_string(), "Claude".to_string())]),
            },
        };
        config.write_to(&path).unwrap();

        let loaded = AppConfig::read_from(&path).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn parse_missing_fields_get_defaults() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.toml");
        fs::write(&path, "model_dir = 'C:/models'\n").unwrap();

        let loaded = AppConfig::read_from(&path).unwrap();
        assert_eq!(loaded.model_dir.as_deref(), Some("C:/models"));
        assert_eq!(loaded.hotkey, HotkeyConfig::default());
        assert_eq!(loaded.asr, AsrConfig::default());
        assert_eq!(loaded.desktop, DesktopConfig::default());
        assert_eq!(loaded.rewrite, RewriteConfig::default());
    }

    #[test]
    fn parse_rewrite_section() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.toml");
        fs::write(
            &path,
            r#"
[rewrite]
enabled = true
provider = "openai"
model = "gpt-4o-mini"
default_profile = "prompt"
timeout_ms = 2500

[rewrite.user_dictionary]
"克劳德" = "Claude"
"我推" = "Vue"
"#,
        )
        .unwrap();

        let loaded = AppConfig::read_from(&path).unwrap();
        assert!(loaded.rewrite.enabled);
        assert_eq!(loaded.rewrite.provider, RewriteProvider::OpenAi);
        assert_eq!(loaded.rewrite.model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(loaded.rewrite.default_profile, Profile::Prompt);
        assert_eq!(loaded.rewrite.timeout_ms, 2_500);
        assert_eq!(loaded.rewrite.user_dictionary["克劳德"], "Claude");
        assert_eq!(loaded.rewrite.user_dictionary["我推"], "Vue");
    }

    #[test]
    fn parse_asr_section() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.toml");
        fs::write(
            &path,
            r#"
[asr]
engine = "cloud"
"#,
        )
        .unwrap();

        let loaded = AppConfig::read_from(&path).unwrap();
        assert_eq!(loaded.asr.engine, EngineKind::Cloud);
    }

    #[test]
    fn parse_desktop_section() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.toml");
        fs::write(
            &path,
            r#"
[desktop]
output_mode = "voice_pad"
"#,
        )
        .unwrap();

        let loaded = AppConfig::read_from(&path).unwrap();
        assert_eq!(loaded.desktop.output_mode, DesktopOutputModeConfig::VoicePad);
    }
}
