// Copyright Rob Gage 2026

use super::scene_chunk_region_index::SceneChunkRegionIndex;
use crate::chunks::{
    ChunkCoordinates,
    ChunkError,
};
use std::sync::Arc;

/// An error encountered while streaming scene chunks
#[derive(Clone, Debug)]
pub enum SceneError {
    /// The scene already has its maximum number of chunk region requests
    ChunkRegionRequestLimitReached,
    /// A chunk region has reversed bounds or exceeds the per-side size limit
    InvalidChunkRegion,
    /// The requested chunk regions and dirty chunks exceed the scene resident limit
    ChunkResidentLimitReached,
    /// No active request has the supplied region identifier
    ChunkRegionRequestNotFound {
        /// Identifier of the missing region request
        chunk_region_identifier: SceneChunkRegionIndex,
    },
    /// A chunk has no failed streaming request to retry
    ChunkRetryUnavailable {
        /// Chunk coordinates that could not be retried
        coordinates: ChunkCoordinates,
    },
    /// No bounded background request slot is available
    ChunkStreamingCapacityReached,
    /// A load was requested for a chunk that already has resident or pending state
    ChunkStreamingRequestAlreadyActive {
        /// Coordinates of the chunk that already has state
        coordinates: ChunkCoordinates,
    },
    /// The completion channel closed while streaming was active
    ChunkStreamingResponseChannelClosed,
    /// A requested chunk could not be loaded
    ChunkLoad {
        /// Coordinates of the chunk that failed to load
        coordinates: ChunkCoordinates,
        /// Storage error returned while loading
        error: Arc<ChunkError>,
    },
    /// A dirty chunk could not be saved
    ChunkSave {
        /// Coordinates of the chunk that failed to save
        coordinates: ChunkCoordinates,
        /// Storage error returned while saving
        error: Arc<ChunkError>,
    },
    /// A chunk load task panicked while reading persistent storage
    ChunkLoadPanicked {
        /// Coordinates of the chunk being loaded
        coordinates: ChunkCoordinates,
    },
    /// A chunk save task panicked while writing persistent storage
    ChunkSavePanicked {
        /// Coordinates of the chunk being saved
        coordinates: ChunkCoordinates,
    },
    /// A cellular tile region is empty or does not match the GPU tile ring dimensions
    InvalidCellularTileRegion,
    /// A cellular tile streaming request staging buffer could not be mapped
    CellularTileStreamingResponse {
        /// Mapping error returned by WGPU
        error: wgpu::BufferAsyncError,
    },
    /// A cellular tile streaming response cannot be applied because its chunk is not active
    CellularTileStreamingResponseChunkInactive {
        /// Coordinates of the inactive chunk
        coordinates: ChunkCoordinates,
    },
    /// The accelerator could not be polled for cellular tile streaming responses
    CellularTileStreamingPoll,
}
