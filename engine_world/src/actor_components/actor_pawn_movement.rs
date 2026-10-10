// Copyright Rob Gage 2026

use bevy_ecs::component::Component;

/// Movement of a pawn driven by its control intent
#[derive(Component, Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ActorPawnMovementComponent;
