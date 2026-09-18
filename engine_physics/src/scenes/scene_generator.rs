// Copyright Rob Gage 2026

use crate::actors::ActorPhysicalSpawn;
use crate::chunks::Chunk;
use crate::tiles::TileCoordinates;

/// Generates `Chunks` that do not already exist in a `Scene`
pub trait SceneGenerator: Send + Sync {
    /// Generates a `Chunk` at the provided `TileCoordinates`
    fn generate_chunk(&self, tile_coordinates: TileCoordinates) -> Chunk {
        self.generate_chunk_with_seed(0_u128, tile_coordinates)
    }

    /// Generates a `Chunk` at the provided `TileCoordinates` with a provided seed
    fn generate_chunk_with_seed(&self, seed: u128, tile_coordinates: TileCoordinates) -> Chunk;

    /// Generates initial generic actors for a newly instantiated region.
    fn generate_actor_spawns_with_seed(
        &self,
        _seed: u128,
        _tile_coordinates: TileCoordinates,
    ) -> Vec<ActorPhysicalSpawn> {
        Vec::new()
    }
}

impl SceneGenerator for () {
    fn generate_chunk_with_seed(&self, _: u128, tile_coordinates: TileCoordinates) -> Chunk {
        Chunk::new_empty(tile_coordinates)
    }
}
