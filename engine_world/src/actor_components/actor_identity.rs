// Copyright Rob Gage 2026

use crate::actors::{
    Actor,
    ActorAllocator,
};
use bevy_ecs::{
    component::Component,
    lifecycle::HookContext,
    world::DeferredWorld,
};

/// Unique identity component making an entity the live form of one actor
#[derive(Component, Debug, Eq, PartialEq, Hash)]
#[component(immutable, on_insert, on_discard, clone_behavior = Ignore)]
pub(crate) struct ActorIdentityComponent {
    /// Actor this entity is the live form of
    actor: Actor,
}

impl ActorIdentityComponent {

    /// Creates the only identity component of a newly allocated actor
    pub(crate) const fn new(actor: Actor) -> Self {
        Self { actor }
    }

    /// Returns the actor this identity belongs to
    pub(crate) const fn actor(&self) -> Actor {
        self.actor
    }

    /// Maps this actor to the entity it was inserted into
    fn on_insert(mut world: DeferredWorld, context: HookContext) {
        let Some(actor_identity) = world.get::<ActorIdentityComponent>(context.entity) else { return };
        let actor: Actor = actor_identity.actor;
        if let Some(mut actor_allocator) = world.get_resource_mut::<ActorAllocator>() {
            actor_allocator.entities.insert(actor, context.entity);
        }
    }

    /// Unmaps this actor when its identity is removed, replaced, or despawned
    fn on_discard(mut world: DeferredWorld, context: HookContext) {
        let Some(actor_identity) = world.get::<ActorIdentityComponent>(context.entity) else { return };
        let actor: Actor = actor_identity.actor;
        if let Some(mut actor_allocator) = world.get_resource_mut::<ActorAllocator>() {
            actor_allocator.entities.remove(&actor);
        }
    }

}
