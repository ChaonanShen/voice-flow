//! System clipboard output.
//!
//! Step 4.4 only writes recognized text to the clipboard. Simulating paste is
//! intentionally left to the next PR.

/// Writes text to a clipboard-like target.
pub trait ClipboardWriter {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

/// System clipboard backed by `arboard`.
#[derive(Default)]
pub struct SystemClipboard;

impl SystemClipboard {
    pub fn new() -> Self {
        Self
    }
}

impl ClipboardWriter for SystemClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        let mut clipboard = arboard::Clipboard::new()?;
        clipboard.set_text(text.to_string())?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard error: {0}")]
    Backend(#[from] arboard::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clipboard_implements_writer_trait() {
        fn assert_writer<T: ClipboardWriter>() {}
        assert_writer::<SystemClipboard>();
    }
}
