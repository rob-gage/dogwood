// Copyright Rob Gage 2026

/// The position of a tile within a `Scene`
///
/// Positions move higher up as Y increases, and they move further right as X increases.
pub struct TilePosition {
    pub x: i32,
    pub y: i32,
}