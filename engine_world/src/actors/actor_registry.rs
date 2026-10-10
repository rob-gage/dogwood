// Copyright Rob Gage 2026

use super::{
    actor::Actor,
    actor_allocator::ActorAllocator,
    actor_error::ActorError,
};
use crate::actor_components::ActorIdentityComponent;
use bevy_ecs::{
    bundle::Bundle,
    change_detection::Mut,
    component::{
        Component,
        Mutable,
    },
    entity::Entity,
    world::World,
};

/// Scene-owned ECS storage of every live actor and its components
pub struct ActorRegistry {
    /// ECS world storing actor entities, their components, and the actor allocator
    world: World,
}

impl ActorRegistry {

    /// Creates an empty registry with no actors
    pub fn new() -> Self {
        let mut world: World = World::new();
        world.insert_resource(ActorAllocator::new());
        Self { world }
    }

    /// Spawns a live actor with the supplied components and returns its new identity
    pub fn spawn(&mut self, components: impl Bundle) -> Result<Actor, ActorError> {
        let actor_identity: ActorIdentityComponent = self.world.resource::<ActorAllocator>().allocate_actor()?;
        let actor: Actor = actor_identity.actor();
        self.world.spawn((actor_identity, components));
        Ok(actor)
    }

    /// Permanently despawns a live actor
    pub fn despawn(&mut self, actor: Actor) -> Result<(), ActorError> {
        let entity: Entity = self.entity(actor)?;
        self.world.despawn(entity);
        Ok(())
    }

    /// Returns whether an actor currently has a live entity
    pub fn is_loaded(&self, actor: Actor) -> bool {
        self.entity(actor).is_ok()
    }

    /// Returns a component of a live actor
    pub fn component<T: Component>(&self, actor: Actor) -> Result<&T, ActorError> {
        let entity: Entity = self.entity(actor)?;
        self.world.get::<T>(entity).ok_or(ActorError::ActorComponentMissing { actor })
    }

    /// Returns a mutable component of a live actor
    pub fn component_mutable<T: Component<Mutability = Mutable>>(
        &mut self,
        actor: Actor,
    ) -> Result<&mut T, ActorError> {
        let entity: Entity = self.entity(actor)?;
        self.world
            .get_mut::<T>(entity)
            .map(|component: Mut<T>| component.into_inner())
            .ok_or(ActorError::ActorComponentMissing { actor })
    }

    /// Inserts components into a live actor, replacing any it already has
    pub fn insert(&mut self, actor: Actor, components: impl Bundle) -> Result<(), ActorError> {
        let entity: Entity = self.entity(actor)?;
        self.world.entity_mut(entity).insert(components);
        Ok(())
    }

    /// Removes a component from a live actor
    pub fn remove<T: Component>(&mut self, actor: Actor) -> Result<T, ActorError> {
        let entity: Entity = self.entity(actor)?;
        self.world.entity_mut(entity).take::<T>().ok_or(ActorError::ActorComponentMissing { actor })
    }

    /// Returns the live entity of an actor
    fn entity(&self, actor: Actor) -> Result<Entity, ActorError> {
        self.world.resource::<ActorAllocator>().entity(actor)
    }

}
