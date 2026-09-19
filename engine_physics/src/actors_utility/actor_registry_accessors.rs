// Copyright Rob Gage 2026

use super::actor_registry::ActorRegistry;
use crate::actors::Actor;
use crate::actors::ActorPawn;
use crate::actors::ActorSprites;
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

    /// Returns an actor's sprite and animation state.
    pub fn sprites(&self, identifier: Actor) -> Option<&ActorSprites> {
        self.bevy_entity(identifier)
            .and_then(|entity| self.world.get::<ActorSprites>(entity))
    }

    /// Returns mutable access to an actor's sprite and animation state.
    pub fn sprites_mutable(
        &mut self,
        identifier: Actor,
    ) -> Option<bevy_ecs::world::Mut<'_, ActorSprites>> {
        self.bevy_entity(identifier)
            .and_then(|entity| self.world.get_mut::<ActorSprites>(entity))
    }

    /// Attaches or replaces an actor's sprite and animation state.
    pub fn set_sprites(&mut self, identifier: Actor, sprites: ActorSprites) -> bool {
        let Some(entity) = self.bevy_entity(identifier) else {
            return false;
        };
        self.world.entity_mut(entity).insert(sprites);
        true
    }

    /// Removes an actor's sprite and animation state.
    pub fn remove_sprites(&mut self, identifier: Actor) -> Option<ActorSprites> {
        let entity: bevy_ecs::entity::Entity = self.bevy_entity(identifier)?;
        self.world.entity_mut(entity).take::<ActorSprites>()
    }
}
