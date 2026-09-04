// Copyright Rob Gage 2026

use super::Key;
use std::collections::HashSet;

/// The state of keyboard input
pub struct KeyboardInputState {
    /// The physical keys currently held down
    pressed_keys: HashSet<Key>,
}

impl KeyboardInputState {

    /// Creates an empty keyboard input state
    pub fn new() -> Self { Self { pressed_keys: HashSet::new() } }

    /// Updates the state from a winit keyboard event
    pub fn process_event(&mut self, event: &winit::event::KeyEvent) {
        let key: Key = match event.physical_key {
            winit::keyboard::PhysicalKey::Code(key_code) => Key::from_winit(key_code),
            winit::keyboard::PhysicalKey::Unidentified(_) => return,
        };
        match event.state {
            winit::event::ElementState::Pressed => { self.pressed_keys.insert(key); },
            winit::event::ElementState::Released => { self.pressed_keys.remove(&key); },
        }
    }

    /// Returns whether a key is currently held down
    pub fn is_pressed(&self, key: Key) -> bool { self.pressed_keys.contains(&key) }

}
