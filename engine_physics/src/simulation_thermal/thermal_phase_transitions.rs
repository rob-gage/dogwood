use std::sync::mpsc::Receiver;
use std::sync::mpsc::sync_channel;

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use crate::simulation::simulation_constants::RIGID_PHASE_CANDIDATE_SIZE;
use crate::simulation::simulation_constants::RIGID_PHASE_CANDIDATES_OFFSET;

pub(crate) fn rigid_phase_readback_len(rigid_count: u32) -> u64 {
    RIGID_PHASE_CANDIDATES_OFFSET + u64::from(rigid_count) * RIGID_PHASE_CANDIDATE_SIZE
}

/// Evaluates declarative phase metadata after scatter; mutation application remains shared.
pub(crate) struct ThermalPhaseTransitions {
    thermal_phase_transition_parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    cells: wgpu::ComputePipeline,
    particles: wgpu::ComputePipeline,
    gases: wgpu::ComputePipeline,
    rigid: wgpu::ComputePipeline,
    rollback: wgpu::ComputePipeline,
    rigid_phase_candidates: AcceleratorBuffer,
    rigid_phase_count: AcceleratorBuffer,
    rigid_phase_readback: wgpu::Buffer,
    rigid_phase_readback_len: u64,
    rigid_phase_readback_capacity: u32,
    rigid_phase_readback_result: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
    rollback_slots: AcceleratorBuffer,
    rollback_count: AcceleratorBuffer,
    cell_count: u32,
    particle_count: u32,
    gas_count: u32,
    tick: u32,
}
impl ThermalPhaseTransitions {
    pub(crate) fn encode(
        &mut self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        rigid_count: u32,
    ) {
        if self.rigid_phase_readback_result.is_none() {
            self.clear_rigid_candidates(accelerator);
        }
        self.write_parameters(accelerator, origin, tiles, ring, rigid_count);
        let mut thermal_phase_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "thermal phase transitions");
        thermal_phase_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        thermal_phase_compute_pass.set_pipeline(&self.cells);
        thermal_phase_compute_pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        thermal_phase_compute_pass.set_pipeline(&self.particles);
        thermal_phase_compute_pass.dispatch_workgroups(self.particle_count.div_ceil(64), 1, 1);
        thermal_phase_compute_pass.set_pipeline(&self.gases);
        thermal_phase_compute_pass.dispatch_workgroups(self.cell_count / 64, self.gas_count, 2);
        if self.rigid_phase_readback_result.is_none() {
            thermal_phase_compute_pass.set_pipeline(&self.rigid);
            if rigid_count > 0 {
                thermal_phase_compute_pass.dispatch_workgroups(rigid_count.div_ceil(64), 1, 1);
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        accelerator: &Accelerator,
        cells: &AcceleratorBuffer,
        amounts: &AcceleratorBuffer,
        temperatures: &AcceleratorBuffer,
        particles: &AcceleratorBuffer,
        concentrations: &AcceleratorBuffer,
        gas_temperatures: &AcceleratorBuffer,
        properties: &AcceleratorBuffer,
        thermal: &wgpu::Buffer,
        requests: &AcceleratorBuffer,
        request_count: &AcceleratorBuffer,
        gas_fluid_candidates: &AcceleratorBuffer,
        cell_count: u32,
        particle_count: u32,
        gas_count: u32,
        rigid_claims: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        rigid_amounts: &AcceleratorBuffer,
        rigid_temperatures: &AcceleratorBuffer,
        fluid_free_indices: &AcceleratorBuffer,
        fluid_free_count: &AcceleratorBuffer,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let thermal_phase_transition_parameters: wgpu::Buffer =
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("thermal phase parameters"),
                size: 48,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        let storage: fn(u32, bool) -> wgpu::BindGroupLayoutEntry =
            crate::simulation::storage_bind_group_layout_entry;
        let mut thermal_phase_transition_bind_group_layout_entries: Vec<
            wgpu::BindGroupLayoutEntry,
        > = (0..10)
            .filter(|binding| *binding != 7)
            .map(|binding| storage(binding, !matches!(binding, 8 | 9)))
            .collect();
        thermal_phase_transition_bind_group_layout_entries
            .push(crate::simulation::uniform_bind_group_layout_entry(7));
        thermal_phase_transition_bind_group_layout_entries.push(storage(11, false));
        thermal_phase_transition_bind_group_layout_entries.push(storage(12, false));
        thermal_phase_transition_bind_group_layout_entries.push(storage(13, true));
        thermal_phase_transition_bind_group_layout_entries.push(storage(14, true));
        thermal_phase_transition_bind_group_layout_entries.push(storage(15, true));
        thermal_phase_transition_bind_group_layout_entries.push(storage(16, false));
        thermal_phase_transition_bind_group_layout_entries.push(storage(17, false));
        thermal_phase_transition_bind_group_layout_entries.push(storage(18, false));
        thermal_phase_transition_bind_group_layout_entries.push(storage(19, false));
        thermal_phase_transition_bind_group_layout_entries.push(storage(20, true));
        thermal_phase_transition_bind_group_layout_entries.push(storage(21, true));
        thermal_phase_transition_bind_group_layout_entries
            .push(crate::simulation::uniform_bind_group_layout_entry(10));
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("thermal phase"),
                entries: &thermal_phase_transition_bind_group_layout_entries,
            });
        let buffers: [&AcceleratorBuffer; 7] = [
            cells,
            amounts,
            temperatures,
            particles,
            concentrations,
            gas_temperatures,
            properties,
        ];
        let mut bind_group_entries: Vec<wgpu::BindGroupEntry<'_>> = buffers
            .iter()
            .enumerate()
            .map(|(binding_index, buffer)| wgpu::BindGroupEntry {
                binding: binding_index as u32,
                resource: buffer.wgpu_buffer().as_entire_binding(),
            })
            .collect();
        bind_group_entries.push(wgpu::BindGroupEntry {
            binding: 7,
            resource: thermal.as_entire_binding(),
        });
        bind_group_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            8, requests,
        ));
        bind_group_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            9,
            request_count,
        ));
        bind_group_entries.push(wgpu::BindGroupEntry {
            binding: 10,
            resource: thermal_phase_transition_parameters.as_entire_binding(),
        });
        bind_group_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            11,
            rigid_claims,
        ));
        bind_group_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            12,
            gas_fluid_candidates,
        ));
        let rigid_phase_candidates: AcceleratorBuffer =
            accelerator.allocate::<[u32; 10]>(cell_count as usize);
        let rigid_phase_count: AcceleratorBuffer = accelerator.allocate::<u32>(1);
        let rigid_phase_readback: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rigid thermal phase readback"),
            size: 256 + cell_count as u64 * 40,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let rollback_slots: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count as usize);
        let rollback_count: AcceleratorBuffer = accelerator.allocate::<u32>(1);
        accelerator.wgpu_queue().write_buffer(
            rigid_phase_count.wgpu_buffer(),
            0,
            &0u32.to_le_bytes(),
        );
        for (binding, buffer) in [
            (13, rigid_cells),
            (14, rigid_amounts),
            (15, rigid_temperatures),
            (16, &rigid_phase_candidates),
            (17, &rigid_phase_count),
        ] {
            bind_group_entries.push(wgpu::BindGroupEntry {
                binding,
                resource: buffer.wgpu_buffer().as_entire_binding(),
            });
        }
        bind_group_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            18,
            fluid_free_indices,
        ));
        bind_group_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            19,
            fluid_free_count,
        ));
        bind_group_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            20,
            &rollback_slots,
        ));
        bind_group_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            21,
            &rollback_count,
        ));
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal phase"),
            layout: &layout,
            entries: &bind_group_entries,
        });
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "thermal phase shader",
            concat!(
                include_str!("thermal_phase_transitions_shader_helpers.wgsl"),
                include_str!("thermal_phase_transitions_shader_entry_points.wgsl"),
            ),
            "engine_physics/src/simulation_thermal/thermal_phase_transitions.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("thermal phase"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let create_compute_pipeline: &dyn Fn(&'static str) -> wgpu::ComputePipeline =
            &|entry: &'static str| -> wgpu::ComputePipeline {
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
            };
        Self {
            thermal_phase_transition_parameters,
            bind_group,
            cells: create_compute_pipeline("phase_cells"),
            particles: create_compute_pipeline("phase_particles"),
            gases: create_compute_pipeline("phase_gases"),
            rigid: create_compute_pipeline("phase_rigid"),
            rollback: create_compute_pipeline("rollback_rigid_reservations"),
            rigid_phase_candidates,
            rigid_phase_count,
            rigid_phase_readback,
            rigid_phase_readback_len: 0,
            rigid_phase_readback_capacity: 0,
            rigid_phase_readback_result: None,
            rollback_slots,
            rollback_count,
            cell_count,
            particle_count,
            gas_count,
            tick: 0,
        }
    }
    pub(crate) fn clear_rigid_candidates(&self, accelerator: &Accelerator) {
        accelerator.wgpu_queue().write_buffer(
            self.rigid_phase_count.wgpu_buffer(),
            0,
            &0u32.to_le_bytes(),
        );
    }
    pub(crate) const fn rigid_phase_candidates_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_phase_candidates
    }
    pub(crate) const fn rigid_phase_count_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_phase_count
    }
    pub(crate) fn rigid_readback_pending(&self) -> bool {
        self.rigid_phase_readback_result.is_some()
    }
    pub(crate) fn submit_rigid_readback(&mut self, accelerator: &Accelerator, rigid_count: u32) {
        if self.rigid_phase_readback_result.is_some() {
            return;
        }
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid thermal phase readback"),
                });
        encoder.copy_buffer_to_buffer(
            self.rigid_phase_count.wgpu_buffer(),
            0,
            &self.rigid_phase_readback,
            0,
            4,
        );
        self.rigid_phase_readback_capacity = rigid_count.min(self.cell_count);
        let rigid_count: u32 = self.rigid_phase_readback_capacity;
        let readback_len: u64 = rigid_phase_readback_len(rigid_count);
        if rigid_count > 0 {
            encoder.copy_buffer_to_buffer(
                self.rigid_phase_candidates.wgpu_buffer(),
                0,
                &self.rigid_phase_readback,
                RIGID_PHASE_CANDIDATES_OFFSET,
                u64::from(rigid_count) * RIGID_PHASE_CANDIDATE_SIZE,
            );
        }
        self.rigid_phase_readback_len = readback_len;
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = sync_channel(1);
        self.rigid_phase_readback.slice(0..readback_len).map_async(
            wgpu::MapMode::Read,
            move |readback_result| {
                sender.send(readback_result).ok();
            },
        );
        self.rigid_phase_readback_result = Some(receiver);
    }
    pub(crate) fn take_rigid_candidates(&mut self) -> Option<Vec<[u32; 10]>> {
        let rigid_phase_readback_result: Result<(), wgpu::BufferAsyncError> =
            self.rigid_phase_readback_result.as_ref()?.try_recv().ok()?;
        self.rigid_phase_readback_result = None;
        if rigid_phase_readback_result.is_err() {
            self.rigid_phase_readback.unmap();
            return Some(Vec::new());
        }
        let rigid_phase_readback_bytes: wgpu::BufferView = self
            .rigid_phase_readback
            .slice(0..self.rigid_phase_readback_len)
            .get_mapped_range()
            .ok()?;
        let rigid_phase_candidate_count: usize =
            u32::from_le_bytes(rigid_phase_readback_bytes[..4].try_into().ok()?)
                .min(self.rigid_phase_readback_capacity) as usize;
        if rigid_phase_candidate_count == 0 {
            drop(rigid_phase_readback_bytes);
            self.rigid_phase_readback.unmap();
            return Some(Vec::new());
        }
        let rigid_phase_candidate_records: Vec<[u32; 10]> = rigid_phase_readback_bytes
            [usize::try_from(RIGID_PHASE_CANDIDATES_OFFSET).unwrap()..]
            .as_chunks::<40>()
            .0
            .iter()
            .take(rigid_phase_candidate_count)
            .map(|record_bytes| {
                let mut rigid_phase_candidate_record: [u32; 10] = [0u32; 10];
                for (record_word, record_word_bytes) in rigid_phase_candidate_record
                    .iter_mut()
                    .zip(record_bytes.as_chunks::<4>().0)
                {
                    *record_word = u32::from_le_bytes(*record_word_bytes);
                }
                rigid_phase_candidate_record
            })
            .collect();
        drop(rigid_phase_readback_bytes);
        self.rigid_phase_readback.unmap();
        Some(rigid_phase_candidate_records)
    }
    pub(crate) fn rollback_rigid_reservations(&self, accelerator: &Accelerator, slots: &[u32]) {
        if slots.is_empty() {
            return;
        }
        let rigid_thermal_rollback_slot_count: u32 =
            slots.len().min(self.cell_count as usize) as u32;
        accelerator.wgpu_queue().write_buffer(
            self.rollback_slots.wgpu_buffer(),
            0,
            &slots[..rigid_thermal_rollback_slot_count as usize]
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        accelerator.wgpu_queue().write_buffer(
            self.rollback_count.wgpu_buffer(),
            0,
            &rigid_thermal_rollback_slot_count.to_le_bytes(),
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rollback rigid thermal reservations"),
                });
        let mut thermal_phase_rollback_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(&mut encoder, "rollback rigid thermal reservations");
        thermal_phase_rollback_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        thermal_phase_rollback_compute_pass.set_pipeline(&self.rollback);
        thermal_phase_rollback_compute_pass.dispatch_workgroups(
            rigid_thermal_rollback_slot_count.div_ceil(64),
            1,
            1,
        );
        drop(thermal_phase_rollback_compute_pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
    pub(crate) fn evaluate(
        &mut self,
        accelerator: &Accelerator,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        rigid_count: u32,
    ) {
        if self.rigid_phase_readback_result.is_none() {
            self.clear_rigid_candidates(accelerator);
        }
        self.write_parameters(accelerator, origin, tiles, ring, rigid_count);
        let mut thermal_command_encoder: wgpu::CommandEncoder = accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("thermal phase transitions"),
            });
        let mut thermal_compute_pass: wgpu::ComputePass<'_> = accelerator
            .begin_compute_pass(&mut thermal_command_encoder, "thermal phase transitions");
        thermal_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        thermal_compute_pass.set_pipeline(&self.cells);
        thermal_compute_pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        thermal_compute_pass.set_pipeline(&self.particles);
        thermal_compute_pass.dispatch_workgroups(self.particle_count.div_ceil(64), 1, 1);
        thermal_compute_pass.set_pipeline(&self.gases);
        thermal_compute_pass.dispatch_workgroups(self.cell_count / 64, self.gas_count, 2);
        if self.rigid_phase_readback_result.is_none() {
            thermal_compute_pass.set_pipeline(&self.rigid);
            if rigid_count > 0 {
                thermal_compute_pass.dispatch_workgroups(rigid_count.div_ceil(64), 1, 1);
            }
        }
        drop(thermal_compute_pass);
        accelerator
            .wgpu_queue()
            .submit(Some(thermal_command_encoder.finish()));
    }

    fn write_parameters(
        &mut self,
        accelerator: &Accelerator,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        rigid_count: u32,
    ) {
        let parameter_values: [u32; 12] = [
            origin[0] as u32,
            origin[1] as u32,
            tiles[0],
            tiles[1],
            ring[0],
            ring[1],
            self.cell_count,
            self.particle_count,
            self.gas_count,
            self.tick,
            rigid_count,
            0,
        ];
        accelerator.wgpu_queue().write_buffer(
            &self.thermal_phase_transition_parameters,
            0,
            &parameter_values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        self.tick = self.tick.wrapping_add(1);
    }
}
impl Drop for ThermalPhaseTransitions {
    fn drop(&mut self) {
        self.thermal_phase_transition_parameters.destroy();
        self.rigid_phase_candidates.free();
        self.rigid_phase_count.free();
        self.rigid_phase_readback.destroy();
        self.rollback_slots.free();
        self.rollback_count.free();
    }
}
