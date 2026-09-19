// Copyright Rob Gage 2026

use crate::actors::ActorPhysicalSpawn;
use crate::chunks::ChunkGenerationRegion;
use crate::chunks::ChunkInitializationWriter;
use crate::tiles::TileCoordinates;

/// Generates initial contents for chunks that have no persisted data.
pub trait SceneGenerator: Send + Sync {
    /// Fills a new chunk using absolute coordinates and the world seed.
    fn generate_chunk(
        &self,
        world_seed: u128,
        region: ChunkGenerationRegion,
        initialization: &mut ChunkInitializationWriter<'_>,
    );

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
    fn generate_chunk(
        &self,
        _: u128,
        _: ChunkGenerationRegion,
        _: &mut ChunkInitializationWriter<'_>,
    ) {
    }
}
