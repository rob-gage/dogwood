// Copyright Rob Gage 2026

use super::Scene;
use super::ScenePosition;
use crate::actors::Actor;
use crate::actors::ActorRegistry;

impl Scene {
    /// Returns the `ActorRegistry` for this `Scene`
    pub const fn actor_registry(&self) -> &ActorRegistry {
        &self.actor_registry
    }

    /// Returns mutable access to the `ActorRegistry` for this `Scene`
    pub const fn actor_registry_mutable(&mut self) -> &mut ActorRegistry {
        &mut self.actor_registry
    }

    /// Returns the currently possessed actor if one exists
    pub fn possessed_actor(&self) -> Option<Actor> {
        self.possessed_actor
            .filter(|actor| self.actor_registry.contains(*actor))
    }

    /// Returns an actor's position interpolated between its latest fixed ticks
    pub fn actor_render_position(&self, actor: Actor) -> Option<ScenePosition> {
        self.actor_registry
            .get_render_position(actor, self.tick_interpolation())
    }

    /// Possesses an actor if it exists in this `Scene`
    pub fn possess_actor(&mut self, identifier: Actor) -> bool {
        if !self.actor_registry.is_possessable(identifier) {
            return false;
        }
        if self.possessed_actor != Some(identifier)
            && let Some(possessed) = self.possessed_actor
        {
            self.actor_registry.clear_control_state(possessed);
        }
        self.possessed_actor = Some(identifier);
        true
    }

    /// Releases the currently possessed actor
    pub fn dispossess_actor(&mut self) {
        if let Some(possessed) = self.possessed_actor.take() {
            self.actor_registry.clear_control_state(possessed);
        }
    }
}
