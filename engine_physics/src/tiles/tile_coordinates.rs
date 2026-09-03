// Copyright Rob Gage 2026

/// The position of a tile within a `Scene`
///
/// Positions move higher up as Y increases, and they move further right as X increases.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct TileCoordinates {
    pub x: i32,
    pub y: i32,
}

impl TileCoordinates {

    /// Returns the `TileCoordinates` of the `SceneChunk` that these `TileCoordinates` are in
    pub const fn chunk_coordinates(self) -> TileCoordinates {
        TileCoordinates { x: self.x.div_euclid(64) * 64, y: self.y.div_euclid(64) * 64, }
    }

}
