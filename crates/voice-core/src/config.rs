//! Minimal TOML config for the realtime path.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub model_dir: Option<String>,
    pub hotkey: HotkeyConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model_dir: None,
            hotkey: HotkeyConfig::default(),
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
    }
}
