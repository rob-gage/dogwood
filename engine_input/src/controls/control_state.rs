// Copyright Rob Gage 2026

/// The current universal controls produced by an input or control scheme
#[derive(Copy, Clone, Default)]
pub struct ControlState {
    /// Normalized horizontal locomotion intent
    pub locomotion_x: f32,
    /// Normalized vertical locomotion intent
    pub locomotion_y: f32,
}
