// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Shape of an actor for gameplay contacts
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorContactShapeComponent;
