// Copyright Rob Gage 2026

use crate::simulation::simulation_constants::*;
use crate::simulation::{
    RigidGranularReactionBatch, RigidGranularReadbackSlot, RigidGranularReadbackStatus,
};
use crate::{
    materials::{Material, MaterialIdentifier, MaterialRegistry},
    tiles::{CellCoordinates, TileCoordinates},
};
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};

/// Applies transient directional cellular pressure and static integrity damage
pub struct CellularPressure {
    static_properties: AcceleratorBuffer,
    dynamic_properties: AcceleratorBuffer,
    fluid_properties: AcceleratorBuffer,
    pending_impulses: AcceleratorBuffer,
    pressure_a: AcceleratorBuffer,
    pressure_b: AcceleratorBuffer,
    retained_pressure: AcceleratorBuffer,
    /// Transient logical-tile mask covering current pressure sources and their stencil halo
    active_tiles: AcceleratorBuffer,
    /// Dense logical indices of the currently active pressure tiles
    active_tile_indices: AcceleratorBuffer,
    /// Accelerator-written indirect dispatch record for active pressure tiles
    indirect_dispatch: wgpu::Buffer,
    rigid_contact_statistics: AcceleratorBuffer,
    rigid_reactions: AcceleratorBuffer,
    rigid_predicted_motion: AcceleratorBuffer,
    rigid_damage: AcceleratorBuffer,
    rigid_fractures: AcceleratorBuffer,
    rigid_damage_dispatch: wgpu::Buffer,
    rigid_fracture_count: wgpu::Buffer,
    rigid_reaction_readback_slots: Vec<RigidGranularReadbackSlot>,
    rigid_reaction_completed: BTreeMap<u64, RigidGranularReactionBatch>,
    rigid_reaction_sequence_next: u64,
    rigid_reaction_sequence_apply_next: u64,
    rigid_topology_revision: u64,
    rigid_fracture_word_count: u64,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    bound_buffers: Vec<(u32, wgpu::Buffer)>,
    rigid_body_capacity: usize,
    /// Compaction-only binding for the indirect dispatch record
    indirect_bind_group: wgpu::BindGroup,
    rigid_damage_bind_group: wgpu::BindGroup,
    impulse_pipeline: wgpu::ComputePipeline,
    /// Clears the coarse pressure work mask at the start of each fixed tick
    clear_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Finds current pressure sources and activates every tile their fixed stencil can reach
    mark_active_tiles_pipeline: wgpu::ComputePipeline,
    /// Compacts the coarse mask for indirect cell-stage dispatches
    compact_active_tiles_pipeline: wgpu::ComputePipeline,
    gather_rigid_contacts_pipeline: wgpu::ComputePipeline,
    gather_rigid_static_contacts_pipeline: wgpu::ComputePipeline,
    resolve_rigid_static_contacts_pipeline: wgpu::ComputePipeline,
    resolve_contacts_pipelines: [wgpu::ComputePipeline; 4],
    rigid_contact_initialize_pipeline: wgpu::ComputePipeline,
    propagate_pending_pipeline: wgpu::ComputePipeline,
    propagate_a_pipeline: wgpu::ComputePipeline,
    propagate_b_pipeline: wgpu::ComputePipeline,
    finalize_pipeline: wgpu::ComputePipeline,
    apply_rigid_damage_pipeline: wgpu::ComputePipeline,
    mark_pressure_active_tiles_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    gas_count: u32,
    tick: u32,
}

#[path = "cellular_pressure_construction.rs"]
mod cellular_pressure_construction;
#[path = "cellular_pressure_runtime.rs"]
mod cellular_pressure_runtime;
#[path = "cellular_pressure_simulation.rs"]
mod cellular_pressure_simulation;

impl CellularPressure {
    /// Returns the transient retained-pressure field for viewport visualization
    pub(crate) const fn retained_pressure(&self) -> &AcceleratorBuffer {
        &self.retained_pressure
    }
    /// Local pressure-source accumulator shared with chemistry before the
    /// ordinary pressure propagation stage.
    pub(crate) const fn pending_pressure(&self) -> &AcceleratorBuffer {
        &self.pending_impulses
    }
}

impl Drop for CellularPressure {
    fn drop(&mut self) {
        self.static_properties.free();
        self.dynamic_properties.free();
        self.fluid_properties.free();
        self.pending_impulses.free();
        self.pressure_a.free();
        self.pressure_b.free();
        self.retained_pressure.free();
        self.active_tiles.free();
        self.active_tile_indices.free();
        self.rigid_contact_statistics.free();
        self.rigid_reactions.free();
        self.rigid_predicted_motion.free();
        self.rigid_damage.free();
        self.rigid_fractures.free();
        self.indirect_dispatch.destroy();
        self.rigid_damage_dispatch.destroy();
        self.rigid_fracture_count.destroy();
        self.parameters.destroy();
        for slot in &self.rigid_reaction_readback_slots {
            slot.buffer.destroy();
        }
    }
}
