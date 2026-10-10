// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Contact groups deciding which actors report gameplay contacts
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorContactGroupsComponent;
