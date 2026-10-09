// Copyright Rob Gage 2026

mod chunk;
mod chunk_data;
mod chunk_coordinates;
mod chunk_error;

pub use chunk::Chunk;
pub use chunk_data::ChunkData;
pub use chunk_coordinates::ChunkCoordinates;
pub use chunk_error::ChunkError;

/// Number of cells along each side of a chunk
pub const CHUNK_SIZE: i64 = 512;
