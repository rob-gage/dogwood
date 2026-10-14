// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Movement of a kinematic pawn driven by its control intent
#[derive(Component, Copy, Clone, Debug, PartialEq)]
pub struct ActorPawnMovementComponent {
    /// Maximum horizontal speed in tiles per second
    pub speed: f32,
    /// Maximum horizontal speed while crouching in tiles per second
    pub crouching_speed: f32,
    /// Horizontal acceleration and deceleration in tiles per second squared
    pub acceleration: f32,
    /// Upward speed at the start of a jump in tiles per second
    pub jump_speed: f32,
}

impl Default for ActorPawnMovementComponent {

    /// Creates platformer movement that jumps about three tiles under default gravity
    fn default() -> Self {
        Self { speed: 10.0, crouching_speed: 4.0, acceleration: 80.0, jump_speed: 18.0 }
    }

}
