//! Simulated paste output.
//!
//! Step 4.5 sends the platform paste shortcut after text has been copied to
//! the system clipboard.

use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};

/// Simulates paste into the current focused application.
pub trait PasteSimulator {
    fn paste(&mut self) -> Result<(), PasteError>;
}

/// System input simulator backed by `enigo`.
#[derive(Default)]
pub struct SystemPaste;

impl SystemPaste {
    pub fn new() -> Self {
        Self
    }
}

impl PasteSimulator for SystemPaste {
    fn paste(&mut self) -> Result<(), PasteError> {
        let mut enigo = Enigo::new(&Settings::default())?;
        enigo.key(paste_modifier_key(), Press)?;
        let click = enigo.key(Key::Unicode('v'), Click);
        let release = enigo.key(paste_modifier_key(), Release);
        click?;
        release?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PasteError {
    #[error("paste simulator connection failed: {0}")]
    Connect(#[from] enigo::NewConError),
    #[error("paste input failed: {0}")]
    Input(#[from] enigo::InputError),
}

#[cfg(target_os = "macos")]
fn paste_modifier_key() -> Key {
    Key::Meta
}

#[cfg(not(target_os = "macos"))]
fn paste_modifier_key() -> Key {
    Key::Control
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_paste_implements_paste_simulator_trait() {
        fn assert_simulator<T: PasteSimulator>() {}
        assert_simulator::<SystemPaste>();
    }

    #[test]
    fn paste_modifier_matches_platform_convention() {
        #[cfg(target_os = "macos")]
        assert_eq!(paste_modifier_key(), Key::Meta);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(paste_modifier_key(), Key::Control);
    }
}
