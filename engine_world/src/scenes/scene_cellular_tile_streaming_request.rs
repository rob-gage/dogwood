// Copyright Rob Gage 2026

use super::scene_chunk_region_index::SceneChunkRegionIndex;
use crate::cellular_tiles::CellularTileCoordinates;

/// Outgoing cellular tiles copied into one staging buffer awaiting a response
pub struct SceneCellularTileStreamingRequest {
    /// Requested cellular tiles in staging buffer order
    pub coordinates: Vec<CellularTileCoordinates>,
    /// Staging buffer holding the copied material identifiers
    pub buffer: wgpu::Buffer,
    /// Retained chunk region covering every chunk receiving the requested material identifiers
    pub chunk_region_index: Option<SceneChunkRegionIndex>,
    /// Whether a response has been requested but not yet received
    pub is_response_requested: bool,
    /// Whether the response arrived and the staging buffer is readable
    pub is_response_ready: bool,
}
