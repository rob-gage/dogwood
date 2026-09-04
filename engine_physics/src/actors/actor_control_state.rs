// Copyright Rob Gage 2026

use engine_input::ControlState;

/// The current `ControlState` assigned to an actor pawn
#[derive(Copy, Clone, Default, bevy_ecs::component::Component)]
pub struct ActorControlState(pub ControlState);
