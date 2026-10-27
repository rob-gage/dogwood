// Copyright Rob Gage 2026

use super::thermal_cellular_tile_pass::ThermalCellularTilePass;
use crate::{
    materials::MaterialTable,
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
        },
        pressure::GAS_AMOUNT_PER_ATMOSPHERE,
    },
};
use engine_host::acceleration::Accelerator;

/// GPU simulation of the heat in a scene's active cellular tiles: conduction between the cells'
/// cellular matter, gas, and fluid particles, and the relaxation of phantom air toward the
/// scene's ambient temperature
pub struct Thermal {
    /// GPU simulation of the cells' heat
    pass: ThermalCellularTilePass,
}

impl Thermal {

    /// Creates a simulation of the heat in the cellular tile buffers, with fluid particles' heat
    /// read from the fluid heat they deposit
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_heat_buffer: &wgpu::Buffer,
        rigid_body_tile_buffer: &wgpu::Buffer,
        changed_cellular_tile_buffer: &wgpu::Buffer,
        changed_rigid_body_cell_buffer: &wgpu::Buffer,
    ) -> Self {
        Self {
            pass: ThermalCellularTilePass::new(
                accelerator,
                cellular_tile_buffers,
                material_table,
                fluid_heat_buffer,
                rigid_body_tile_buffer,
                changed_cellular_tile_buffer,
                changed_rigid_body_cell_buffer,
            ),
        }
    }

    /// Returns the cells and gases queued for conversion into fluid particles
    pub fn conversion_buffer(&self) -> &wgpu::Buffer { self.pass.conversion_buffer() }

    /// Returns whether recent completed thermal samples contain fluid conversions
    pub fn has_fluid_conversions(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<bool, crate::scenes::SceneError> {
        self.pass.has_fluid_conversions(accelerator)
    }

    /// Simulates one tick of the heat in the active tiles without waiting for the GPU, with
    /// phantom air of an ambient temperature, heat capacity per atmosphere, and conduction
    pub fn simulate_thermal(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        ambient_temperature: f32,
        ambient_heat_capacity: f32,
        ambient_conduction: f32,
    ) {
        // the shader weighs phantom air by its amount units
        self.pass.simulate_cellular_tiles(
            accelerator,
            active_region,
            ring_offset,
            ambient_temperature,
            ambient_heat_capacity / GAS_AMOUNT_PER_ATMOSPHERE,
            ambient_conduction,
        );
    }

}
