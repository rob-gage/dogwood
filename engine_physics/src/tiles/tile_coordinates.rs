// Copyright Rob Gage 2026

/// The position of a tile within a `Scene`
///
/// Positions move higher up as Y increases, and they move further right as X increases.
#[derive(Copy, Clone)]
pub struct TileCoordinates {
    pub x: i32,
    pub y: i32,
}

impl TileCoordinates {

    /// Returns the region X coordinate of these `TileCoordinates`
    pub const fn region_coordinates_x(self) -> u64 {
        (self.x as i64 - i32::MIN as i64).div_euclid(64) as u64
    }

    /// Returns the region Y coordinate of these `TileCoordinates`
    pub const fn region_coordinates_y(self) -> u64 {
        (self.y as i64 - i32::MIN as i64).div_euclid(64) as u64
    }

}
