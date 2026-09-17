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

impl MaterialReactions {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(&self, accelerator: &Accelerator, encoder: &mut wgpu::CommandEncoder) {
        if self.reaction_count == 0 {
            return;
        }
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry clear");
        pass.set_pipeline(&self.clear_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.clear_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry discovery");
        pass.set_pipeline(&self.discover_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry candidate compaction");
        pass.set_pipeline(&self.compact_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass =
            accelerator.begin_compute_pass(encoder, "chemistry sort dispatch preparation");
        pass.set_pipeline(&self.prepare_sort_pipeline);
        pass.set_bind_group(0, &self.prepare_sort_bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
        drop(pass);
        for step in 0..self.sort_step_count {
            encoder.copy_buffer_to_buffer(
                self.sort_steps.wgpu_buffer(),
                step as u64 * 8,
                &self.sort_parameters,
                0,
                8,
            );
            let mut pass = accelerator.begin_compute_pass(encoder, "chemistry candidate sort");
            pass.set_pipeline(&self.sort_pipeline);
            pass.set_bind_group(0, &self.sort_bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.sort_indirect, 0);
            drop(pass);
        }
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry arbitration");
        pass.set_pipeline(&self.reserve_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        // One invocation performs the ordered arbitration. It is intentionally
        // serialized: reservation order is a correctness rule, not a race.
        pass.dispatch_workgroups(1, 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry apply");
        pass.set_pipeline(&self.apply_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
    }
    pub(crate) const fn reaction_energy_buffer(&self) -> &AcceleratorBuffer {
        &self.reaction_energy
    }
    pub(super) fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }
    pub(crate) fn submit_rigid_removal_readback(&mut self, accelerator: &Accelerator) {
        if self.rigid_removal_readback_result.is_some() {
            return;
        }
        let len = RIGID_REMOVAL_EVENTS_OFFSET;
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid chemistry removal readback"),
                });
        encoder.copy_buffer_to_buffer(
            self.rigid_removal_count.wgpu_buffer(),
            0,
            &self.rigid_removal_readback,
            0,
            4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        self.rigid_removal_readback_len = len;
        self.rigid_removal_readback_capacity = 0;
        let (sender, receiver) = sync_channel(1);
        self.rigid_removal_readback
            .slice(0..len)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.rigid_removal_readback_result = Some(receiver);
    }
    pub(crate) fn take_rigid_removal_events(
        &mut self,
        accelerator: &Accelerator,
    ) -> Option<Vec<[u32; 6]>> {
        let result = self
            .rigid_removal_readback_result
            .as_ref()?
            .try_recv()
            .ok()?;
        self.rigid_removal_readback_result = None;
        if result.is_err() {
            self.rigid_removal_readback.unmap();
            return Some(Vec::new());
        }
        let bytes = self
            .rigid_removal_readback
            .slice(0..self.rigid_removal_readback_len)
            .get_mapped_range()
            .ok()?;
        let count = u32::from_le_bytes(bytes[..4].try_into().ok()?).min(self.cell_count) as usize;
        if self.rigid_removal_readback_capacity == 0 {
            drop(bytes);
            self.rigid_removal_readback.unmap();
            if count == 0 {
                return Some(Vec::new());
            }
            let len = RIGID_REMOVAL_EVENTS_OFFSET + count as u64 * RIGID_REMOVAL_EVENT_SIZE;
            let mut encoder =
                accelerator
                    .wgpu_device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("rigid chemistry event readback"),
                    });
            encoder.copy_buffer_to_buffer(
                self.rigid_removal_events.wgpu_buffer(),
                0,
                &self.rigid_removal_readback,
                RIGID_REMOVAL_EVENTS_OFFSET,
                count as u64 * RIGID_REMOVAL_EVENT_SIZE,
            );
            accelerator.wgpu_queue().submit(Some(encoder.finish()));
            self.rigid_removal_readback_len = len;
            self.rigid_removal_readback_capacity = count as u32;
            let (sender, receiver) = sync_channel(1);
            self.rigid_removal_readback.slice(0..len).map_async(
                wgpu::MapMode::Read,
                move |result| {
                    let _ = sender.send(result);
                },
            );
            self.rigid_removal_readback_result = Some(receiver);
            return None;
        }
        let events = bytes[usize::try_from(RIGID_REMOVAL_EVENTS_OFFSET).unwrap()..]
            .as_chunks::<32>()
            .0
            .iter()
            .take(count)
            .map(|b| {
                let mut event = [0; 6];
                for (word, value) in event.iter_mut().zip(b.as_chunks::<4>().0) {
                    *word = u32::from_le_bytes(*value);
                }
                event
            })
            .collect();
        drop(bytes);
        self.rigid_removal_readback.unmap();
        Some(events)
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
