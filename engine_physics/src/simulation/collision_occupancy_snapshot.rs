// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;

#[allow(dead_code)]
/// A completed CPU collision snapshot in logical buffered-tile order
pub struct CollisionOccupancySnapshot {
    /// The dispatch order used to reject older out-of-order completions
    pub sequence: u64,
    /// The logical coordinates of the snapshot's bottom-left tile
    pub origin: TileCoordinates,
    /// The snapshot width in tiles
    pub width: u16,
    /// The snapshot height in tiles
    pub height: u16,
    /// Two occupancy words for each logical tile in row-major order
    pub masks: Box<[[u32; 2]]>,
}
