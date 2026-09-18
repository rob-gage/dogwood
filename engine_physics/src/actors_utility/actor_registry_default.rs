// Copyright Rob Gage 2026

use super::actor_registry::ActorRegistry;

impl Default for ActorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
