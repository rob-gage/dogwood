// Copyright Rob Gage 2026

mod chunk;
mod chunk_data;
mod chunk_coordinates;
mod chunk_error;
mod chunk_region;
mod chunk_region_index;
mod chunk_resident;
mod chunk_streaming_request;
mod chunk_streaming_response;
mod chunk_streaming_state;

pub use chunk::Chunk;
pub use chunk_data::ChunkData;
pub use chunk_coordinates::ChunkCoordinates;
pub use chunk_error::ChunkError;
pub use chunk_region::ChunkRegion;
pub use chunk_region_index::ChunkRegionIndex;
pub use chunk_streaming_state::ChunkStreamingState;

/// Number of cells along each side of a chunk
pub const CHUNK_SIZE: i64 = 512;
