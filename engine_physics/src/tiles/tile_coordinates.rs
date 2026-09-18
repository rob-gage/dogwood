// Copyright Rob Gage 2026

/// The position of an eight-by-eight-cell tile within a `Scene`.
///
/// Positions move higher up as `y` increases, and further right as `x` increases.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct TileCoordinates {
    /// Horizontal tile coordinate in world space.
    pub x: i32,
    /// Vertical tile coordinate in world space.
    pub y: i32,
}

impl TileCoordinates {
    /// Returns the bottom-left tile coordinates of the `Chunk` containing this tile.
    pub const fn chunk_coordinates(self) -> TileCoordinates {
        TileCoordinates {
            x: self.x.div_euclid(64) * 64,
            y: self.y.div_euclid(64) * 64,
        }
    }
}
