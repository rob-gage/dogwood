// Copyright Rob Gage 2026

use super::chunks::{
    ChunkCoordinates,
    ChunkError,
    ChunkRegionIndex,
};
use crate::actors::{
    Actor,
    ActorError,
};
use std::{
    error::Error,
    sync::Arc,
};

/// An error encountered while using a scene
#[derive(Clone, Debug)]
pub enum SceneError {
    /// Fluid capacity factor is not finite or is outside the range `(0, 1]`
    InvalidFluidCapacityFactor,
    /// The fluid particle pool already exists and its capacity can no longer change
    FluidCapacityFixed,
    /// Ambient gas pressure is not finite or is outside the range `[0, 256)` atmospheres
    InvalidAmbientGasPressure,
    /// Ambient gas mass is not finite or is not positive
    InvalidAmbientGasMass,
    /// The scene already has its maximum number of chunk region requests
    ChunkRegionRequestLimitReached,
    /// A chunk region has reversed bounds or exceeds the per-side size limit
    InvalidChunkRegion,
    /// The requested chunk regions and dirty chunks exceed the scene resident limit
    ChunkResidentLimitReached,
    /// No active request has the supplied region identifier
    ChunkRegionRequestNotFound {
        /// Identifier of the missing region request
        chunk_region_identifier: ChunkRegionIndex,
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
    /// A chunk generator failed to produce the requested chunk
    ChunkGeneration {
        /// Coordinates of the chunk that failed to generate
        coordinates: ChunkCoordinates,
        /// Generation error returned by the scene generator
        error: Arc<dyn Error + Send + Sync>,
    },
    /// The processor has no thread pool for chunk streaming
    ChunkThreadPoolUnavailable,
    /// A camera actor used for streaming lacks its camera or position
    CameraActor {
        /// Error returned while accessing the camera actor
        error: ActorError,
    },
    /// A cellular tile region is empty or does not match the GPU tile ring dimensions
    InvalidCellularTileRegion,
    /// A cellular tile streaming request staging buffer could not be mapped
    CellularTileStreamingResponse {
        /// Mapping error returned by WGPU
        error: wgpu::BufferAsyncError,
    },
    /// No outgoing tile request has the supplied identifier
    CellularTileStreamingRequestNotFound,
    /// A cellular tile streaming response cannot be applied because its chunk is not active
    CellularTileStreamingResponseChunkInactive {
        /// Coordinates of the inactive chunk
        coordinates: ChunkCoordinates,
    },
    /// A returned fluid particle record could not be validated for chunk storage
    FluidParticleStreamingRecordInvalid,
    /// A returned particle readback cannot be applied because its chunk is not active
    FluidParticleStreamingResponseChunkInactive {
        /// Coordinates of the inactive chunk
        coordinates: ChunkCoordinates,
    },
    /// The accelerator could not be polled for cellular tile streaming responses
    CellularTileStreamingPoll,
    /// A collision cellular tile staging buffer could not be mapped
    CollisionCellularTileResponse {
        /// Mapping error returned by WGPU
        error: wgpu::BufferAsyncError,
    },
    /// The accelerator could not be polled for collision cellular tiles
    CollisionCellularTilePoll,
    /// A cellular tile contact readback staging buffer could not be mapped
    CellularTileResponse {
        /// Mapping error returned by WGPU
        error: wgpu::BufferAsyncError,
    },
    /// The accelerator could not be polled for cellular tile contacts
    CellularTilePoll,
    /// A fluid particle count readback staging buffer could not be mapped
    FluidParticleResponse {
        /// Mapping error returned by WGPU
        error: wgpu::BufferAsyncError,
    },
    /// The accelerator could not be polled for fluid particle count readbacks
    FluidParticlePoll,
    /// A fluid export readback buffer does not match the compaction export buffer
    FluidParticleExportBufferSizeInvalid,
    /// A mapped fluid particle export record contains invalid world position or state
    FluidParticleExportRecordInvalid,
    /// The asynchronous resident particle snapshot channel closed unexpectedly
    FluidParticleSnapshotResponseChannelClosed,
    /// An actor's collider has a nonpositive, nonfinite, or capsule-violating dimension or mass
    InvalidCollider {
        /// Actor owning the invalid collider
        actor: Actor,
    },
    /// An actor leaving the active region could not be despawned
    ActorDespawn {
        /// Error returned while despawning the actor
        error: ActorError,
    },
}
