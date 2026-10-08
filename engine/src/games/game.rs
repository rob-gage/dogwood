// Copyright Rob Gage 2026

use engine_host::system_events::{
    SystemKeyboardEvent,
    SystemMouseEvent,
    SystemWindowEvent,
};
use std::time::Duration;

/// A game that runs on the `Engine`
pub trait Game {

    /// Advances game state by one fixed update step
    fn update(&mut self, _elapsed_time: Duration) {}

    /// Renders the game after a native redraw request
    fn render(&mut self) {}

    /// Handles a host window event
    fn window_event(&mut self, _event: SystemWindowEvent) {}

    /// Handles a host keyboard event
    fn keyboard_event(&mut self, _event: SystemKeyboardEvent) {}

    /// Handles a host mouse event
    fn mouse_event(&mut self, _event: SystemMouseEvent) {}

}
