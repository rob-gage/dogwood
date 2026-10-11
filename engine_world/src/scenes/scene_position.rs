// Copyright Rob Gage 2026

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

}
