// Copyright Rob Gage 2026

/// One index-preserving snapshot of authoritative Rapier rigid-body state
pub(crate) struct RigidCellularBodyState {
    pub(crate) translation: [f32; 2],
    pub(crate) angle: f32,
    pub(crate) linear_velocity: [f32; 2],
    pub(crate) angular_velocity: f32,
    pub(crate) sleeping: bool,
    pub(crate) center_of_mass: [f32; 2],
    pub(crate) inverse_mass: f32,
    pub(crate) inverse_angular_inertia: f32,
}
