// Copyright Rob Gage 2026

use crate::scenes::ScenePosition;

/// The actor-position snapshot used to interpolate rendering between fixed ticks
#[derive(Clone, Copy, bevy_ecs::component::Component)]
pub(crate) struct ActorPreviousPosition(pub(crate) ScenePosition);
