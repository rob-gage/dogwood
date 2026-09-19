// Copyright Rob Gage 2026

use crate::tiles::CellCoordinates;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;

use super::Chunk;

/// Exact absolute bounds of one requested chunk generation region.
#[derive(Copy, Clone)]
pub struct ChunkGenerationRegion {
    /// Bottom-left tile coordinate owned by this chunk.
    pub tile_coordinates: TileCoordinates,
    /// Absolute tile bounds owned by this chunk.
    pub tile_area: TileArea,
    /// Bottom-left cell coordinate owned by this chunk.
    pub cell_origin: CellCoordinates,
    /// Number of cells in the region along each axis.
    pub cell_dimensions: [u16; 2],
}

impl ChunkGenerationRegion {
    /// Creates the region for a chunk at its bottom-left tile coordinate.
    pub const fn new(tile_coordinates: TileCoordinates) -> Self {
        Self {
            tile_coordinates,
            tile_area: TileArea::new(tile_coordinates, Self::CHUNK_WIDTH, Self::CHUNK_WIDTH),
            cell_origin: CellCoordinates {
                x: tile_coordinates.x * 8,
                y: tile_coordinates.y * 8,
            },
            cell_dimensions: [Self::CHUNK_WIDTH * 8, Self::CHUNK_WIDTH * 8],
        }
    }

    /// The width of the owned chunk in tiles.
    pub const CHUNK_WIDTH: u16 = Chunk::WIDTH;

    /// Returns whether an absolute cell coordinate belongs to this region.
    pub fn contains_cell(self, coordinates: CellCoordinates) -> bool {
        coordinates.x >= self.cell_origin.x
            && coordinates.y >= self.cell_origin.y
            && coordinates.x < self.cell_origin.x + i32::from(self.cell_dimensions[0])
            && coordinates.y < self.cell_origin.y + i32::from(self.cell_dimensions[1])
    }
}
