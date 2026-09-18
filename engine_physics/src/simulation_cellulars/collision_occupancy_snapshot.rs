// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;

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
    pub(crate) fn dynamic_tile_mask(&self, world_tile_x: i32, world_tile_y: i32) -> [u32; 2] {
        let snapshot_tile_x: i32 = world_tile_x - self.origin.x;
        let snapshot_tile_y: i32 = world_tile_y - self.origin.y;
        if snapshot_tile_x < 0
            || snapshot_tile_y < 0
            || snapshot_tile_x >= i32::from(self.width)
            || snapshot_tile_y >= i32::from(self.height)
        {
            return [0; 2];
        }
        self.dynamic_masks
            [snapshot_tile_y as usize * usize::from(self.width) + snapshot_tile_x as usize]
    }

    pub(crate) fn static_patch_masks(
        &self,
        world_patch_x: i32,
        world_patch_y: i32,
    ) -> [[u32; 2]; 16] {
        let mut patch_masks: [[u32; 2]; 16] = [[0; 2]; 16];
        for patch_tile_y in 0..4 {
            for patch_tile_x in 0..4 {
                let snapshot_tile_x: i32 = world_patch_x * 4 + patch_tile_x - self.origin.x;
                let snapshot_tile_y: i32 = world_patch_y * 4 + patch_tile_y - self.origin.y;
                if snapshot_tile_x >= 0
                    && snapshot_tile_y >= 0
                    && snapshot_tile_x < i32::from(self.width)
                    && snapshot_tile_y < i32::from(self.height)
                {
                    patch_masks[(patch_tile_y * 4 + patch_tile_x) as usize] = self.static_masks
                        [snapshot_tile_y as usize * usize::from(self.width)
                            + snapshot_tile_x as usize];
                }
            }
        }
        patch_masks
    }

    /// Returns whether a world cell contains static material
    pub(crate) fn is_static_cell_occupied(
        &self,
        world_cell_x: i32,
        world_cell_y: i32,
    ) -> Option<bool> {
        self.is_cell_occupied(&self.static_masks, world_cell_x, world_cell_y)
    }

    /// Looks up one world cell in a provided logical tile-mask array
    fn is_cell_occupied(
        &self,
        occupancy_masks: &[[u32; 2]],
        world_cell_x: i32,
        world_cell_y: i32,
    ) -> Option<bool> {
        let snapshot_tile_x: i32 = world_cell_x.div_euclid(8) - self.origin.x;
        let snapshot_tile_y: i32 = world_cell_y.div_euclid(8) - self.origin.y;
        if snapshot_tile_x < 0
            || snapshot_tile_y < 0
            || snapshot_tile_x >= i32::from(self.width)
            || snapshot_tile_y >= i32::from(self.height)
        {
            return None;
        }
        let tile_index: usize =
            snapshot_tile_y as usize * usize::from(self.width) + snapshot_tile_x as usize;
        let cell_index: usize =
            world_cell_y.rem_euclid(8) as usize * 8 + world_cell_x.rem_euclid(8) as usize;
        Some(occupancy_masks[tile_index][cell_index / 32] & (1 << (cell_index % 32)) != 0)
    }

    /// Removes one static cell from this derived snapshot
    #[cfg(test)]
    pub(crate) fn clear_static_cell(&mut self, world_cell_x: i32, world_cell_y: i32) {
        let snapshot_tile_x: i32 = world_cell_x.div_euclid(8) - self.origin.x;
        let snapshot_tile_y: i32 = world_cell_y.div_euclid(8) - self.origin.y;
        if snapshot_tile_x < 0
            || snapshot_tile_y < 0
            || snapshot_tile_x >= i32::from(self.width)
            || snapshot_tile_y >= i32::from(self.height)
        {
            return;
        }
        let tile_index: usize =
            snapshot_tile_y as usize * usize::from(self.width) + snapshot_tile_x as usize;
        let cell_index: usize =
            world_cell_y.rem_euclid(8) as usize * 8 + world_cell_x.rem_euclid(8) as usize;
        self.static_masks[tile_index][cell_index / 32] &= !(1 << (cell_index % 32));
    }
}
