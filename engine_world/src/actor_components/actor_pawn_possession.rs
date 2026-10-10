// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Possession of a pawn by a player
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorPawnPossessionComponent;
