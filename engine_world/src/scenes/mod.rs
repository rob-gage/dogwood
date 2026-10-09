// Copyright Rob Gage 2026

mod scene;
mod scene_chunk;
mod scene_chunk_region;
mod scene_chunk_streaming_request;
mod scene_chunk_streaming_response;
mod scene_chunk_streaming_state;
mod scene_error;

pub use scene::Scene;
pub use scene_chunk_region::SceneChunkRegion;
pub use scene_chunk_streaming_state::SceneChunkStreamingState;
pub use scene_error::SceneError;

/// Maximum number of persistent retained regions
const SCENE_CHUNK_REGION_COUNT_MAXIMUM: usize = 32;

/// Maximum side length of one requested chunk rectangle
const SCENE_CHUNK_REGION_SIZE_MAXIMUM: u8 = 32;

/// Maximum union of retained and dirty chunks owned by one scene
const SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM: usize = 256;

/// Maximum attempts to save one dirty chunk before dropping a scene panics
const SCENE_CHUNK_SAVE_ATTEMPT_COUNT_MAXIMUM: usize = 3;