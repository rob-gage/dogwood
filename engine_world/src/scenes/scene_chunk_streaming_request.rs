// Copyright Rob Gage 2026

use crate::chunks::{
    Chunk,
    ChunkCoordinates,
};

/// One chunk load or save submitted to the shared Rayon pool
pub enum SceneChunkStreamingRequest {
    /// Load one chunk from persistent storage
    Load {
        /// Chunk coordinates to load
        coordinates: ChunkCoordinates,
        /// Identifier used to reject stale streaming responses
        streaming_request_identifier: u64,
    },
    /// Save one dirty chunk to persistent storage
    Save {
        /// Identifier used to reject stale streaming responses
        streaming_request_identifier: u64,
        /// Dirty chunk transferred to the save task
        chunk: Chunk,
    },
}
