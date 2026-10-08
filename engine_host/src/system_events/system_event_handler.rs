// Copyright Rob Gage 2026

use crate::system_events::{
    SystemKeyboardEvent,
    SystemMouseEvent,
    SystemWindowEvent,
};

/// Receives independent host window and input event categories
pub trait SystemEventHandler {

    /// Handles a window event
    fn window_event(&mut self, event: SystemWindowEvent);

    /// Handles a keyboard event
    fn keyboard_event(&mut self, event: SystemKeyboardEvent);

    /// Handles a mouse event
    fn mouse_event(&mut self, event: SystemMouseEvent);

}
