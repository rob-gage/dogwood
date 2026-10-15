// Copyright Rob Gage 2026

pub mod cellular_tiles;
pub mod chunks;
pub mod collisions;
pub mod edits;
pub mod kinematics;
mod scene;
mod scene_error;
mod scene_generator;
mod scene_position;

pub use scene::Scene;
pub use scene_error::SceneError;
pub use scene_generator::SceneGenerator;
pub use scene_position::ScenePosition;

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

/// Downward acceleration of dynamic bodies and pawns in tiles per second squared
const SCENE_GRAVITY_DEFAULT: f32 = 50.0;

/// Name of the processor thread pool running chunk loads, generation, and saves
const SCENE_CHUNK_THREAD_POOL_NAME: &str = "chunk_io";