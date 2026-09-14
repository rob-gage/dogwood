// Copyright Rob Gage 2026

/// Latest asynchronously sampled fluid state surrounding a swimming-capable pawn
#[derive(bevy_ecs::component::Component, Default)]
pub struct ActorPawnSwimmingState {
    /// Coverage-weighted fraction of the pawn capsule occupied by fluid
    pub immersion: f32,
    /// Coverage-weighted surrounding fluid velocity in tiles per second
    pub fluid_velocity: [f32; 2],
    /// Coverage-weighted physical fluid density
    pub fluid_density: f32,
    /// Coverage-weighted physical fluid viscosity
    pub fluid_viscosity: f32,
}
