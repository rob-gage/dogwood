// Copyright Rob Gage 2026

use super::Chunk;
use crate::tiles::TileCoordinates;
use std::error::Error;

/// The result returned by a background chunk load or generation task
pub enum ChunkStreamingResponse {
    /// The result of attempting to load a chunk from persistent scene data
    Loaded {
        /// The identifier of the streaming operation
        streaming_identifier: u64,
        /// The coordinates of the chunk
        coordinates: TileCoordinates,
        /// The loaded chunk, or `None` when no saved chunk exists
        result: Result<Option<Box<Chunk>>, Box<dyn Error + Send + Sync>>,
    },
    /// The result of generating a chunk
    Generated {
        /// The identifier of the streaming operation
        streaming_identifier: u64,
        /// The coordinates of the chunk
        coordinates: TileCoordinates,
        /// The generated chunk
        result: Result<Box<Chunk>, Box<dyn Error + Send + Sync>>,
    },
}
