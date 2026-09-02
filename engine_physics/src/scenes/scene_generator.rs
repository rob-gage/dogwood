// Copyright Rob Gage 2026

use super::SceneChunk;

/// Generates `SceneChunks` that do not already exist in a `Scene`
pub trait SceneGenerator {

    /// Generates a `SceneChunk` at the provided chunk coordinates
    fn generate_chunk(&self, x: u64, y: u64) -> SceneChunk {
        self.generate_chunk_with_seed(0_u128, x, y)
    }

    /// Generates a `SceneChunk` at the provided chunk coordinates with a provided `u128` seed
    fn generate_chunk_with_seed(&self, seed: u128, x: u64, y: u64) -> SceneChunk;

}