// Copyright Rob Gage 2026

use super::scene_error::SceneError;
use crate::chunks::{
    Chunk,
    ChunkCoordinates,
};

/// Completed background chunk request returned to the scene
pub enum SceneChunkStreamingResponse {
    /// Completed chunk load
    Loaded {
        /// Requested chunk coordinates
        coordinates: ChunkCoordinates,
        /// Identifier of the matching load request
        streaming_request_identifier: u64,
        /// Loaded chunk or typed load error
        result: Result<Chunk, SceneError>,
    },
    /// Completed save that returns ownership of its chunk
    Saved {
        /// Saved chunk coordinates
        coordinates: ChunkCoordinates,
        /// Identifier of the matching save request
        streaming_request_identifier: u64,
        /// Chunk retained if it must be restored after the save
        chunk: Chunk,
        /// Save result or typed save error
        result: Result<(), SceneError>,
    },
}
