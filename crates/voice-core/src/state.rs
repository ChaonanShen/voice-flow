//! Realtime state model shared by CLI and desktop shells.

use serde::{Deserialize, Serialize};

/// High-level state of the realtime voice input flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeState {
    Idle,
    Recording,
    Transcribing,
    Completed,
}

impl RealtimeState {
    /// Human-readable label suitable for logs and UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
            Self::Completed => "completed",
        }
    }
}

/// Event payload for state changes in the realtime pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealtimeStateEvent {
    pub state: RealtimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
}

impl RealtimeStateEvent {
    pub fn new(state: RealtimeState) -> Self {
        Self {
            state,
            transcript: None,
        }
    }

    pub fn with_transcript(mut self, transcript: impl Into<String>) -> Self {
        self.transcript = Some(transcript.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_stable() {
        assert_eq!(RealtimeState::Idle.label(), "idle");
        assert_eq!(RealtimeState::Recording.label(), "recording");
        assert_eq!(RealtimeState::Transcribing.label(), "transcribing");
        assert_eq!(RealtimeState::Completed.label(), "completed");
    }

    #[test]
    fn events_round_trip_through_toml() {
        let event = RealtimeStateEvent::new(RealtimeState::Completed).with_transcript("你好");
        let doc = toml::to_string(&event).unwrap();
        let parsed: RealtimeStateEvent = toml::from_str(&doc).unwrap();
        assert_eq!(parsed, event);
    }
}
