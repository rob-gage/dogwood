// Copyright Rob Gage 2026

use super::CellularTileCoordinates;
use crate::scenes::chunks::ChunkRegionIndex;

/// Outgoing cellular tiles copied into one staging buffer awaiting a response
pub struct CellularTileStreamingRequest {
    /// Requested cellular tiles in staging buffer order
    pub coordinates: Vec<CellularTileCoordinates>,
    /// Staging buffer holding the copied material identifiers
    pub buffer: wgpu::Buffer,
    /// Staging buffer holding particles exported by the matching compaction pass
    pub fluid_particle_buffer: Option<wgpu::Buffer>,
    /// Whether the fluid export copy has been submitted
    pub is_fluid_particle_export_submitted: bool,
    /// Whether a fluid export mapping is pending
    pub is_fluid_particle_response_requested: bool,
    /// Whether the fluid export mapping has completed
    pub is_fluid_particle_response_ready: bool,
    /// Retained chunk region covering every chunk receiving the requested material identifiers
    pub chunk_region_index: Option<ChunkRegionIndex>,
    /// Whether a response has been requested but not yet received
    pub is_response_requested: bool,
    /// Whether the response arrived and the staging buffer is readable
    pub is_response_ready: bool,
}
