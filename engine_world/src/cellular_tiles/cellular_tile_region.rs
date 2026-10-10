// Copyright Rob Gage 2026

use super::CellularTileCoordinates;
use crate::chunks::ChunkRegion;

/// Inclusive cellular tile bounds of one rectangular region
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct CellularTileRegion {
    /// Bottom-left cellular tile coordinates
    pub bottom_left: CellularTileCoordinates,
    /// Top-right cellular tile coordinates
    pub top_right: CellularTileCoordinates,
}

impl CellularTileRegion {

    /// Returns the inclusive region width, saturating at the platform maximum
    pub fn width(&self) -> usize {
        let width: i128 = i128::from(self.top_right.x) - i128::from(self.bottom_left.x) + 1;
        if width <= 0 { return 0; }
        usize::try_from(width).unwrap_or(usize::MAX)
    }

    /// Returns the inclusive region height, saturating at the platform maximum
    pub fn height(&self) -> usize {
        let height: i128 = i128::from(self.top_right.y) - i128::from(self.bottom_left.y) + 1;
        if height <= 0 { return 0; }
        usize::try_from(height).unwrap_or(usize::MAX)
    }

    /// Returns whether the region contains the cellular tile coordinates
    pub fn contains(&self, coordinates: CellularTileCoordinates) -> bool {
        coordinates.x >= self.bottom_left.x
            && coordinates.x <= self.top_right.x
            && coordinates.y >= self.bottom_left.y
            && coordinates.y <= self.top_right.y
    }

    /// Returns the inclusive region of chunks containing this region
    pub fn chunk_region(&self) -> ChunkRegion {
        ChunkRegion {
            bottom_left: self.bottom_left.chunk_coordinates(),
            top_right: self.top_right.chunk_coordinates(),
        }
    }

    /// Iterates over the cellular tile coordinates in this region in row-major order
    pub fn cellular_tile_coordinates(&self) -> impl Iterator<Item = CellularTileCoordinates> {
        let bottom_left: CellularTileCoordinates = self.bottom_left;
        let top_right: CellularTileCoordinates = self.top_right;
        (bottom_left.y..=top_right.y).flat_map(move |y| {
            (bottom_left.x..=top_right.x).map(move |x| CellularTileCoordinates { x, y })
        })
    }

}
