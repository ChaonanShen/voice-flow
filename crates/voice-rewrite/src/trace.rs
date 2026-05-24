use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::profile::Profile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteTrace {
    pub profile: Profile,
    pub fallback: bool,
    pub llm_called: bool,
    pub preprocess_ms: u128,
    pub llm_ms: Option<u128>,
    pub error: Option<String>,
}

impl RewriteTrace {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            fallback: false,
            llm_called: false,
            preprocess_ms: 0,
            llm_ms: None,
            error: None,
        }
    }

    pub fn with_preprocess_duration(mut self, duration: Duration) -> Self {
        self.preprocess_ms = duration.as_millis();
        self
    }

    pub fn mark_llm_called(&mut self) {
        self.llm_called = true;
    }

    pub fn set_llm_duration(&mut self, duration: Duration) {
        self.llm_ms = Some(duration.as_millis());
    }

    pub fn mark_fallback(&mut self, error: impl Into<String>) {
        self.fallback = true;
        self.error = Some(error.into());
    }
}
