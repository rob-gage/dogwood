// Copyright Rob Gage 2026

/// Window lifecycle event produced by the host system
pub enum SystemWindowEvent {
    /// The host window received a close request
    CloseRequested,
    /// The host window changed its physical size
    Resized {
        /// New window width in physical pixels
        width: u32,
        /// New window height in physical pixels
        height: u32,
    },
    /// The host window acquired keyboard focus
    FocusAcquired,
    /// The host window released keyboard focus
    FocusReleased,
}
