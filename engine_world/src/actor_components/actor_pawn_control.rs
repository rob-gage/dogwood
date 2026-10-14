// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Control intent of a pawn, written by player input or AI
#[derive(Component, Copy, Clone, Debug, Default, PartialEq)]
pub struct ActorPawnControlComponent {
    /// Horizontal movement intent from `-1.0` for left to `1.0` for right
    pub movement: f32,
    /// Whether the pawn wants to jump, jumping once per press while grounded
    pub is_jumping: bool,
    /// Whether the pawn wants to crouch, moving at its crouching speed
    pub is_crouching: bool,
}
