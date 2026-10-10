// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Control intent of a pawn, written by player input or AI
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorPawnControlComponent;
