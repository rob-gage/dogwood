// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Possession of a pawn by the local player, whose keyboard writes its control
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorPawnPossessionComponent;
