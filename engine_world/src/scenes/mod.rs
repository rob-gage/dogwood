// Copyright Rob Gage 2026

pub mod cellular_tiles;
pub mod chemistry;
pub mod chunks;
pub mod collisions;
pub mod edits;
pub(crate) mod actor_sensing;
pub mod fluids;
pub mod pressure;
pub mod thermal;
mod scene;
mod scene_error;
mod scene_backgrounds;
mod scene_generator;
mod scene_material_surfaces;
mod scene_position;
pub mod soft_bodies;

pub use scene::{
    Scene,
    ScenePawnTether,
    ScenePawnTetherBody,
    SceneRayHit,
    SceneRayTarget,
    SceneRigidBody,
};
pub use scene_error::SceneError;
pub use scene_generator::SceneGenerator;
pub use scene_material_surfaces::{MaterialSurface, MATERIAL_SURFACE_SIZE};
pub use scene_position::ScenePosition;
pub use actor_sensing::{
    SceneActorSensing,
    SceneActorSensingConfiguration,
    SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM,
};

/// Maximum number of persistent retained regions
const SCENE_CHUNK_REGION_COUNT_MAXIMUM: usize = 32;

/// Maximum side length of one requested chunk rectangle
const SCENE_CHUNK_REGION_SIZE_MAXIMUM: u8 = 32;

/// Maximum union of retained and dirty chunks owned by one scene
const SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM: usize = 256;

/// Maximum attempts to save one dirty chunk before dropping a scene panics
const SCENE_CHUNK_SAVE_ATTEMPT_COUNT_MAXIMUM: usize = 3;

/// Maximum cellular tile streaming requests awaiting application to chunks
const SCENE_CELLULAR_TILE_STREAMING_REQUEST_COUNT_MAXIMUM: usize = 4;

/// Dormant fluid particles moved from chunks into the live pool per scene tick, in whole tiles
/// unless a single tile holds more
const SCENE_FLUID_IMPORT_COUNT_MAXIMUM_PER_TICK: usize = 4096;

/// Downward acceleration of dynamic bodies and pawns in tiles per second squared
const SCENE_GRAVITY_DEFAULT: f32 = 50.0;

/// Default pressure of the phantom air in empty cells in atmospheres
const SCENE_AMBIENT_GAS_PRESSURE_DEFAULT: f32 = 1.0;

/// Default mass of one atmosphere of the phantom air in empty cells, compared with gas masses
const SCENE_AMBIENT_GAS_MASS_DEFAULT: f32 = 29.0;

/// Default temperature in kelvin of the phantom air in empty cells
const SCENE_AMBIENT_GAS_TEMPERATURE_DEFAULT: f32 = 293.0;

/// Default heat per atmosphere of the phantom air in one cell needed to warm it by one kelvin
const SCENE_AMBIENT_GAS_HEAT_CAPACITY_DEFAULT: f32 = 0.01;

/// Default heat per second per kelvin passing through each face between cells of phantom air, and
/// between phantom air and the surroundings it relaxes toward
const SCENE_AMBIENT_GAS_CONDUCTION_DEFAULT: f32 = 0.05;

/// Default fraction of active-region cells available to fluid particles
const SCENE_FLUID_CAPACITY_FACTOR_DEFAULT: f32 = 0.25;

/// Name of the processor thread pool running chunk loads, generation, and saves
const SCENE_CHUNK_THREAD_POOL_NAME: &str = "chunk_io";

mod acoustics;
pub use acoustics::AcousticBucket;
