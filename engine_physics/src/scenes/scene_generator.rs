// Copyright Rob Gage 2026

use crate::{chunks::Chunk, tiles::TileCoordinates};

/// Generates `Chunks` that do not already exist in a `Scene`
pub trait SceneGenerator: Send + Sync {
    /// Generates a `Chunk` at the provided `TileCoordinates`
    fn generate_chunk(&self, tile_coordinates: TileCoordinates) -> Chunk {
        self.generate_chunk_with_seed(0_u128, tile_coordinates)
    }

    /// Generates a `Chunk` at the provided `TileCoordinates` with a provided seed
    fn generate_chunk_with_seed(&self, seed: u128, tile_coordinates: TileCoordinates) -> Chunk;
}

impl SceneGenerator for () {
    fn generate_chunk_with_seed(&self, _: u128, tile_coordinates: TileCoordinates) -> Chunk {
        Chunk::new_empty(tile_coordinates)
    }
}
