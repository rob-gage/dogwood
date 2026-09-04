// Copyright Rob Gage 2026

use super::{
    Actor,
    ActorControlState,
    ActorPawn,
    ActorPossessable,
};
use crate::scenes::{
    ScenePosition,
    SceneVelocity,
};

/// Owns the ECS world and provides the engine's actor-facing API
pub struct ActorRegistry {
    world: bevy_ecs::world::World,
}

impl ActorRegistry {

    /// Creates an empty `ActorRegistry`
    pub fn new() -> Self { Self { world: bevy_ecs::world::World::new() } }

    /// Creates an actor with a `ScenePosition`
    pub fn spawn(&mut self, position: ScenePosition) -> Actor {
        Actor::from_bevy_entity(self.world.spawn(position).id())
    }

    /// Creates a pawn with a position and velocity
    pub fn spawn_pawn(
        &mut self,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Actor::from_bevy_entity(
            self.world.spawn((
                ActorPawn,
                ActorControlState::default(),
                position,
                velocity,
            )).id()
        )
    }

    /// Creates a pawn that is eligible for possession
    pub fn spawn_possessable_pawn(
        &mut self,
        position: ScenePosition,
        velocity: SceneVelocity,
    ) -> Actor {
        Actor::from_bevy_entity(
            self.world.spawn((
                ActorPawn,
                ActorPossessable,
                ActorControlState::default(),
                position,
                velocity,
            )).id()
        )
    }

    /// Removes an actor from the registry, returning true if successful
    pub fn despawn(&mut self, identifier: Actor) -> bool {
        self.world.despawn(identifier.bevy_entity())
    }

    /// Returns `true` if this `ActorRegistry` contains this `Actor`
    pub fn contains(&self, identifier: Actor) -> bool {
        self.world.get_entity(identifier.bevy_entity()).is_ok()
    }

    /// Returns an actor's position
    pub fn get_position(&self, identifier: Actor) -> Option<&ScenePosition> {
        self.world.get::<ScenePosition>(identifier.bevy_entity())
    }

    /// Returns an actor's velocity
    pub fn get_velocity(&self, identifier: Actor) -> Option<&SceneVelocity> {
        self.world.get::<SceneVelocity>(identifier.bevy_entity())
    }

    /// Sets an actor's position
    pub fn set_position(
        &mut self,
        identifier: Actor,
        position: ScenePosition,
    ) -> bool {
        self.world.get_mut::<ScenePosition>(identifier.bevy_entity())
            .map(|mut current| *current = position)
            .is_some()
    }

    /// Sets an actor's velocity
    pub fn set_velocity(
        &mut self,
        identifier: Actor,
        velocity: SceneVelocity,
    ) -> bool {
        self.world.get_mut::<SceneVelocity>(identifier.bevy_entity())
            .map(|mut current| *current = velocity)
            .is_some()
    }

    /// Passes universal controls to a pawn
    pub fn set_control_state(
        &mut self,
        identifier: Actor,
        control_state: ActorControlState,
    ) -> bool {
        if self.world.get::<ActorPawn>(identifier.bevy_entity()).is_none() {
            return false;
        }
        self.world.entity_mut(identifier.bevy_entity())
            .insert(control_state);
        true
    }

    /// Returns whether an actor is eligible for possession
    pub fn is_possessable(&self, identifier: Actor) -> bool {
        self.world.get::<ActorPossessable>(identifier.bevy_entity()).is_some()
    }

}
