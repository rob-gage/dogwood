// Copyright Rob Gage 2026

mod cell_coordinates;
mod cellular_tile_buffers;
mod cellular_tile_coordinates;
mod cellular_tile_index;
mod cellular_tile_region;
mod cellular_tile_streaming_request;
mod cellular_tile_streaming_response;
mod cellular_tile_streaming_state;

pub use cell_coordinates::CellCoordinates;
pub use cellular_tile_buffers::CellularTileBuffers;
pub use cellular_tile_coordinates::CellularTileCoordinates;
pub use cellular_tile_index::CellularTileIndex;
pub use cellular_tile_region::CellularTileRegion;
pub use cellular_tile_streaming_state::CellularTileStreamingState;

/// Number of cells along each side of a cellular tile
pub const CELLULAR_TILE_SIZE: i64 = 8;

/// Awake buffer value of a tile whose dynamic cells, and those of its neighbors, must be simulated
pub const CELLULAR_TILE_AWAKE: u32 = 0b11;
