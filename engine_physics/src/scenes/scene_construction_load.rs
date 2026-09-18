// Copyright Rob Gage 2026

use std::error::Error;
use std::sync::Arc;

use engine_compute::Accelerator;

use super::Scene;
use crate::scenes::SceneData;
use crate::simulation::SceneSimulationConfiguration;

impl Scene {
    /// Loads a `Scene` from existing `SceneData`
    pub fn load(
        accelerator: &Arc<Accelerator>,
        simulation: SceneSimulationConfiguration,
        data: SceneData,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_with_generator(accelerator, simulation, data, ())
    }
}
