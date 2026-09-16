use crate::materials::MaterialRegistry;
use engine_compute::{Accelerator, AcceleratorBuffer};

/// Immutable per-cell thermal contribution snapshot for later conduction passes.
pub(crate) struct ThermalInteraction {
    interaction: AcceleratorBuffer,
    rigid_raster_claim_counts: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    clear_pipeline: wgpu::ComputePipeline,
    count_pipeline: wgpu::ComputePipeline,
    gather_pipeline: wgpu::ComputePipeline,
    cell_count: u32,
    rigid_capacity: u32,
}

impl ThermalInteraction {
    pub(crate) fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        cellular_materials: &AcceleratorBuffer,
        cellular_amounts: &AcceleratorBuffer,
        cellular_temperatures: &AcceleratorBuffer,
        rigid_claims: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        rigid_amounts: &AcceleratorBuffer,
        rigid_temperatures: &AcceleratorBuffer,
        fluid_thermal: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
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
        let device = accelerator.wgpu_device();
        let interaction = accelerator.allocate::<[f32; 4]>(cell_count as usize);
        let rigid_raster_claim_counts = accelerator.allocate::<u32>(rigid_capacity as usize);
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thermal interaction parameters"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let values = [
            ambient_temperature.to_bits(),
            empty_space_conductivity.to_bits(),
            empty_space_capacity.to_bits(),
            cell_count,
            gas_count,
            0,
            0,
        ];
        accelerator.wgpu_queue().write_buffer(
            &parameters,
            0,
            &values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let mut entries: Vec<_> = (0..14).map(|b| storage(b, b != 13)).collect();
        entries[12] = storage(12, true);
        entries[13] = storage(13, false);
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 14,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 15,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        entries.push(storage(16, false));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thermal interaction"),
            entries: &entries,
        });
        let buffers = [
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
        let mut bind_entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.wgpu_buffer().as_entire_binding(),
            })
            .collect();
        bind_entries.push(wgpu::BindGroupEntry {
            binding: 14,
            resource: parameters.as_entire_binding(),
        });
        bind_entries.push(wgpu::BindGroupEntry {
            binding: 16,
            resource: rigid_raster_claim_counts.wgpu_buffer().as_entire_binding(),
        });
        bind_entries.push(wgpu::BindGroupEntry {
            binding: 15,
            resource: thermal_parameters.as_entire_binding(),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal interaction"),
            layout: &layout,
            entries: &bind_entries,
        });
        let shader = super::create_simulation_shader_module(
            device,
            "thermal interaction shader",
            include_str!("thermal_interaction.wgsl"),
            "engine_physics/src/simulation/thermal_interaction.wgsl",
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("thermal interaction"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let _ = materials;
        Self {
            interaction,
            rigid_raster_claim_counts,
            parameters,
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
    ) {
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            32,
            &buffered_origin[0].to_le_bytes(),
        );
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            36,
            &buffered_origin[1].to_le_bytes(),
        );
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            40,
            &buffered_tiles[0].to_le_bytes(),
        );
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            44,
            &buffered_tiles[1].to_le_bytes(),
        );
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 48, &ring_offset[0].to_le_bytes());
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 52, &ring_offset[1].to_le_bytes());
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("thermal interaction"),
                });
        let mut pass = accelerator.begin_compute_pass(&mut encoder, "gather thermal interaction");
        pass.set_pipeline(&self.clear_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.rigid_capacity.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.count_pipeline);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.gather_pipeline);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
    pub(crate) const fn interaction_buffer(&self) -> &AcceleratorBuffer {
        &self.interaction
    }
}

impl Drop for ThermalInteraction {
    fn drop(&mut self) {
        self.interaction.free();
        self.parameters.destroy();
    }
}
