// Copyright Rob Gage 2026

use crate::scenes::SceneCollider;
use bevy_ecs::component::Component;

/// Gameplay contact of an actor
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ActorContactComponent {
    /// Collider reporting contacts
    pub collider: SceneCollider,
    /// Bitmask of the groups this actor belongs to
    pub memberships: u32,
    /// Bitmask of the groups this actor contacts, contacting only when both actors accept
    pub filter: u32,
}
