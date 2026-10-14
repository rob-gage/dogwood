// Copyright Rob Gage 2026

use super::{
    CellularTileCoordinates,
    CELLULAR_TILE_SIZE,
};

/// Integer coordinates identifying one cell
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CellCoordinates {
    /// Horizontal cell coordinate
    pub x: i64,
    /// Vertical cell coordinate
    pub y: i64,
}

impl CellCoordinates {

    /// Returns the coordinates of the cellular tile containing this cell
    pub const fn cellular_tile_coordinates(self) -> CellularTileCoordinates {
        CellularTileCoordinates {
            x: self.x.div_euclid(CELLULAR_TILE_SIZE),
            y: self.y.div_euclid(CELLULAR_TILE_SIZE),
        }
    }

}
