// Copyright Rob Gage 2026

use super::ControlState;
use crate::keyboard::KeyboardInputState;

/// Translates input state into universal controls
pub trait InputTranslator: Send + Sync {
    /// Translates keyboard input into a universal `ControlState`
    fn translate(&self, keyboard_input: &KeyboardInputState) -> ControlState;
}
