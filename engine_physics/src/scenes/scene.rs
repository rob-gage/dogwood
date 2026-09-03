// Copyright Rob Gage 2026

use super::{
    SceneChunk,
    SceneData,
    SceneGenerator,
};
use crate::tiles::TileCoordinates;
use engine_compute::AcceleratorBuffer;
use std::{
    error::Error,
    path::PathBuf
};

/// A scene that can be simulated by the engine
pub struct Scene {
    /// The persistent `SceneData` backing this `Scene`
    data: SceneData,
    /// The `SceneGenerator` generating new tiles for this `Scene`
    generator: Box<dyn SceneGenerator>,
    /// The simulation width of this `Scene` in tiles
    simulation_width: u16,
    /// The height of this `Scene` in tiles
    simulation_height: u16,
    /// The active tiles in this `Scene`
    tiles: Box<[()]>,
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    tiles_offset: TileCoordinates,
    /// The buffer containing `MaterialIdentifier`s for cellular particles
    cellular_particle_material_identifier_buffer: AcceleratorBuffer,
}

impl Scene {

    /// Creates a new `Scene` with provided dimensions
    pub fn new(
        scene_data_path: impl Into<PathBuf>,
        simulation_dimensions: (u16, u16),
    ) -> Result<Self, Box<dyn Error>> {
        todo!()
    }

}