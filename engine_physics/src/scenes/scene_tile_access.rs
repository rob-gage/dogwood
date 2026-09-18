// Copyright Rob Gage 2026

use super::Scene;
use crate::tiles::Tile;
use crate::tiles::TileCoordinates;

impl Scene {
    /// Returns the active tile at the provided tile coordinates if one exists.
    pub fn tile_at(&self, coordinates: TileCoordinates) -> Option<Tile> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let buffered_tile_width: usize = self.simulation_width as usize + buffer_size as usize * 2;
        let buffered_tile_x: usize =
            usize::try_from(coordinates.x - (self.origin.x - buffer_size)).ok()?;
        let buffered_tile_y: usize =
            usize::try_from(coordinates.y - (self.origin.y - buffer_size)).ok()?;
        let buffered_tile_height: usize =
            self.simulation_height as usize + buffer_size as usize * 2;
        if buffered_tile_x >= buffered_tile_width || buffered_tile_y >= buffered_tile_height {
            return None;
        }
        let ring_buffer_tile_x: usize =
            (buffered_tile_x + self.tiles_ring_offset_x as usize) % buffered_tile_width;
        let ring_buffer_tile_y: usize =
            (buffered_tile_y + self.tiles_ring_offset_y as usize) % buffered_tile_height;
        self.tiles
            .get(ring_buffer_tile_y * buffered_tile_width + ring_buffer_tile_x)
            .copied()
    }
}
