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
    /// Maximum angle between a walkable surface normal and gravity-relative up, in radians
    pub maximum_slope_angle: f32,
    /// Walking collider diameter across gravity-relative tangent, in tiles
    pub collider_width: f32,
    /// Walking collider extent along gravity-relative up, in tiles
    pub collider_height: f32,
}
