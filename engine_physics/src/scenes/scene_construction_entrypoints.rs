// Copyright Rob Gage 2026

use std::error::Error;
use std::sync::Arc;

use super::Scene;
use crate::materials::MaterialRegistry;
use crate::scenes::SceneData;
use crate::simulation::SceneSimulationConfiguration;

impl Scene {
    /// Creates a temporary scene, its buffered accelerator storage, and initial chunks.
    pub fn new(
        accelerator: &Arc<engine_compute::Accelerator>,
        materials: MaterialRegistry,
        simulation: SceneSimulationConfiguration,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load(
            accelerator,
            simulation,
            SceneData::new_temporary(materials)?,
        )
    }

    /// Creates a temporary scene using a generator for missing chunks.
    pub fn new_with_generator(
        accelerator: &Arc<engine_compute::Accelerator>,
        materials: MaterialRegistry,
        simulation: SceneSimulationConfiguration,
        generator: impl super::SceneGenerator + 'static,
    ) -> Result<Self, Box<dyn Error>> {
        Self::new_with_generator_and_seed(accelerator, materials, simulation, 0, generator)
    }

    /// Creates a temporary scene with a generator and stable world seed.
    pub fn new_with_generator_and_seed(
        accelerator: &Arc<engine_compute::Accelerator>,
        materials: MaterialRegistry,
        simulation: SceneSimulationConfiguration,
        world_seed: u128,
        generator: impl super::SceneGenerator + 'static,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_with_generator_and_seed(
            accelerator,
            simulation,
            SceneData::new_temporary(materials)?,
            world_seed,
            generator,
        )
    }
}
