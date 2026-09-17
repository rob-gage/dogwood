// Copyright Rob Gage 2026

/// A velocity measured in tiles per simulation second
#[derive(Clone, Copy, bevy_ecs::component::Component)]
pub struct SceneVelocity {
    /// The horizontal velocity
    pub x: f32,
    /// The vertical velocity
    pub y: f32,
}
