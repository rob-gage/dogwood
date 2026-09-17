// Copyright Rob Gage 2026

pub(crate) const READBACK_SLOT_COUNT: usize = 3;
pub(crate) const MAXIMUM_MOVEMENT_CELLS: u32 = 4;
pub(crate) const PRESSURE_DAMAGE_RATE: f32 = 10.0;
pub(crate) const RIGID_REACTION_READBACK_SLOT_COUNT: usize = 3;
pub(crate) const INITIAL_RIGID_BODY_CAPACITY: usize = 16;
pub(crate) const SUPPORT_RADIUS_CELLS: f32 = 2.5;
pub(crate) const PARTICLE_RADIUS_CELLS: f32 = 0.45;
pub(crate) const MAXIMUM_CORRECTION_CELLS: f32 = 0.25;
pub(crate) const FLUID_EDIT_ERASE: u32 = 1;
pub(crate) const PBF_SUBSTEP_COUNT: u32 = 2;
pub(crate) const PBF_CONSTRAINT_ITERATION_COUNT: u32 = 4;
pub(crate) const PRESSURE_ITERATION_COUNT: u32 = 12;
pub(crate) const VORTICITY_CONFINEMENT: f32 = 0.2;
pub(crate) const BUOYANCY_COEFFICIENT: f32 = 0.05;
pub(crate) const MAXIMUM_SPEED_CELLS_PER_SECOND: f32 = 8.0;
pub(crate) const FLUID_OBSTACLE_COVERAGE: f32 = 0.85;
pub(crate) const AMBIENT_DENSITY: f32 = 1.0;
pub(crate) const AUTHORED_CONCENTRATION: f32 = 1.0;
pub(crate) const RIGID_REMOVAL_EVENT_SIZE: u64 = 32;
pub(crate) const RIGID_REMOVAL_EVENTS_OFFSET: u64 = 256;
pub(crate) const TERRAIN_COLLISION_PATCH_TILES: i32 = 4;
pub(crate) const TERRAIN_COLLISION_PATCH_CELLS: i32 = TERRAIN_COLLISION_PATCH_TILES * 8;
pub(crate) const TERRAIN_PATCH_RETENTION_TICKS: u64 = 120;
pub(crate) const DYNAMIC_TILE_RETENTION_TICKS: u64 = 30;
pub(crate) const RIGID_PHASE_CANDIDATE_SIZE: u64 = 40;
pub(crate) const RIGID_PHASE_CANDIDATES_OFFSET: u64 = 256;
