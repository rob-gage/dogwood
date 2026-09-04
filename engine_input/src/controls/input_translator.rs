// Copyright Rob Gage 2026

use super::ControlState;

/// Translates keyboard and mouse input state into engine-specific controls
pub trait InputTranslator {

    /// Translates source input into a universal `ControlState`
    fn translate(&self, keyboard_input:(), mouse_input: ()) -> ControlState;

}
