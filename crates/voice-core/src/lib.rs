//! voice-core
//!
//! 录音、ASR 引擎路由与模式管道的核心 crate。

pub mod asr;
pub mod capture;
pub mod clipboard;
pub mod cpal_backend;
pub mod file_backend;
pub mod hotkey;
pub mod paste;
pub mod push_to_talk;
pub mod state;
pub mod wav;
