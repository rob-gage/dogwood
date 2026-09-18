// Copyright Rob Gage 2026

use std::error::Error;
use std::io;

use super::Chunk;
use crate::tiles::TileCoordinates;

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
    /// A dirty chunk save completed outside the fixed tick
    Saved {
        /// The identifier of the save operation
        streaming_identifier: u64,
        /// The coordinates of the saved chunk
        coordinates: TileCoordinates,
        /// The unchanged chunk, retaining ownership on either success or failure
        result: Result<Box<Chunk>, (Box<Chunk>, io::Error)>,
    },
}
