// Copyright Rob Gage 2026

use super::{
    SceneChunk,
    SceneConfiguration,
    SceneData,
    SceneGenerator,
};
use crate::tiles::TileCoordinates;
use engine_compute::{
    Accelerator,
    AcceleratorBuffer
};
use std::{
    collections::HashMap,
    error::Error,
    path::PathBuf,
    sync::Arc,
};

/// A scene that can be simulated by the engine
pub struct Scene {
    /// The `Accelerator` this `Scene` is running on
    accelerator: Arc<Accelerator>,
    /// The `SceneConfiguration` for this `Scene`
    configuration: SceneConfiguration,
    /// The persistent `SceneData` backing this `Scene`
    data: SceneData,
    /// The `SceneGenerator` generating new tiles for this `Scene`
    generator: Box<dyn SceneGenerator>,
    /// Chunks in this scene indexed by their `TilePosition`s
    chunks: HashMap<TileCoordinates, SceneChunk>,
    /// The active tiles in this `Scene`
    tiles: Box<[()]>,
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    origin: TileCoordinates,
    /// The buffer containing `MaterialIdentifier`s for cellular particles
    cellular_particle_material_identifier_buffer: AcceleratorBuffer,
}

impl Scene {

    /// Creates a new `Scene` with provided dimensions
    pub fn new(
        accelerator: &Arc<Accelerator>,
        configuration: SceneConfiguration,
    ) -> Result<Self, Box<dyn Error>> {
        let accelerator: Arc<Accelerator> = accelerator.clone();
        let data: SceneData = SceneData::open(configuration.data_path.clone())?;
        let generator: Box<dyn SceneGenerator> = Box::new(());
        let active_tile_count: usize =
            configuration.width as usize * configuration.height as usize;
        let origin: TileCoordinates = TileCoordinates { x: 0, y: 0 };
        let cellular_particle_material_identifier_buffer: AcceleratorBuffer =
            accelerator.allocate::<u32>(active_tile_count * 64);
        Ok(Self {
            accelerator,
            configuration,
            data,
            generator,
            chunks: HashMap::new(),
            tiles: Box::new([]),
            origin,
            cellular_particle_material_identifier_buffer,
        })
    }

}

impl Drop for Scene {

    fn drop(&mut self) {
        self.cellular_particle_material_identifier_buffer.free()
    }

}