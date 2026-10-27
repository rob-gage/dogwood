// Copyright Rob Gage 2026

use super::{
    pressure_cellular_tile_pass::PressureCellularTilePass,
    GAS_AMOUNT_PER_ATMOSPHERE,
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
    /// the fluid in the fluid raster and the actors in the cellular tile actor records, rigid body
    /// cells told apart by the rigid body tile records, and static tiles and rigid body cells
    /// changed by damage reported in the collision change lists
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_raster_buffer: &wgpu::Buffer,
        actor_buffer: &wgpu::Buffer,
        rigid_body_cellular_tile_buffer: &wgpu::Buffer,
        changed_cellular_tile_buffer: &wgpu::Buffer,
        changed_rigid_body_cell_buffer: &wgpu::Buffer,
    ) -> Self {
        Self {
            pass: PressureCellularTilePass::new(
                accelerator,
                cellular_tile_buffers,
                material_table,
                fluid_raster_buffer,
                actor_buffer,
                rigid_body_cellular_tile_buffer,
                changed_cellular_tile_buffer,
                changed_rigid_body_cell_buffer,
            ),
        }
    }

    /// Simulates one tick of the pressure in the active tiles without waiting for the GPU, against
    /// phantom air of an ambient amount and mass per cell, temperature, and heat capacity per
    /// atmosphere and gravity in tiles per second squared, displaced by the first actor records
    pub fn simulate_pressure(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        gravity: f32,
        ambient_amount: u32,
        ambient_mass: f32,
        ambient_temperature: f32,
        ambient_heat_capacity: f32,
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
            ambient_temperature,
            ambient_heat_capacity / GAS_AMOUNT_PER_ATMOSPHERE,
            actor_count,
        )
    }

}
