// Copyright Rob Gage 2026

use super::ChunkCoordinates;

/// Inclusive chunk-coordinate bounds of one rectangular region
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChunkRegion {
    /// Bottom-left chunk coordinates
    pub bottom_left: ChunkCoordinates,
    /// Top-right chunk coordinates
    pub top_right: ChunkCoordinates,
}

impl ChunkRegion {

    /// Returns the inclusive region width, saturating at the platform maximum
    pub fn width(&self) -> usize {
        let width: i64 = i64::from(self.top_right.x) - i64::from(self.bottom_left.x) + 1;
        if width <= 0 { return 0; }
        usize::try_from(width).unwrap_or(usize::MAX)
    }

    /// Returns the inclusive region height, saturating at the platform maximum
    pub fn height(&self) -> usize {
        let height: i64 = i64::from(self.top_right.y) - i64::from(self.bottom_left.y) + 1;
        if height <= 0 { return 0; }
        usize::try_from(height).unwrap_or(usize::MAX)
    }

}
