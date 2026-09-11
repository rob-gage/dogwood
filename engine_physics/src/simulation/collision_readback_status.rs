// Copyright Rob Gage 2026

use super::CollisionOccupancySnapshot;

/// The lifecycle of one collision occupancy readback slot
pub enum CollisionReadbackStatus {
    /// The slot can accept a new GPU copy
    Available,
    /// A submitted copy is awaiting asynchronous mapping completion
    Mapping,
    /// The callback has produced a snapshot or readback error
    Complete(Result<CollisionOccupancySnapshot, String>),
}
