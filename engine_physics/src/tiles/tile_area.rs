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

    /// Expands this area by the provided tile distances
    pub(crate) fn expanded(
        self,
        left: u16,
        bottom: u16,
        right: u16,
        top: u16,
    ) -> Self {
        Self {
            minimum: TileCoordinates {
                x: self.minimum.x.checked_sub(left as i32).unwrap_or(self.minimum.x),
                y: self.minimum.y.checked_sub(bottom as i32).unwrap_or(self.minimum.y),
            },
            maximum: TileCoordinates {
                x: self.maximum.x.checked_add(right as i32).unwrap_or(self.maximum.x),
                y: self.maximum.y.checked_add(top as i32).unwrap_or(self.maximum.y),
            },
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

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn expands_a_chunk_area_toward_streaming_direction() {
        let area: TileArea = TileArea::new(TileCoordinates { x: 0, y: 0 }, 64, 64)
            .chunk_area().expanded(0, 0, 64, 0);
        let chunks: Vec<TileCoordinates> = area.iterate_chunk_coordinates().collect();

        assert!(chunks.len() == 2);
        assert!(area.contains(TileCoordinates { x: 0, y: 0 }));
        assert!(area.contains(TileCoordinates { x: 64, y: 0 }));
        assert!(!area.contains(TileCoordinates { x: -64, y: 0 }));
    }

}
