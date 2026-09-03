// Copyright Rob Gage 2026

/// The position of a tile within a `Scene`
///
/// Positions move higher up as Y increases, and they move further right as X increases.
#[derive(Copy, Clone)]
pub struct TileCoordinates {
    pub x: i32,
    pub y: i32,
}
