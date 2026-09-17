// Copyright Rob Gage 2026

use super::{
    ActorPawnFlyingConfiguration, ActorPawnMovement, ActorPawnNoclipConfiguration,
    ActorPawnSwimmingConfiguration, ActorPawnWalkingConfiguration,
};
use crate::actors_utility::ActorCollisionShape;

/// Configures the movement capabilities and active movement of an actor pawn
#[derive(bevy_ecs::component::Component)]
pub struct ActorPawn {
    /// Shared physical geometry used by movement and cellular interaction
    pub collision_shape: Option<ActorCollisionShape>,
    /// Walking capability configuration, if supported
    pub walking: Option<ActorPawnWalkingConfiguration>,
    /// Flying capability configuration, if supported
    pub flying: Option<ActorPawnFlyingConfiguration>,
    /// Swimming capability configuration, if supported
    pub swimming: Option<ActorPawnSwimmingConfiguration>,
    /// Noclip capability configuration, if supported
    pub noclip: Option<ActorPawnNoclipConfiguration>,
    /// The movement behavior currently used by this pawn
    pub movement: Option<ActorPawnMovement>,
    /// Whether this pawn continues movement simulation while gameplay is paused
    pub simulate_when_paused: bool,
}

impl Default for ActorPawn {
    fn default() -> Self {
        Self::new()
    }
}

impl ActorPawn {
    /// Creates a pawn with no movement capabilities or active movement
    pub const fn new() -> Self {
        Self {
            collision_shape: None,
            walking: None,
            flying: None,
            swimming: None,
            noclip: None,
            movement: None,
            simulate_when_paused: false,
        }
    }
}
