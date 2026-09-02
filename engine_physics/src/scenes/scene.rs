// Copyright Rob Gage 2026

use super::{
    SceneChunk,
    SceneData,
    SceneGenerator,
};
use crate::tiles::{
    TileCoordinates
};

/// A scene that can be simulated by the engine
pub struct Scene {
    /// The persistent `SceneData` backing this `Scene`
    data: SceneData,
    /// The `SceneGenerator` generating new tiles for this `Scene`
    generator: Box<dyn SceneGenerator>,
    /// The simulation width of this `Scene` in tiles
    simulation_width: u32,
    /// The height of this `Scene` in tiles
    simulation_height: u32,
    /// The active tiles in this `Scene`
    tiles: Box<[()]>,
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    tiles_offset: TileCoordinates,
}