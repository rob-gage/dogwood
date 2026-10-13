// Copyright Rob Gage 2026

use crate::scenes::{
    SceneCollider,
    SceneCollisionBody,
};
use bevy_ecs::component::Component;

/// Physical collision of an actor
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ActorCollisionComponent {
    /// Motion of the body carrying both colliders, kinematic for actors with only a contact
    pub body: SceneCollisionBody,
    /// Collider physically colliding with other actors and cellular tiles
    pub collider: SceneCollider,
    /// Bitmask of the groups this actor belongs to
    pub memberships: u32,
    /// Bitmask of the groups this actor collides with, colliding only when both actors accept
    pub filter: u32,
}
