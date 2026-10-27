// Copyright Rob Gage 2026

use crate::scenes::chunks::{
    ChunkCoordinates,
    ChunkFluidParticle,
};

mod fluid_particle_pass;
mod fluids;

pub(crate) use fluids::decode_exported_particle_records;
pub use fluids::Fluids;

/// Fluid feedback sampled for one actor by the latest completed GPU submission
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct FluidActorContact {
    /// Impulse on the actor, in mass cells per tick
    pub(crate) impulse: [f32; 2],
    /// Fraction of covered actor cells occupied by fluid, from zero to one
    pub(crate) immersion: f32,
    /// Mean fluid velocity over covered cells, in cells per tick
    pub(crate) fluid_velocity: [f32; 2],
    /// Density of the dominant sampled fluid material, in mass per cell area
    pub(crate) fluid_density: f32,
    /// Physical drag coefficient of the dominant sampled fluid material
    pub(crate) fluid_drag: f32,
}

/// One dormant chunk particle selected for an exact-position GPU import
#[derive(Copy, Clone)]
pub(crate) struct FluidParticleImport {
    /// Chunk that owns the record if the GPU rejects the import
    pub(crate) chunk_coordinates: ChunkCoordinates,
    /// Exact persisted particle state, material, color, temperature, and amount
    pub(crate) particle: ChunkFluidParticle,
    /// Position relative to the active region origin, in cells
    pub(crate) position: [f32; 2],
}

/// Fixed-point units per actor impulse and fluid sample value
pub(crate) const FLUID_ACTOR_CONTACT_SCALE: f32 = 1024.0;

/// Number of fluid solver substeps performed during one fixed scene tick
pub const FLUID_SUBSTEP_COUNT: usize = 4;

/// Number of Jacobi pressure and position correction iterations performed per substep
pub const FLUID_ITERATION_COUNT: usize = 2;

/// Fluid density and pressure kernel support radius in cells
pub const FLUID_KERNEL_RADIUS: f32 = 2.5;

/// Width and height of each dense particle-neighbor bin in cells
pub const FLUID_BIN_SIZE: f32 = 3.0;

/// Small denominator added to the PBF constraint gradient norm for numerical stability
pub const FLUID_RELAXATION: f32 = 0.01;

/// Position-correction scale for material cohesion, in cells cubed per strength unit
pub const FLUID_COHESION_SCALE: f32 = 0.5;

/// Area-normalization coefficient of the dimensionless two-dimensional Akinci spline
pub const FLUID_COHESION_NORMALIZATION_2D: f32 = 54.584_815;

/// Maximum absolute particle velocity along either cell axis, measured per fixed tick
pub const FLUID_VELOCITY_MAXIMUM: f32 = 4.0;

/// Fixed-point units per fluid mass and momentum unit in the cell raster
pub const FLUID_RASTER_SCALE: f32 = 4096.0;

/// Number of particle invocations in each GPU workgroup
pub const FLUID_WORKGROUP_SIZE: u32 = 128;

/// Number of bins scanned by one workgroup in the subgroup-free prefix scan
pub const FLUID_SCAN_WORKGROUP_SIZE: u32 = 256;

/// Fixed duration of one scene tick in seconds
pub const FLUID_TIME_STEP: f32 = 1.0 / 60.0;
