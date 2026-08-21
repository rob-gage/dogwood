// Copyright Rob Gage 2026

use crate::tiles::{
    TilePosition
};

/// A scene that can be simulated by the engine
pub struct Scene<
    const WIDTH: usize,
    const HEIGHT: usize,
> {
    /// The active tiles in this `Scene`
    tiles: [[(); HEIGHT]; WIDTH],
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    tiles_offset: TilePosition,
}