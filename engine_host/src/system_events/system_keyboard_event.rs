// Copyright Rob Gage 2026

/// Keyboard input event produced by the host system
pub struct SystemKeyboardEvent {
    /// Physical key identity reported by the host system
    pub physical_key: winit::keyboard::PhysicalKey,
    /// Whether the physical key is pressed
    pub pressed: bool,
    /// Whether the press is an operating system key repeat
    pub repeat: bool,
}
