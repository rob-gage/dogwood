// Copyright Rob Gage 2026

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use crate::materials::MaterialRegistry;

/// Immutable per-cell thermal contribution snapshot for later conduction passes.
pub(crate) struct ThermalInteraction {
    interaction: AcceleratorBuffer,
    rigid_raster_claim_counts: AcceleratorBuffer,
    thermal_interaction_parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    clear_pipeline: wgpu::ComputePipeline,
    count_pipeline: wgpu::ComputePipeline,
    gather_pipeline: wgpu::ComputePipeline,
    cell_count: u32,
    rigid_capacity: u32,
}

impl ThermalInteraction {
    pub(crate) fn encode(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        buffered_origin: [i32; 2],
        buffered_tiles: [u32; 2],
        ring_offset: [u32; 2],
        has_rigid: bool,
    ) {
        let parameter_values: [u32; 6] = [
            buffered_origin[0] as u32,
            buffered_origin[1] as u32,
            buffered_tiles[0],
            buffered_tiles[1],
            ring_offset[0],
            ring_offset[1],
        ];
        accelerator.wgpu_queue().write_buffer(
            &self.thermal_interaction_parameters,
            32,
            &parameter_values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut thermal_interaction_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "thermal interaction");
        thermal_interaction_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        if has_rigid {
            thermal_interaction_compute_pass.set_pipeline(&self.clear_pipeline);
            thermal_interaction_compute_pass.dispatch_workgroups(
                self.rigid_capacity.div_ceil(64),
                1,
                1,
            );
            thermal_interaction_compute_pass.set_pipeline(&self.count_pipeline);
            thermal_interaction_compute_pass.dispatch_workgroups(
                self.cell_count.div_ceil(64),
                1,
                1,
            );
        }
        thermal_interaction_compute_pass.set_pipeline(&self.gather_pipeline);
        thermal_interaction_compute_pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
    }
    pub(crate) fn new(
        accelerator: &Accelerator,
        _materials: &MaterialRegistry,
        cellular_materials: &AcceleratorBuffer,
        cellular_amounts: &AcceleratorBuffer,
        cellular_temperatures: &AcceleratorBuffer,
        rigid_claims: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        rigid_amounts: &AcceleratorBuffer,
        rigid_temperatures: &AcceleratorBuffer,
        fluid_thermal: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
        reaction_energy: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        gas_temperatures: &AcceleratorBuffer,
        thermal_properties: &AcceleratorBuffer,
        thermal_parameters: &wgpu::Buffer,
        external_occupancy: &AcceleratorBuffer,
        rigid_capacity: u32,
        cell_count: u32,
        gas_count: u32,
        ambient_temperature: f32,
        empty_space_conductivity: f32,
        empty_space_capacity: f32,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let interaction: AcceleratorBuffer = accelerator.allocate::<[f32; 4]>(cell_count as usize);
        let rigid_raster_claim_counts: AcceleratorBuffer =
            accelerator.allocate::<u32>(rigid_capacity as usize);
        let thermal_interaction_parameters: wgpu::Buffer =
            crate::simulation::create_simulation_uniform_buffer(
                device,
                "thermal interaction parameters",
                64,
            );
        let parameter_values: [u32; 7] = [
            ambient_temperature.to_bits(),
            empty_space_conductivity.to_bits(),
            empty_space_capacity.to_bits(),
            cell_count,
            gas_count,
            0,
            0,
        ];
        accelerator.wgpu_queue().write_buffer(
            &thermal_interaction_parameters,
            0,
            &parameter_values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let storage: fn(u32, bool) -> wgpu::BindGroupLayoutEntry =
            crate::simulation::storage_bind_group_layout_entry;
        let mut thermal_interaction_bind_group_layout_entries: Vec<wgpu::BindGroupLayoutEntry> =
            (0u32..14)
                .map(|binding: u32| storage(binding, binding != 13))
                .collect();
        thermal_interaction_bind_group_layout_entries[12] = storage(12, true);
        thermal_interaction_bind_group_layout_entries[13] = storage(13, false);
        thermal_interaction_bind_group_layout_entries
            .push(crate::simulation::uniform_bind_group_layout_entry(14));
        thermal_interaction_bind_group_layout_entries
            .push(crate::simulation::uniform_bind_group_layout_entry(15));
        thermal_interaction_bind_group_layout_entries.push(storage(16, false));
        thermal_interaction_bind_group_layout_entries.push(storage(17, false));
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("thermal interaction"),
                entries: &thermal_interaction_bind_group_layout_entries,
            });
        let buffers: [&AcceleratorBuffer; 14] = [
            cellular_materials,
            cellular_amounts,
            cellular_temperatures,
            rigid_claims,
            rigid_cells,
            rigid_amounts,
            rigid_temperatures,
            fluid_thermal,
            gas_concentrations,
            gas_temperatures,
            thermal_properties,
            external_occupancy,
            fluid_coverage,
            &interaction,
        ];
        let mut bind_entries: Vec<wgpu::BindGroupEntry<'_>> = buffers
            .iter()
            .enumerate()
            .map(|(index, buffer)| wgpu::BindGroupEntry {
                binding: index as u32,
                resource: buffer.wgpu_buffer().as_entire_binding(),
            })
            .collect();
        bind_entries.push(wgpu::BindGroupEntry {
            binding: 14,
            resource: thermal_interaction_parameters.as_entire_binding(),
        });
        bind_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            17,
            reaction_energy,
        ));
        bind_entries.push(crate::simulation::accelerator_buffer_bind_group_entry(
            16,
            &rigid_raster_claim_counts,
        ));
        bind_entries.push(wgpu::BindGroupEntry {
            binding: 15,
            resource: thermal_parameters.as_entire_binding(),
        });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal interaction"),
            layout: &layout,
            entries: &bind_entries,
        });
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "thermal interaction shader",
            include_str!("thermal_interaction.wgsl"),
            "engine_physics/src/simulation_thermal/thermal_interaction.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("thermal interaction"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let pipeline: &dyn Fn(&'static str) -> wgpu::ComputePipeline =
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
            interaction,
            rigid_raster_claim_counts,
            thermal_interaction_parameters,
            bind_group,
            clear_pipeline: pipeline("clear_rigid_claim_counts"),
            count_pipeline: pipeline("count_rigid_claims"),
            gather_pipeline: pipeline("gather_thermal_interaction"),
            cell_count,
            rigid_capacity,
        }
    }

    pub(crate) fn gather(
        &self,
        accelerator: &Accelerator,
        buffered_origin: [i32; 2],
        buffered_tiles: [u32; 2],
        ring_offset: [u32; 2],
        has_rigid: bool,
    ) {
        let parameter_values: [u32; 6] = [
            buffered_origin[0] as u32,
            buffered_origin[1] as u32,
            buffered_tiles[0],
            buffered_tiles[1],
            ring_offset[0],
            ring_offset[1],
        ];
        accelerator.wgpu_queue().write_buffer(
            &self.thermal_interaction_parameters,
            32,
            &parameter_values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("thermal interaction"),
                });
        let mut thermal_interaction_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(&mut encoder, "gather thermal interaction");
        thermal_interaction_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        if has_rigid {
            thermal_interaction_compute_pass.set_pipeline(&self.clear_pipeline);
            thermal_interaction_compute_pass.dispatch_workgroups(
                self.rigid_capacity.div_ceil(64),
                1,
                1,
            );
            thermal_interaction_compute_pass.set_pipeline(&self.count_pipeline);
            thermal_interaction_compute_pass.dispatch_workgroups(
                self.cell_count.div_ceil(64),
                1,
                1,
            );
        }
        thermal_interaction_compute_pass.set_pipeline(&self.gather_pipeline);
        thermal_interaction_compute_pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(thermal_interaction_compute_pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
    pub(crate) const fn interaction_buffer(&self) -> &AcceleratorBuffer {
        &self.interaction
    }
    pub(crate) const fn rigid_raster_claim_counts_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_raster_claim_counts
    }
}

impl Drop for ThermalInteraction {
    fn drop(&mut self) {
        self.interaction.free();
        self.rigid_raster_claim_counts.free();
        self.thermal_interaction_parameters.destroy();
    }
}
