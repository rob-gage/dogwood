// Copyright Rob Gage 2026

use super::Actor;
use crate::scenes::ScenePosition;

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

}