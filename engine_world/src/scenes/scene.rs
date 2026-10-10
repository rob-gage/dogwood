// Copyright Rob Gage 2026

use super::scene_chunk_streaming_state::SceneChunkStreamingState;
use crate::{
    actors::ActorRegistry,
    cellular_tiles::CellularTileBuffers,
    chunks::ChunkData,
};
use engine_host::acceleration::Accelerator;
use std::sync::Arc;

/// A playable scene
pub struct Scene {
    /// Handles the streaming of chunks between the disk and native memory
    chunk_streaming_state: SceneChunkStreamingState,
    /// GPU storage for the properties of every resident cellular tile
    cellular_tile_buffers: CellularTileBuffers,
    /// ECS world and fixed-tick systems of every live actor
    actor_registry: ActorRegistry,
}

impl Scene {

    /// Creates a scene with chunk storage, a shared Rayon pool, and cellular tile buffers
    pub fn new(
        chunk_data: ChunkData,
        thread_pool: Arc<rayon::ThreadPool>,
        accelerator: &Accelerator,
        width: u32,
        height: u32,
    ) -> Self {
        let chunk_streaming_state: SceneChunkStreamingState =
            SceneChunkStreamingState::new(chunk_data, thread_pool);
        let cellular_tile_buffers: CellularTileBuffers =
            CellularTileBuffers::new(accelerator, width, height);
        let actor_registry: ActorRegistry = ActorRegistry::new();
        Self { chunk_streaming_state, cellular_tile_buffers, actor_registry }
    }

}
