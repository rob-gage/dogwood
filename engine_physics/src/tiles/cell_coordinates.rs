// Copyright Rob Gage 2026

/// The position of a cell within a `Scene`
///
/// Positions move higher up as Y increases, and they move further right as X increases.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct CellCoordinates {
    pub x: i32,
    pub y: i32,
}
