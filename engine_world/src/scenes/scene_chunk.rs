// Copyright Rob Gage 2026

use super::scene_error::SceneError;
use crate::chunks::Chunk;

/// Resident chunk data or one outstanding chunk streaming request
pub enum SceneChunk {
    /// Loaded CPU chunk owned by this scene
    Active {
        /// Resident chunk data
        chunk: Chunk,
        /// Whether the chunk has authoritative changes to persist
        is_dirty: bool,
    },
    /// Chunk load running in the shared processor pool
    Loading {
        /// Identifier used to reject stale load completions
        streaming_request_identifier: u64,
    },
    /// Dirty chunk ownership is held by a background save request
    Saving {
        /// Identifier used to reject stale save completions
        streaming_request_identifier: u64,
    },
    /// Failed load or save retained for inspection and explicit retry
    Error {
        /// Storage error returned by the chunk data API
        error: SceneError,
        /// Dirty chunk returned after a failed save
        chunk: Option<Chunk>,
        /// Whether the retained chunk still needs persistence
        is_dirty: bool,
    },
}
