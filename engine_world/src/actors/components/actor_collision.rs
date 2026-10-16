// Copyright Rob Gage 2026

use crate::scenes::collisions::{
    Collider,
    CollisionBody,
};
use bevy_ecs::component::Component;

/// Physical collision of an actor
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ActorCollisionComponent {
    /// Motion of the body carrying both colliders, kinematic for actors with only a contact
    pub body: CollisionBody,
    /// Collider physically colliding with other actors and cellular tiles
    pub collider: Collider,
    /// Bitmask of the groups this actor belongs to
    pub memberships: u32,
    /// Bitmask of the groups this actor collides with, colliding only when both actors accept;
    /// settled dynamic cellular collisions are reserved for cellular rigid bodies
    pub filter: u32,
    /// Mass of the body, in the units of dynamic cellular material masses, deciding how far it
    /// pushes dynamic cells and how much they slow it
    pub mass: f32,
}
