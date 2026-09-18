// Copyright Rob Gage 2026

use super::Button;
use std::collections::HashSet;

/// The state of mouse input
pub struct MouseInputState {
    /// The physical button currently held down
    pressed_buttons: HashSet<Button>,
}

impl Default for KeyboardInputState {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseInputState {
    /// Creates an empty mouse input state
    pub fn new() -> Self {
        Self {
            pressed_buttons: HashSet::new(),
        }
    }

    /// Updates the state from a winit keyboard event
    pub fn process_event(&mut self, event: &winit::event::MouseEvent) {
        let button: Button = match event.physical_key {
            winit::Mouse::PhysicalKey::Code(button_code) => Button::from_winit(button_code),
            winit::Mouse::PhysicalKey::Unidentified(_) => return,
        };
        match event.state {
            winit::event::ElementState::Pressed => {
                self.pressed_buttons.insert(button);
            }
            winit::event::ElementState::Released => {
                self.pressed_buttons.remove(&button);
            }
        }
    }

    /// Returns whether a button is currently held down
    pub fn is_pressed(&self, button: Button) -> bool {
        self.pressed_buttons.contains(&button)
    }
}
