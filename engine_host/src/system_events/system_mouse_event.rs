// Copyright Rob Gage 2026

/// Mouse input event produced by the host system
pub enum SystemMouseEvent {
    /// A mouse button changed its pressed state
    Button {
        /// Mouse button identity reported by the host system
        button: winit::event::MouseButton,
        /// Whether the mouse button is pressed
        pressed: bool,
    },
    /// The cursor moved to a physical window position
    CursorMoved {
        /// Horizontal cursor position in physical pixels
        x: f64,
        /// Vertical cursor position in physical pixels
        y: f64,
    },
    /// The cursor entered the host window
    CursorEntered,
    /// The cursor left the host window
    CursorLeft,
    /// The mouse wheel moved by line units
    WheelLines {
        /// Horizontal wheel movement in lines
        horizontal: f32,
        /// Vertical wheel movement in lines
        vertical: f32,
    },
    /// The mouse wheel moved by pixel units
    WheelPixels {
        /// Horizontal wheel movement in physical pixels
        horizontal: f64,
        /// Vertical wheel movement in physical pixels
        vertical: f64,
    },
}
