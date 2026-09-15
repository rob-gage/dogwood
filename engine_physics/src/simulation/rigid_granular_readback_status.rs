// Copyright Rob Gage 2026

use super::RigidGranularReactionBatch;

/// The lifecycle of one ordered rigid/granular reaction readback
pub(crate) enum RigidGranularReadbackStatus {
    Available,
    Mapping,
    Complete(Result<RigidGranularReactionBatch, String>),
}
