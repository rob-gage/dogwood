// Copyright Rob Gage 2026

//! Input state and device-to-game control translation.

pub mod controls;
pub mod keyboard;

pub use controls::{ControlState, InputTranslator, SimpleInputTranslator};
pub use keyboard::{Key, KeyboardInputState};
