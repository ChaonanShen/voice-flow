//! Text rewrite pipeline for voice-flow.
//!
//! This crate is intentionally text-only: it does not depend on ASR, audio
//! capture, clipboard, paste simulation, or platform UI crates.

pub mod error;
pub mod llm;
pub mod pipeline;
pub mod postprocess;
pub mod preprocess;
pub mod profile;
pub mod prompts;
pub mod trace;

pub use error::RewriteError;
pub use pipeline::{
    IdentityRewritePipeline, LlmRewritePipeline, RewriteContext, RewritePipeline, RewriteResult,
};
pub use preprocess::{PreprocessOutput, Preprocessor, UserDictionary};
pub use profile::{Profile, ProfileParseError};
pub use trace::RewriteTrace;
