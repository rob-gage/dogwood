// Copyright Rob Gage 2026

use super::cellular_tiles::{
    CellCoordinates,
    CELLULAR_TILE_SIZE,
};

/// Exact scene position as a cellular tile plus a fixed-point offset within it
#[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ScenePosition {
    /// Horizontal cellular tile coordinate
    pub tile_x: i64,
    /// Vertical cellular tile coordinate
    pub tile_y: i64,
    /// Horizontal offset within the tile in units of `1 / 2³²` tile
    pub offset_x: u32,
    /// Vertical offset within the tile in units of `1 / 2³²` tile
    pub offset_y: u32,
}

impl ScenePosition {

    /// Number of offset units in one tile
    pub const OFFSET_UNITS_PER_TILE: f64 = 4_294_967_296.0;

    /// Returns this position's offset from an origin in tiles, subtracting tiles exactly first
    pub fn relative_position(self, origin: ScenePosition) -> (f64, f64) {
        let tile_difference_x: i128 = i128::from(self.tile_x) - i128::from(origin.tile_x);
        let tile_difference_y: i128 = i128::from(self.tile_y) - i128::from(origin.tile_y);
        let offset_difference_x: i64 = i64::from(self.offset_x) - i64::from(origin.offset_x);
        let offset_difference_y: i64 = i64::from(self.offset_y) - i64::from(origin.offset_y);
        (
            tile_difference_x as f64 + offset_difference_x as f64 / Self::OFFSET_UNITS_PER_TILE,
            tile_difference_y as f64 + offset_difference_y as f64 / Self::OFFSET_UNITS_PER_TILE,
        )
    }

    /// Returns the cell containing this position
    pub const fn cell_coordinates(self) -> CellCoordinates {
        let x: i64 = self.tile_x * CELLULAR_TILE_SIZE + (self.offset_x >> 29) as i64;
        let y: i64 = self.tile_y * CELLULAR_TILE_SIZE + (self.offset_y >> 29) as i64;
        CellCoordinates { x, y }
    }

    /// Returns this position moved by an offset in tiles, the inverse of `relative_position`
    pub fn translate(self, x: f64, y: f64) -> ScenePosition {
        // carry whole tiles out of the offset, leaving an axis unchanged by a nonfinite move
        let translate_axis = |tile: i64, offset: u32, distance: f64| -> (i64, u32) {
            if !distance.is_finite() { return (tile, offset); }
            let tiles: f64 = f64::from(offset) / Self::OFFSET_UNITS_PER_TILE + distance;
            let whole_tiles: f64 = tiles.floor();
            let offset: f64 = (tiles - whole_tiles) * Self::OFFSET_UNITS_PER_TILE;
            if offset >= Self::OFFSET_UNITS_PER_TILE { (tile + whole_tiles as i64 + 1, 0) }
            else { (tile + whole_tiles as i64, offset as u32) }
        };
        let (tile_x, offset_x): (i64, u32) = translate_axis(self.tile_x, self.offset_x, x);
        let (tile_y, offset_y): (i64, u32) = translate_axis(self.tile_y, self.offset_y, y);
        ScenePosition { tile_x, tile_y, offset_x, offset_y }
    }

}
