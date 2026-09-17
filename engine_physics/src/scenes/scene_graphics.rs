// Copyright Rob Gage 2026

use super::*;

impl Scene {
    /// Returns graphics information for this scene
    pub fn graphics(&self) -> SceneGraphics<'_> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u32 = u32::from(self.simulation_buffer_size) * 2;
        let walking_pawn: Option<([f32; 2], [f32; 2])> = self
            .actor_registry
            .first_walking_pawn_graphics(self.tick_interpolation());
        SceneGraphics {
            material_graphics: self.material_table.graphics(),
            cellular_material_identifiers: &self.cellular_material_identifiers,
            cellular_appearances: &self.cellular_appearances,
            rigid_material_identifiers: self
                .cellular_physics_body_proxy
                .rigid_material_identifiers_buffer(),
            rigid_appearances: self.cellular_physics_body_proxy.rigid_appearances_buffer(),
            fluid_material_identifiers: self.fluids.material_identifiers_buffer(),
            fluid_coverage: self.fluids.coverage_buffer(),
            gas_concentrations: self.gases.concentrations_buffer(),
            gas_count: self.gases.gas_count(),
            cellular_pressure: self.cellular_pressure.retained_pressure(),
            buffered_origin: [self.origin.x - buffer_size, self.origin.y - buffer_size],
            buffered_tile_size: [
                u32::from(self.simulation_width) + dimensions,
                u32::from(self.simulation_height) + dimensions,
            ],
            ring_offset: [
                u32::from(self.tiles_ring_offset_x),
                u32::from(self.tiles_ring_offset_y),
            ],
            walking_pawn,
        }
    }
}
