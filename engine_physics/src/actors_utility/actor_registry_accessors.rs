// Copyright Rob Gage 2026

use super::actor_registry::ActorRegistry;
use crate::actors::Actor;
use crate::actors::ActorPawn;
use crate::scenes::ScenePosition;
use crate::scenes::SceneVelocity;

impl ActorRegistry {
    /// Returns `true` if this `ActorRegistry` contains this `Actor`
    pub fn contains(&self, identifier: Actor) -> bool {
        self.entities
            .get(&identifier)
            .is_some_and(|entity| self.world.get_entity(*entity).is_ok())
    }

    /// Returns an actor's position
    pub fn get_position(&self, identifier: Actor) -> Option<&ScenePosition> {
        self.bevy_entity(identifier)
            .and_then(|entity| self.world.get::<ScenePosition>(entity))
    }

    /// Returns an actor's velocity
    pub fn get_velocity(&self, identifier: Actor) -> Option<&SceneVelocity> {
        self.bevy_entity(identifier)
            .and_then(|entity| self.world.get::<SceneVelocity>(entity))
    }

    /// Returns an actor's pawn configuration
    pub fn get_pawn(&self, identifier: Actor) -> Option<&ActorPawn> {
        self.bevy_entity(identifier)
            .and_then(|entity| self.world.get::<ActorPawn>(entity))
    }
}
