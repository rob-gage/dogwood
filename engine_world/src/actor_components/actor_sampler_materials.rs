// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Samples the cell materials around an actor
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorSamplerMaterialsComponent;
