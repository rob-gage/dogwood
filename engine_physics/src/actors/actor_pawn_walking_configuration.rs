// Copyright Rob Gage 2026

/// Configures a pawn's walking capability
#[derive(Copy, Clone)]
pub struct ActorPawnWalkingConfiguration {
    /// Maximum walking speed in tiles per second
    pub speed: f32,
    /// Walking acceleration in tiles per second squared
    pub acceleration: f32,
    /// Jump speed opposite gravity in tiles per second
    pub jump_velocity: f32,
    /// Axis-aligned collider width in tiles
    pub collider_width: f32,
    /// Axis-aligned collider height in tiles
    pub collider_height: f32,
}
