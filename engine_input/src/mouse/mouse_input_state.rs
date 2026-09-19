// Copyright Rob Gage 2026

use super::Button;
use std::collections::HashSet;

/// The state of mouse input
pub struct MouseInputState {
    /// The physical button currently held down
    pressed_buttons: HashSet<Button>,
}

impl Default for MouseInputState {
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

    /// Updates the state from the `state` and `button` fields of a winit
    /// [`winit::event::WindowEvent::MouseInput`] event.
    pub fn process_event(
        &mut self,
        state: winit::event::ElementState,
        button: winit::event::MouseButton,
    ) {
        let button: Button = Button(button);
        match state {
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

#[cfg(test)]
mod tests {
    use super::MouseInputState;
    use crate::Button;
    use winit::event::{ElementState, MouseButton};

    #[test]
    fn tracks_pressed_and_released_buttons() {
        let mut input: MouseInputState = MouseInputState::new();

        input.process_event(ElementState::Pressed, MouseButton::Left);
        assert!(input.is_pressed(Button::LEFT));

        input.process_event(ElementState::Released, MouseButton::Left);
        assert!(!input.is_pressed(Button::LEFT));
    }
}
