// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;

#[allow(dead_code)]
/// A completed CPU collision snapshot in logical buffered-tile order
pub struct CollisionOccupancySnapshot {
    /// The dispatch order used to reject older out-of-order completions
    pub sequence: u64,
    /// The logical coordinates of the snapshot's bottom-left tile
    pub origin: TileCoordinates,
    /// The snapshot width in tiles
    pub width: u16,
    /// The snapshot height in tiles
    pub height: u16,
    /// Two occupancy words for each logical tile in row-major order
    pub masks: Box<[[u32; 2]]>,
}

impl CollisionOccupancySnapshot {

    /// Returns whether a world cell is occupied, or `None` when it is outside this snapshot
    pub fn is_cell_occupied(&self, x: i32, y: i32) -> Option<bool> {
        let tile_x: i32 = x.div_euclid(8) - self.origin.x;
        let tile_y: i32 = y.div_euclid(8) - self.origin.y;
        if tile_x < 0 || tile_y < 0 || tile_x >= i32::from(self.width) ||
                tile_y >= i32::from(self.height) {
            return None;
        }
        let tile: usize = tile_y as usize * usize::from(self.width) + tile_x as usize;
        let cell: usize = y.rem_euclid(8) as usize * 8 + x.rem_euclid(8) as usize;
        Some(self.masks[tile][cell / 32] & (1 << (cell % 32)) != 0)
    }

}
