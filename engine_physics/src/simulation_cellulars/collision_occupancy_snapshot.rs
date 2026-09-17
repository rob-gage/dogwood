// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;

#[allow(dead_code)]
#[derive(Clone)]
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
    /// Static occupancy words for each logical tile in row-major order
    pub(crate) static_masks: Box<[[u32; 2]]>,
    /// Dynamic occupancy words for each logical tile in row-major order
    pub(crate) dynamic_masks: Box<[[u32; 2]]>,
}

impl CollisionOccupancySnapshot {
    pub(crate) fn dynamic_tile_mask(&self, tile_x: i32, tile_y: i32) -> [u32; 2] {
        let x = tile_x - self.origin.x;
        let y = tile_y - self.origin.y;
        if x < 0 || y < 0 || x >= i32::from(self.width) || y >= i32::from(self.height) {
            return [0; 2];
        }
        self.dynamic_masks[y as usize * usize::from(self.width) + x as usize]
    }

    pub(crate) fn static_patch_masks(&self, patch_x: i32, patch_y: i32) -> [[u32; 2]; 16] {
        let mut result = [[0; 2]; 16];
        for y in 0..4 {
            for x in 0..4 {
                let tx = patch_x * 4 + x - self.origin.x;
                let ty = patch_y * 4 + y - self.origin.y;
                if tx >= 0 && ty >= 0 && tx < i32::from(self.width) && ty < i32::from(self.height) {
                    result[(y * 4 + x) as usize] =
                        self.static_masks[ty as usize * usize::from(self.width) + tx as usize];
                }
            }
        }
        result
    }

    /// Returns whether a world cell contains static material
    pub(crate) fn is_static_cell_occupied(&self, x: i32, y: i32) -> Option<bool> {
        self.is_cell_occupied(&self.static_masks, x, y)
    }

    /// Looks up one world cell in a provided logical tile-mask array
    fn is_cell_occupied(&self, masks: &[[u32; 2]], x: i32, y: i32) -> Option<bool> {
        let tile_x: i32 = x.div_euclid(8) - self.origin.x;
        let tile_y: i32 = y.div_euclid(8) - self.origin.y;
        if tile_x < 0
            || tile_y < 0
            || tile_x >= i32::from(self.width)
            || tile_y >= i32::from(self.height)
        {
            return None;
        }
        let tile: usize = tile_y as usize * usize::from(self.width) + tile_x as usize;
        let cell: usize = y.rem_euclid(8) as usize * 8 + x.rem_euclid(8) as usize;
        Some(masks[tile][cell / 32] & (1 << (cell % 32)) != 0)
    }

    /// Removes one static cell from this derived snapshot
    pub(crate) fn clear_static_cell(&mut self, x: i32, y: i32) {
        let tile_x: i32 = x.div_euclid(8) - self.origin.x;
        let tile_y: i32 = y.div_euclid(8) - self.origin.y;
        if tile_x < 0
            || tile_y < 0
            || tile_x >= i32::from(self.width)
            || tile_y >= i32::from(self.height)
        {
            return;
        }
        let tile: usize = tile_y as usize * usize::from(self.width) + tile_x as usize;
        let cell: usize = y.rem_euclid(8) as usize * 8 + x.rem_euclid(8) as usize;
        self.static_masks[tile][cell / 32] &= !(1 << (cell % 32));
    }
}
