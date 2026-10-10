// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Sprites drawn for an actor
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorSpritesComponent;
