// Copyright Rob Gage 2026

mod cellular_tile_buffers;
mod cellular_tile_coordinates;
mod cellular_tile_region;

pub use cellular_tile_buffers::CellularTileBuffers;
pub use cellular_tile_coordinates::CellularTileCoordinates;
pub use cellular_tile_region::CellularTileRegion;

/// Number of cells along each side of a cellular tile
pub const CELLULAR_TILE_SIZE: i64 = 8;
