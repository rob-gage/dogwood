// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Rotation of an actor, letting its dynamic collision body turn
#[derive(Component, Copy, Clone, Debug, Default, PartialEq)]
pub struct ActorKinematicsRotationComponent {
    /// Counterclockwise rotation around the actor's position in radians, written back by
    /// collision for dynamic bodies and applied to the body when gameplay changes it
    pub rotation: f32,
}
