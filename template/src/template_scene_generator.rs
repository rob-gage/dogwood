// Copyright Rob Gage 2026

use engine::{
    materials::MaterialIdentifier,
    scenes::{
        chunks::{
            Chunk,
            ChunkCoordinates,
        },
        SceneGenerator,
    },
};
use std::error::Error;

/// Generates stone below tile Y zero and empty space above it
pub struct TemplateSceneGenerator {
    /// Material filling every chunk below Y zero
    pub stone: MaterialIdentifier,
}

impl SceneGenerator for TemplateSceneGenerator {

    fn generate_chunk(
        &self,
        coordinates: ChunkCoordinates,
        _world_seed: u64,
    ) -> Result<Chunk, Box<dyn Error + Send + Sync>> {
        let mut chunk: Chunk = Chunk::new(coordinates);
        if coordinates.y < 0 {
            chunk.fill(self.stone, u32::from_le_bytes([255, 255, 255, 255]), u32::MAX);
        }
        Ok(chunk)
    }

}
