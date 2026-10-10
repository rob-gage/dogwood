// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Persistence of an actor when its chunk unloads
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorPersistenceComponent;
