// Copyright Rob Gage 2026

use bevy_ecs::entity::Entity;

/// Identifies an actor managed by an `ActorRegistry`
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Actor(Entity);

impl Actor {
    /// Creates an `ActorIdentifier` from an internal `bevy_ecs::entity::Entity`
    pub(super) const fn from_bevy_entity(entity: Entity) -> Self {
        Self(entity)
    }

    /// Returns the internal `bevy_ecs::entity::Entity` represented by this identifier
    pub(super) const fn bevy_entity(self) -> Entity {
        self.0
    }
}
