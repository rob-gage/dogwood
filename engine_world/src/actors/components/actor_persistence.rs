// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Persistence of an actor when it exits the active simulation region, despawning it when absent
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorPersistenceComponent;
