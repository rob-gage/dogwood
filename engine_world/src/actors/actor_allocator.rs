// Copyright Rob Gage 2026

use super::{
    actor::Actor,
    actor_error::ActorError,
};
use super::components::ActorIdentityComponent;
use bevy_ecs::{
    entity::Entity,
    resource::Resource,
};
use std::{
    collections::HashMap,
    sync::atomic::{
        AtomicU64,
        Ordering,
    },
};

/// Actor identifier allocation and the live entity of every loaded actor
#[derive(Resource, Debug)]
pub struct ActorAllocator {
    /// Live entity of every loaded actor, maintained by the `ActorIdentityComponent` hooks
    pub entities: HashMap<Actor, Entity>,
    /// Next identifier to allocate, never reused within a scene
    next_actor_identifier: AtomicU64,
}

impl ActorAllocator {

    /// Creates an empty allocator whose first allocated identifier is zero
    pub fn new() -> Self {
        Self { entities: HashMap::new(), next_actor_identifier: AtomicU64::new(0) }
    }

    /// Allocates a new actor whose identity component makes it live once inserted into an entity
    pub fn allocate_actor(&self) -> Result<ActorIdentityComponent, ActorError> {
        let identifier: u64 = self.next_actor_identifier
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |identifier: u64| identifier.checked_add(1))
            .map_err(|_| ActorError::ActorIdentifiersExhausted)?;
        Ok(ActorIdentityComponent::new(Actor::new(identifier)))
    }

    /// Returns the live entity of an actor
    pub fn entity(&self, actor: Actor) -> Result<Entity, ActorError> {
        self.entities.get(&actor).copied().ok_or(ActorError::ActorNotLoaded { actor })
    }

}
