// Copyright Rob Gage 2026

/// How the native window occupies the screen
#[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum WindowMode {
    /// An ordinary decorated window
    #[default]
    Windowed,
    /// A borderless window covering the whole monitor at its current resolution, which switches
    /// instantly and keeps other windows and overlays working
    Fullscreen,
    /// Exclusive fullscreen at the monitor's current resolution and highest refresh rate; where
    /// the platform has no exclusive mode, or lists no video mode for the monitor, it is
    /// borderless fullscreen
    ExclusiveFullscreen,
}
