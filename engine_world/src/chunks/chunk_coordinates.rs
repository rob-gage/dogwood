// Copyright Rob Gage 2026

/// Integer coordinates identifying one chunk
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ChunkCoordinates {
    /// Horizontal chunk coordinate
    pub x: i32,
    /// Vertical chunk coordinate
    pub y: i32,
}