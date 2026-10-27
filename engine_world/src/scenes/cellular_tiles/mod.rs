// Copyright Rob Gage 2026

mod cell_coordinates;
mod cellular_tile_buffers;
mod cellular_tile_coordinates;
mod cellular_tile_index;
mod cellular_tile_pass;
mod cellular_tile_region;
mod cellular_tile_streaming_request;
mod cellular_tile_streaming_response;
mod cellular_tile_streaming_state;
mod cellular_tiles;

pub use cell_coordinates::CellCoordinates;
pub use cellular_tile_buffers::CellularTileBuffers;
pub use cellular_tile_coordinates::CellularTileCoordinates;
pub use cellular_tile_index::CellularTileIndex;
pub use cellular_tile_region::CellularTileRegion;
pub use cellular_tile_streaming_state::CellularTileStreamingState;
pub use cellular_tiles::CellularTiles;

/// Number of cells along each side of a cellular tile
pub const CELLULAR_TILE_SIZE: i64 = 8;

/// Awake buffer value of a tile whose dynamic cells, and those of its neighbors, must be simulated
pub const CELLULAR_TILE_AWAKE: u32 = 0b11;

/// Gas awake buffer value of a tile whose gas, and that of its neighbors, must be simulated
pub const CELLULAR_TILE_GAS_AWAKE: u32 = 0b1;

/// Thermal awake buffer value of a tile whose temperatures, and those of its neighbors, must be
/// simulated
pub const CELLULAR_TILE_THERMAL_AWAKE: u32 = 0b1;

/// Amount of a cell or fluid particle holding one whole unit of its material
pub const CELLULAR_TILE_AMOUNT_PER_UNIT: u32 = 65_536;

/// Duration of one fixed tick in seconds, matching the collision step
const CELLULAR_TILE_TIME_STEP: f32 = 1.0 / 60.0;

/// Maximum number of actors pushing and resting on dynamic cells in one tick
pub const CELLULAR_TILE_ACTOR_COUNT_MAXIMUM: usize = 32;

/// Largest number of passes resolving cells that move together, each doubling the longest chain
const CELLULAR_TILE_RESOLVE_COUNT: usize = 8;

/// Mass given to fixed bodies, which dynamic cells never move
const CELLULAR_TILE_FIXED_MASS: f32 = 1.0e20;
