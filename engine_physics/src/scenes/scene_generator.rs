// Copyright Rob Gage 2026

use super::SceneChunk;
use crate::tiles::TileCoordinates;

/// Generates `SceneChunks` that do not already exist in a `Scene`
pub trait SceneGenerator {

    /// Generates a `SceneChunk` at the provided `TileCoordinates`
    fn generate_chunk(&self, tile_coordinates: TileCoordinates) -> SceneChunk {
        self.generate_chunk_with_seed(0_u128, tile_coordinates)
    }

    /// Generates a `SceneChunk` at the provided `TileCoordinates` with a provided seed
    fn generate_chunk_with_seed(&self, seed: u128, tile_coordinates: TileCoordinates) -> SceneChunk;

}

impl SceneGenerator for () {

    fn generate_chunk_with_seed(&self, _: u128, tile_coordinates: TileCoordinates) -> SceneChunk {
        SceneChunk::new_empty(tile_coordinates)
    }

}
