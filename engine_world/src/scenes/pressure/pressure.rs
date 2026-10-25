// Copyright Rob Gage 2026

use super::{
    pressure_cellular_tile_pass::PressureCellularTilePass,
    PRESSURE_BUOYANCY_SCALE,
    PRESSURE_SUBSTEP_COUNT,
    PRESSURE_TIME_STEP,
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

/// GPU simulation of the pressure in a scene's active cellular tiles: the gas and phantom air in
/// open cells and the pressure stored in static cells
pub struct Pressure {
    /// GPU simulation of the pressure cells
    pass: PressureCellularTilePass,
}

impl Pressure {

    /// Creates a simulation of the pressure in the cellular tile buffers, with gas displaced by
    /// the fluid in the fluid raster and the actors in the cellular tile actor records, and
    /// rigid body cells told apart by the rigid body tile records
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_raster_buffer: &wgpu::Buffer,
        actor_buffer: &wgpu::Buffer,
        rigid_body_cellular_tile_buffer: &wgpu::Buffer,
    ) -> Self {
        Self {
            pass: PressureCellularTilePass::new(
                accelerator,
                cellular_tile_buffers,
                material_table,
                fluid_raster_buffer,
                actor_buffer,
                rigid_body_cellular_tile_buffer,
            ),
        }
    }

    /// Simulates one tick of the pressure in the active tiles without waiting for the GPU, against
    /// phantom air of an ambient amount and mass per cell and gravity in tiles per second squared,
    /// displaced by the first actor records
    pub fn simulate_pressure(
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
        let substep_time: f32 = PRESSURE_TIME_STEP / PRESSURE_SUBSTEP_COUNT as f32;
        let gravity: f32 = gravity * PRESSURE_BUOYANCY_SCALE * CELLULAR_TILE_SIZE as f32
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

    /// Returns the count of the tiles whose cellular tile cells broke in the last tick, then each
    /// tile's buffer index and remaining static cells as two `u32` masks
    pub fn broken_cellular_tile_buffer(&self) -> &wgpu::Buffer {
        self.pass.broken_cellular_tile_buffer()
    }

    /// Returns the count of the rigid body cells that broke in the last tick, then each one's
    /// rigid body tile index times 64 plus its cell
    pub fn broken_rigid_body_cell_buffer(&self) -> &wgpu::Buffer {
        self.pass.broken_rigid_body_cell_buffer()
    }

}
