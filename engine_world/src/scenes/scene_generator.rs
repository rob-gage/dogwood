// Copyright Rob Gage 2026

use crate::chunks::{
    Chunk,
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

}
