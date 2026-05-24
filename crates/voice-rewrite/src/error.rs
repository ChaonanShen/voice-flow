use std::time::Duration;

use crate::llm::LlmError;
use crate::profile::ProfileParseError;

#[derive(Debug, thiserror::Error)]
pub enum RewriteError {
    #[error("unsupported profile: {0}")]
    UnsupportedProfile(String),
    #[error("invalid profile: {0}")]
    InvalidProfile(#[from] ProfileParseError),
    #[error("llm error: {0}")]
    Llm(#[from] LlmError),
    #[error("rewrite timed out after {0:?}")]
    Timeout(Duration),
    #[error("invalid llm output: {0}")]
    InvalidOutput(String),
}
