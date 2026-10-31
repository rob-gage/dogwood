// Copyright Rob Gage 2026

use super::chemistry_cellular_tile_pass::ChemistryCellularTilePass;
use crate::{
    materials::MaterialTable,
    scenes::cellular_tiles::{
        CellularTileBuffers,
        CellularTileRegion,
    },
};
use engine_host::acceleration::Accelerator;

/// GPU simulation of the reactions between the matter of a scene's active cellular tiles: the
/// cells' cellular matter, gas, and fluid particles, in the tiles thermal lists each tick
pub struct Chemistry {
    /// GPU simulation of the cells' reactions
    pass: ChemistryCellularTilePass,
}

impl Chemistry {

    /// Creates a simulation of the reactions in the cellular tile buffers, with fluid particles
    /// seen through the fluid species and heat they deposit and created from the fluid spawns,
    /// over the tiles thermal lists
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_species_buffer: wgpu::BufferBinding<'_>,
        fluid_heat_buffer: &wgpu::Buffer,
        fluid_spawn_buffer: &wgpu::Buffer,
        rigid_body_tile_buffer: &wgpu::Buffer,
        changed_rigid_body_cell_buffer: &wgpu::Buffer,
        thermal_control_buffer: &wgpu::Buffer,
    ) -> Self {
        Self {
            pass: ChemistryCellularTilePass::new(
                accelerator,
                cellular_tile_buffers,
                material_table,
                fluid_species_buffer,
                fluid_heat_buffer,
                fluid_spawn_buffer,
                rigid_body_tile_buffer,
                changed_rigid_body_cell_buffer,
                thermal_control_buffer,
            ),
        }
    }

    /// Prepares one tick of reactions in the tiles thermal lists, which thermal records before
    /// its step, with phantom air at an ambient amount per cell and heat capacity per atmosphere
    pub fn prepare_chemistry(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        ambient_gas_amount: u32,
        ambient_heat_capacity: f32,
    ) {
        self.pass.prepare_cellular_tiles(
            accelerator,
            active_region,
            ring_offset,
            ambient_gas_amount,
            ambient_heat_capacity,
        );
    }

    /// Records the prepared tick's reactions into a compute pass, one workgroup per tile of
    /// thermal's indirect dispatch
    pub fn record_chemistry(
        &self,
        compute_pass: &mut wgpu::ComputePass<'_>,
        thermal_dispatch_buffer: &wgpu::Buffer,
    ) {
        self.pass.record_cellular_tiles(compute_pass, thermal_dispatch_buffer);
    }

}
