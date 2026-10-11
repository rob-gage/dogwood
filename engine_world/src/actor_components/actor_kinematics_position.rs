// Copyright Rob Gage 2026

use crate::scenes::ScenePosition;
use bevy_ecs::component::Component;

/// Position of an actor
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorKinematicsPositionComponent {
    /// Exact position of the actor in its scene
    pub position: ScenePosition,
}
