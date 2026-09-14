// Copyright Rob Gage 2026

/// Configures a pawn's swimming capability
#[derive(Copy, Clone)]
pub struct ActorPawnSwimmingConfiguration {
    /// Maximum gravity-relative swimming speed through fluid, in tiles per second
    pub maximum_speed: f32,
    /// Swimming thrust acceleration in tiles per second squared
    pub acceleration: f32,
    /// Physical density used for buoyancy
    pub density: f32,
    /// Scale applied to material viscosity when damping relative velocity
    pub drag: f32,
    /// Immersion at which walking automatically changes to swimming
    pub enter_immersion: f32,
    /// Immersion below which swimming automatically changes to walking
    pub exit_immersion: f32,
}
