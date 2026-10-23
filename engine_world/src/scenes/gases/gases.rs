// Copyright Rob Gage 2026

use super::{
    gas_cellular_tile_pass::GasCellularTilePass,
    GAS_BUOYANCY_SCALE,
    GAS_SUBSTEP_COUNT,
    GAS_TIME_STEP,
};
use crate::{
    materials::MaterialTable,
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
            CELLULAR_TILE_SIZE,
        },
        SceneError,
    },
};
use engine_host::acceleration::Accelerator;

/// GPU simulation of the gas in a scene's active cellular tiles
pub struct Gases {
    /// GPU simulation of the gas cells
    pass: GasCellularTilePass,
}

impl Gases {

    /// Creates a simulation of the gas in the cellular tile buffers, displaced by the fluid in the
    /// fluid raster and the actors in the cellular tile actor records
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_raster_buffer: &wgpu::Buffer,
        actor_buffer: &wgpu::Buffer,
    ) -> Self {
        Self {
            pass: GasCellularTilePass::new(
                accelerator,
                cellular_tile_buffers,
                material_table,
                fluid_raster_buffer,
                actor_buffer,
            ),
        }
    }

    /// Simulates one tick of the gas in the active tiles without waiting for the GPU, against
    /// phantom air of an ambient amount and mass per cell and gravity in tiles per second squared,
    /// displaced by the first actor records
    pub fn simulate_gases(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        gravity: f32,
        ambient_amount: u32,
        ambient_mass: f32,
        actor_count: u32,
    ) -> Result<(), SceneError> {
        // the shader measures everything in cells and substeps
        let substep_time: f32 = GAS_TIME_STEP / GAS_SUBSTEP_COUNT as f32;
        let gravity: f32 = gravity * GAS_BUOYANCY_SCALE * CELLULAR_TILE_SIZE as f32
            * substep_time
            * substep_time;
        self.pass.simulate_cellular_tiles(
            accelerator,
            active_region,
            ring_offset,
            gravity,
            ambient_amount,
            ambient_mass,
            actor_count,
        )
    }

}
