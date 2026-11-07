// Copyright Rob Gage 2026

use crate::scenes::{
    cellular_tiles::{
        CellCoordinates,
        CellularTileCoordinates,
        CellularTileRegion,
        CELLULAR_TILE_SIZE,
    },
    collisions::CollisionCellularTile,
};

/// The solid static cells of the active region as the CPU knows them, in Rapier's cell coordinates
pub(crate) struct SoftBodyTerrain<'a> {
    /// Cell of the region's bottom-left corner relative to the Rapier origin
    pub(crate) minimum: [i64; 2],
    /// Region size in tiles
    pub(crate) size: [i64; 2],
    /// Occupancy of the region's tiles in row-major order
    pub(crate) tiles: &'a [CollisionCellularTile],
}

impl<'a> SoftBodyTerrain<'a> {

    /// Creates the view of a region's tiles relative to the cell at the Rapier origin
    pub(crate) fn new(
        region: CellularTileRegion,
        origin: [i64; 2],
        tiles: &'a [CollisionCellularTile],
    ) -> Self {
        Self {
            minimum: [
                (region.bottom_left.x - origin[0]) * CELLULAR_TILE_SIZE,
                (region.bottom_left.y - origin[1]) * CELLULAR_TILE_SIZE,
            ],
            size: [region.width() as i64, region.height() as i64],
            tiles,
        }
    }

    /// Returns whether a cell is inside the region
    #[inline]
    pub(crate) fn contains_cell(&self, cell: [i64; 2]) -> bool {
        let x: i64 = cell[0] - self.minimum[0];
        let y: i64 = cell[1] - self.minimum[1];
        x >= 0 && y >= 0 && x < self.size[0] * CELLULAR_TILE_SIZE
            && y < self.size[1] * CELLULAR_TILE_SIZE
    }

    /// Returns the center of the open cell nearest a solid one, searching a few cells around it
    pub(crate) fn nearest_open_position(
        &self,
        cell: [i64; 2],
        position: [f32; 2],
    ) -> Option<[f32; 2]> {
        let cells_per_tile: f32 = CELLULAR_TILE_SIZE as f32;
        for radius in 1..=4_i64 {
            let mut best: Option<([f32; 2], f32)> = None;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius { continue; }
                    let open: [i64; 2] = [cell[0] + dx, cell[1] + dy];
                    if self.is_solid(open) { continue; }
                    let center: [f32; 2] = [
                        (open[0] as f32 + 0.5) / cells_per_tile,
                        (open[1] as f32 + 0.5) / cells_per_tile,
                    ];
                    let distance: f32 = (center[0] - position[0]).powi(2)
                        + (center[1] - position[1]).powi(2);
                    if best.is_none_or(|(_, nearest)| distance < nearest) {
                        best = Some((center, distance));
                    }
                }
            }
            if let Some((center, _)) = best { return Some(center); }
        }
        None
    }

    /// Returns whether a cell is solid; cells outside the region are not
    #[inline]
    pub(crate) fn is_solid(&self, cell: [i64; 2]) -> bool {
        let x: i64 = cell[0] - self.minimum[0];
        let y: i64 = cell[1] - self.minimum[1];
        if x < 0 || y < 0 { return false; }
        let (tile_x, tile_y): (i64, i64) = (x / CELLULAR_TILE_SIZE, y / CELLULAR_TILE_SIZE);
        if tile_x >= self.size[0] || tile_y >= self.size[1] { return false; }
        let bit: i64 = (y % CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE + x % CELLULAR_TILE_SIZE;
        self.tiles[(tile_y * self.size[0] + tile_x) as usize].as_u64() >> bit & 1 == 1
    }

}

/// The solid static cells of the active region as the CPU knows them: rock, but not rigid bodies
/// or soft bodies, which move, and nothing beyond the region
pub struct SceneTerrain<'a> {
    /// Tile of the region's bottom-left corner
    origin: CellularTileCoordinates,
    /// Region size in tiles
    size: [i64; 2],
    /// Occupancy of the region's tiles in row-major order
    tiles: &'a [CollisionCellularTile],
}

impl<'a> SceneTerrain<'a> {

    /// Creates the view of a region's tiles
    pub(crate) fn new(region: CellularTileRegion, tiles: &'a [CollisionCellularTile]) -> Self {
        Self {
            origin: region.bottom_left,
            size: [region.width() as i64, region.height() as i64],
            tiles,
        }
    }

    /// Returns the tile at the region's bottom-left corner
    pub fn origin(&self) -> CellularTileCoordinates { self.origin }

    /// Returns the region's width and height in tiles
    pub fn size(&self) -> [i64; 2] { self.size }

    /// Returns a tile's solid cells as bit `y * 8 + x`, or `None` beyond the region
    pub fn tile(&self, tile: CellularTileCoordinates) -> Option<u64> {
        let (x, y): (i64, i64) = (tile.x - self.origin.x, tile.y - self.origin.y);
        if x < 0 || y < 0 || x >= self.size[0] || y >= self.size[1] { return None; }
        Some(self.tiles[(y * self.size[0] + x) as usize].as_u64())
    }

    /// Returns whether a cell is solid; cells beyond the region are not
    pub fn is_solid(&self, cell: CellCoordinates) -> bool {
        let tile: CellularTileCoordinates = CellularTileCoordinates {
            x: cell.x.div_euclid(CELLULAR_TILE_SIZE),
            y: cell.y.div_euclid(CELLULAR_TILE_SIZE),
        };
        let bit: i64 = cell.y.rem_euclid(CELLULAR_TILE_SIZE) * CELLULAR_TILE_SIZE
            + cell.x.rem_euclid(CELLULAR_TILE_SIZE);
        self.tile(tile).is_some_and(|tile: u64| tile >> bit & 1 == 1)
    }

}
