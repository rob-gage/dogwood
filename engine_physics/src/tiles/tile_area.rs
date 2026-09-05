// Copyright Rob Gage 2026

use super::TileCoordinates;

/// A rectangular area of tile coordinates
#[derive(Copy, Clone)]
pub struct TileArea {
    minimum: TileCoordinates,
    maximum: TileCoordinates,
}

impl TileArea {

    /// Creates a tile area from its bottom-left origin and dimensions
    pub const fn new(
        origin: TileCoordinates,
        width: u16,
        height: u16,
    ) -> Self {
        Self {
            minimum: origin,
            maximum: TileCoordinates {
                x: origin.x + width as i32 - 1,
                y: origin.y + height as i32 - 1,
            },
        }
    }

    /// Returns the chunk-aligned area containing this tile area
    pub const fn chunk_area(self) -> Self {
        Self {
            minimum: self.minimum.chunk_coordinates(),
            maximum: self.maximum.chunk_coordinates(),
        }
    }

    /// Returns whether the area contains the provided tile coordinates
    pub const fn contains(self, coordinates: TileCoordinates) -> bool {
        coordinates.x >= self.minimum.x && coordinates.x <= self.maximum.x &&
            coordinates.y >= self.minimum.y && coordinates.y <= self.maximum.y
    }

    /// Iterates over tile coordinates contained in this `TileArea`
    pub fn iterate_tile_coordinates(self) -> impl Iterator<Item = TileCoordinates> {
        (self.minimum.y..=self.maximum.y).flat_map(move |y| {
            (self.minimum.x..=self.maximum.x).map(move |x| TileCoordinates { x, y })
        })
    }

    /// Iterates over chunk coordinates contained in this `Chunk`-aligned `TileArea`
    pub fn iterate_chunk_coordinates(self) -> impl Iterator<Item = TileCoordinates> {
        (self.minimum.y..=self.maximum.y).step_by(64).flat_map(move |y| {
            (self.minimum.x..=self.maximum.x).step_by(64).map(move |x| TileCoordinates { x, y })
        })
    }

}
