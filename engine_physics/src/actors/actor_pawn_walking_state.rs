// Copyright Rob Gage 2026

/// Runtime state for a walking pawn
#[derive(bevy_ecs::component::Component, Default)]
pub struct ActorPawnWalkingState {
    /// Whether terrain currently supports the pawn in the gravity direction
    pub grounded: bool,
    /// Tick-integrated intended locomotion impulse for transient cellular interaction
    pub cellular_drive_impulse: [f32; 2],
}
