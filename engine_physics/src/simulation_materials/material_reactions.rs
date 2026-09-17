// Copyright Rob Gage 2026

//! Shared deterministic reaction eligibility and arbitration rules. Accelerator discovery
//! emits the same compact candidate shape; keeping arbitration here makes the
//! ordering contract explicit and independently testable.

use crate::simulation::simulation_constants::*;
use crate::{materials::MaterialTable, simulation_fluids::FluidAuthorityView};
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::sync::mpsc::{Receiver, sync_channel};

/// Immutable-snapshot Accelerator reaction discovery. Application is intentionally a
/// separate stage so no product becomes an input until the next chemistry tick.
pub(crate) struct MaterialReactions {
    candidates: AcceleratorBuffer,
    candidate_indices: AcceleratorBuffer,
    candidate_count: AcceleratorBuffer,
    sort_steps: AcceleratorBuffer,
    sort_indirect: wgpu::Buffer,
    sort_parameters: wgpu::Buffer,
    fluid_reservations: AcceleratorBuffer,
    gas_reservations: AcceleratorBuffer,
    gas_output_reservations: AcceleratorBuffer,
    canonical_reservations: AcceleratorBuffer,
    rigid_reservations: AcceleratorBuffer,
    rigid_removal_events: AcceleratorBuffer,
    rigid_removal_count: AcceleratorBuffer,
    rigid_removal_readback: wgpu::Buffer,
    rigid_removal_readback_len: u64,
    rigid_removal_readback_capacity: u32,
    rigid_removal_readback_result: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
    fluid_reservation_owners: AcceleratorBuffer,
    reaction_energy: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    prepare_sort_bind_group: wgpu::BindGroup,
    sort_bind_group: wgpu::BindGroup,
    clear_pipeline: wgpu::ComputePipeline,
    discover_pipeline: wgpu::ComputePipeline,
    compact_pipeline: wgpu::ComputePipeline,
    prepare_sort_pipeline: wgpu::ComputePipeline,
    sort_pipeline: wgpu::ComputePipeline,
    reserve_pipeline: wgpu::ComputePipeline,
    apply_pipeline: wgpu::ComputePipeline,
    cell_count: u32,
    clear_count: u32,
    sort_capacity: u32,
    sort_step_count: usize,
    reaction_count: u32,
}

#[path = "material_reactions_construction.rs"]
mod material_reactions_construction;
#[path = "material_reactions_dispatch.rs"]
mod material_reactions_dispatch;
#[path = "material_reactions_readback.rs"]
mod material_reactions_readback;

impl MaterialReactions {
    pub(crate) const fn reaction_energy_buffer(&self) -> &AcceleratorBuffer {
        &self.reaction_energy
    }
}

impl Drop for MaterialReactions {
    fn drop(&mut self) {
        self.candidates.free();
        self.candidate_indices.free();
        self.candidate_count.free();
        self.sort_steps.free();
        self.sort_indirect.destroy();
        self.sort_parameters.destroy();
        self.rigid_reservations.free();
        self.rigid_removal_events.free();
        self.rigid_removal_count.free();
        self.rigid_removal_readback.destroy();
    }
}
