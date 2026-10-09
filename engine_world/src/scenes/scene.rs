// Copyright Rob Gage 2026

use super::scene_chunk_streaming_state::SceneChunkStreamingState;
use crate::chunks::ChunkData;
use std::sync::Arc;
use rayon::ThreadPool;

/// A playable scene
pub struct Scene {
    /// Handles the streaming of chunks between the disk and native memory
    chunk_streaming_state: SceneChunkStreamingState,
}

impl Scene {

    /// Creates a scene using the supplied shared Rayon pool and chunk storage
    pub fn new(chunk_data: ChunkData, thread_pool: Arc<ThreadPool>) -> Self {
        let chunk_streaming_state: SceneChunkStreamingState =
            SceneChunkStreamingState::new(chunk_data, thread_pool);
        Self { chunk_streaming_state }
    }

}
