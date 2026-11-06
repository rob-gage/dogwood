// Copyright Rob Gage 2026

/// What the cell grid held around a particle at its newest completed sample, some simulation steps
/// old
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct SoftBodyEnvironment {
    /// Hottest temperature in kelvin of the particle's cell and its four neighbors
    pub temperature: f32,
    /// Pressure in the particle's cell above the ambient pressure in atmospheres
    pub pressure: f32,
    /// Change of pressure per cell along each axis
    pub pressure_gradient: [f32; 2],
    /// Gas flow through the particle's cell along each axis, in tiles per second
    pub gas_velocity: [f32; 2],
    /// Share of the particle's cell filled by fluid
    pub fluid_coverage: f32,
    /// Velocity of the fluid in the particle's cell, in tiles per second
    pub fluid_velocity: [f32; 2],
    /// Bit `i` is set when configured tag `i` was on any material in the cell or a neighbor
    pub tags: u32,
    /// Whether this record has ever been sampled
    pub is_sampled: bool,
}
