// Copyright Rob Gage 2026

use super::CELLULAR_TILE_SIZE;
use crate::chunks::{
    ChunkCoordinates,
    CHUNK_SIZE,
};

/// Integer coordinates identifying one cellular tile
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CellularTileCoordinates {
    /// Horizontal cellular tile coordinate
    pub x: i64,
    /// Vertical cellular tile coordinate
    pub y: i64,
}

impl CellularTileCoordinates {

    /// Returns the coordinates of the chunk containing this cellular tile
    pub const fn chunk_coordinates(self) -> ChunkCoordinates {
        let cellular_tiles_per_chunk_side: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let chunk_x: i32 = self.x.div_euclid(cellular_tiles_per_chunk_side) as i32;
        let chunk_y: i32 = self.y.div_euclid(cellular_tiles_per_chunk_side) as i32;
        ChunkCoordinates { x: chunk_x, y: chunk_y }
    }

}
