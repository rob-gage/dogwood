// Copyright Rob Gage 2026

use super::{
    Chunk,
    ChunkCoordinates,
};
use crate::scenes::SceneGenerator;
use std::sync::Arc;

/// One chunk load or save submitted to the shared Rayon pool
pub enum ChunkStreamingRequest {
    /// Load one chunk from persistent storage
    Load {
        /// Chunk coordinates to load
        coordinates: ChunkCoordinates,
        /// Identifier used to reject stale streaming responses
        streaming_request_identifier: u64,
    },
    /// Generate one chunk that has no persisted file
    Generate {
        /// Chunk coordinates to generate
        coordinates: ChunkCoordinates,
        /// Identifier used to reject stale streaming responses
        streaming_request_identifier: u64,
        /// Generator producing the chunk
        generator: Arc<dyn SceneGenerator>,
        /// Stable seed supplied to the generator
        world_seed: u64,
    },
    /// Save one dirty chunk to persistent storage
    Save {
        /// Identifier used to reject stale streaming responses
        streaming_request_identifier: u64,
        /// Dirty chunk transferred to the save task
        chunk: Chunk,
    },
}
