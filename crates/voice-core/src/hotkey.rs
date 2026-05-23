//! Global hotkey registration for push-to-talk.
//!
//! Step 4.1 only proves that the configured shortcut can be registered and
//! observed. Recording, ASR, clipboard, and paste are wired in later PRs.

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};

use crate::config::HotkeyConfig;

/// Default push-to-talk shortcut used by the minimal realtime loop.
pub const PUSH_TO_TALK_HOTKEY_LABEL: &str = "Ctrl+Alt+Space";

/// A push-to-talk hotkey event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushToTalkEvent {
    Pressed,
    Released,
}

impl std::fmt::Display for PushToTalkEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pressed => write!(f, "pressed"),
            Self::Released => write!(f, "released"),
        }
    }
}

/// Holds the platform registration alive for the default push-to-talk hotkey.
pub struct PushToTalkHotkey {
    _manager: GlobalHotKeyManager,
    hotkey: HotKey,
}

impl PushToTalkHotkey {
    /// Register `Ctrl+Alt+Space` and start receiving events through
    /// [`Self::try_recv`].
    pub fn register_default() -> Result<Self, HotkeyError> {
        Self::register(HotkeyConfig::default())
    }

    /// Register the configured push-to-talk shortcut and start receiving events
    /// through [`Self::try_recv`].
    pub fn register(config: HotkeyConfig) -> Result<Self, HotkeyError> {
        ensure_supported_session()?;

        let manager = GlobalHotKeyManager::new()?;
        let hotkey = hotkey_from_config(&config)?;
        manager.register(hotkey)?;

        Ok(Self {
            _manager: manager,
            hotkey,
        })
    }

    /// Return the next matching hotkey event if one is available.
    pub fn try_recv(&self) -> Result<Option<PushToTalkEvent>, HotkeyError> {
        loop {
            match GlobalHotKeyEvent::receiver().try_recv() {
                Ok(event) if event.id == self.hotkey.id() => {
                    return Ok(Some(match event.state {
                        HotKeyState::Pressed => PushToTalkEvent::Pressed,
                        HotKeyState::Released => PushToTalkEvent::Released,
                    }));
                }
                Ok(_) => continue,
                Err(_) => return Ok(None),
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    #[error("unsupported hotkey session: {0}")]
    UnsupportedSession(String),
    #[error("invalid hotkey: {0}")]
    Invalid(String),
    #[error("hotkey backend error: {0}")]
    Backend(#[from] global_hotkey::Error),
}

pub fn default_push_to_talk_hotkey() -> HotKey {
    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space)
}

pub fn hotkey_from_config(config: &HotkeyConfig) -> Result<HotKey, HotkeyError> {
    let label = config.to_label();
    label
        .parse::<HotKey>()
        .map_err(|e| HotkeyError::Invalid(e.to_string()))
}

#[cfg(target_os = "linux")]
fn ensure_supported_session() -> Result<(), HotkeyError> {
    if std::env::var_os("DISPLAY").is_none() {
        return Err(HotkeyError::UnsupportedSession(
            "global-hotkey supports Linux on X11 only; DISPLAY is not set".to_string(),
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn ensure_supported_session() -> Result<(), HotkeyError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkey_is_ctrl_alt_space() {
        let hotkey = default_push_to_talk_hotkey();

        assert!(hotkey.mods.contains(Modifiers::CONTROL));
        assert!(hotkey.mods.contains(Modifiers::ALT));
        assert_eq!(hotkey.key, Code::Space);
        assert_eq!(PUSH_TO_TALK_HOTKEY_LABEL, "Ctrl+Alt+Space");
    }

    #[test]
    fn default_config_builds_default_hotkey() {
        let hotkey = hotkey_from_config(&HotkeyConfig::default()).unwrap();

        assert!(hotkey.mods.contains(Modifiers::CONTROL));
        assert!(hotkey.mods.contains(Modifiers::ALT));
        assert_eq!(hotkey.key, Code::Space);
    }

    #[test]
    fn push_to_talk_event_display_is_log_friendly() {
        assert_eq!(PushToTalkEvent::Pressed.to_string(), "pressed");
        assert_eq!(PushToTalkEvent::Released.to_string(), "released");
    }
}
