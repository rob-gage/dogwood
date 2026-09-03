// Copyright Rob Gage 2026

use super::{
    SceneChunk,
    SceneData,
    SceneGenerator,
};
use crate::tiles::TileCoordinates;
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::{
    error::Error,
    path::PathBuf,
    sync::Arc,
};

/// A scene that can be simulated by the engine
pub struct Scene {
    /// The `Accelerator` this `Scene` is running on
    accelerator: Arc<Accelerator>,
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
        accelerator: &Arc<Accelerator>,
        scene_data_path: impl Into<PathBuf>,
        simulation_dimensions: (u16, u16),
    ) -> Result<Self, Box<dyn Error>> {
        let accelerator: Arc<Accelerator> = accelerator.clone();
        let data: SceneData = SceneData::open(scene_data_path.into())?;
        let generator: Box<dyn SceneGenerator> = Box::new(());
        let (simulation_width, simulation_height): (u16, u16) = simulation_dimensions;
        let active_tile_count: usize = simulation_width as usize * simulation_height as usize;
        let tiles_offset: TileCoordinates = TileCoordinates {x: 0, y: 0};
        let cellular_particle_material_identifier_buffer: AcceleratorBuffer =
            accelerator.allocate::<u32>(active_tile_count * 64);
        Ok(Self {
            accelerator,
            data,
            generator,
            simulation_width,
            simulation_height,
            tiles: Box::new([]),
            tiles_offset,
            cellular_particle_material_identifier_buffer,
        })
    }

}