// Copyright Rob Gage 2026

use super::chunks::{
    Chunk,
    ChunkBackground,
    ChunkCoordinates,
};
use std::error::Error;

/// Generates scene chunks that have no persisted file
pub trait SceneGenerator: Send + Sync {

    /// Generates one chunk at absolute chunk coordinates using a stable world seed
    fn generate_chunk(
        &self,
        coordinates: ChunkCoordinates,
        world_seed: u64,
    ) -> Result<Chunk, Box<dyn Error + Send + Sync>>;

    /// Generates the background behind one chunk, whether it was generated or loaded, on a
    /// streaming thread, after `generate_chunk` when the chunk was generated; backgrounds are not
    /// saved, so this must be deterministic
    fn generate_background(
        &self,
        _coordinates: ChunkCoordinates,
        _world_seed: u64,
    ) -> Option<ChunkBackground> { None }

}
