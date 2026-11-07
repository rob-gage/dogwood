// Copyright Rob Gage 2026

mod soft_bodies;
mod soft_body;
mod soft_body_descriptor;
mod soft_body_environment;
mod soft_body_sampling_pass;
mod soft_body_terrain;

pub(crate) use soft_bodies::{SoftBodies, SoftBodyRapier, SoftBodyTearEvents};
pub use soft_bodies::{SceneSoftBody, SoftBodyEvent, SoftBodyParticles};
pub(crate) use soft_body::{SoftBody, SoftBodySegment, SoftBodySegmentTile};
pub use soft_body_descriptor::{
    SoftBodyAnchor,
    SoftBodyDescriptor,
    SoftBodyParticleDescriptor,
    SoftBodyCell,
    SoftBodyElasticity,
    SoftBodySegmentDescriptor,
};
pub use soft_body_environment::SoftBodyEnvironment;
pub(crate) use soft_body_sampling_pass::SoftBodySamplingPass;
pub(crate) use soft_body_terrain::SoftBodyTerrain;
pub use soft_body_terrain::SceneTerrain;

/// Simulation steps a soft body must rest before it sleeps
const SOFT_BODY_REST_STEP_COUNT: u32 = 45;

/// Fastest particle speed in tiles per second at which a soft body counts as resting
const SOFT_BODY_REST_SPEED: f32 = 0.5;

/// Seconds of one simulation step
const SOFT_BODY_STEP_SECONDS: f32 = 1.0 / 60.0;

/// Tiles of clearance kept between a particle and the solid cell it stopped against
const SOFT_BODY_TERRAIN_MARGIN: f32 = 0.005;

/// Tiles a particle is pushed out of another body per step, which lets bodies part without a kick
const SOFT_BODY_SEPARATION_STEP: f32 = 0.06;

/// Steps during which the pieces of a cut are not pushed apart
const SOFT_BODY_CUT_GRACE: u32 = 30;

/// Largest number of particle samples one simulation step can read from the cell grid
pub const SOFT_BODY_SAMPLE_COUNT_MAXIMUM: usize = 2048;

/// Largest acceleration in tiles per second squared the cell grid can apply to a particle
const SOFT_BODY_ENVIRONMENT_ACCELERATION_MAXIMUM: f32 = 900.0;

/// Atmospheres of pressure change per cell below which pressure does not push a particle
const SOFT_BODY_PRESSURE_GRADIENT_MINIMUM: f32 = 0.05;

/// Tiles per second of gas flow below which gas does not push a particle, so that the slow stirring
/// of still air lets bodies come to rest
const SOFT_BODY_GAS_SPEED_MINIMUM: f32 = 1.5;

/// Collision group of soft body colliders, reserved by the engine
pub const COLLISION_GROUP_SOFT_BODY: u32 = 1 << 26;

/// Collision group of soft bodies that stop actors, reserved by the engine
pub const COLLISION_GROUP_SOFT_BODY_SOLID: u32 = 1 << 25;
