// Copyright Rob Gage 2026

use super::{
    ControlState,
    InputTranslator,
};
use crate::keyboard::{
    Key,
    KeyboardInputState,
};

/// Translates configured keyboard keys into normalized locomotion controls
pub struct SimpleInputTranslator {
    /// Keys and their horizontal and vertical locomotion directions
    pub key_mappings: &'static [(Key, f32, f32)],
}

impl SimpleInputTranslator {

    /// A translator using the arrow keys for locomotion
    pub const ARROWS: Self = Self {
        key_mappings: &[
            (Key::ARROW_UP, 0.0, 1.0),
            (Key::ARROW_LEFT, -1.0, 0.0),
            (Key::ARROW_DOWN, 0.0, -1.0),
            (Key::ARROW_RIGHT, 1.0, 0.0),
        ],
    };

    /// A translator using WASD for locomotion
    pub const WASD: Self = Self {
        key_mappings: &[
            (Key::W, 0.0, 1.0),
            (Key::A, -1.0, 0.0),
            (Key::S, 0.0, -1.0),
            (Key::D, 1.0, 0.0),
        ],
    };

    /// A translator using both arrow and WASD keys for locomotion
    pub const ARROWS_AND_WASD: Self = Self {
        key_mappings: &[
            (Key::W, 0.0, 1.0),
            (Key::A, -1.0, 0.0),
            (Key::S, 0.0, -1.0),
            (Key::D, 1.0, 0.0),
            (Key::ARROW_UP, 0.0, 1.0),
            (Key::ARROW_LEFT, -1.0, 0.0),
            (Key::ARROW_DOWN, 0.0, -1.0),
            (Key::ARROW_RIGHT, 1.0, 0.0),
        ],
    };

}

impl InputTranslator for SimpleInputTranslator {

    fn translate(&self, keyboard_input: &KeyboardInputState) -> ControlState {
        let mut locomotion_x: f32 = 0.0;
        let mut locomotion_y: f32 = 0.0;
        for (key, x, y) in self.key_mappings {
            if keyboard_input.is_pressed(*key) {
                locomotion_x += x;
                locomotion_y += y;
            }
        }
        let magnitude_squared: f32 = locomotion_x * locomotion_x + locomotion_y * locomotion_y;
        if magnitude_squared > 1.0 {
            let magnitude: f32 = magnitude_squared.sqrt();
            locomotion_x /= magnitude;
            locomotion_y /= magnitude;
        }
        ControlState { locomotion_x, locomotion_y }
    }

}
