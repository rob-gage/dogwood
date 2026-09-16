use engine_compute::{Accelerator, AcceleratorBuffer};

pub(crate) struct ThermalScatter {
    rigid_temperature_sum: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    cellular_pipeline: wgpu::ComputePipeline,
    fluid_pipeline: wgpu::ComputePipeline,
    clear_rigid_pipeline: wgpu::ComputePipeline,
    accumulate_rigid_pipeline: wgpu::ComputePipeline,
    apply_rigid_pipeline: wgpu::ComputePipeline,
    cell_count: u32,
    particle_capacity: u32,
    rigid_capacity: u32,
}

impl ThermalScatter {
    pub(crate) fn encode(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        has_rigid: bool,
    ) {
        let vals = [
            origin[0] as u32,
            origin[1] as u32,
            tiles[0],
            tiles[1],
            ring[0],
            ring[1],
        ];
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            0,
            &vals
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut pass = accelerator.begin_compute_pass(encoder, "thermal scatter");
        pass.set_bind_group(0, &self.bind_group, &[]);
        if has_rigid {
            pass.set_pipeline(&self.clear_rigid_pipeline);
            pass.dispatch_workgroups(self.rigid_capacity.div_ceil(64), 1, 1);
        }
        pass.set_pipeline(&self.cellular_pipeline);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.fluid_pipeline);
        pass.dispatch_workgroups(self.particle_capacity.div_ceil(64), 1, 1);
        if has_rigid {
            pass.set_pipeline(&self.accumulate_rigid_pipeline);
            pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
            pass.set_pipeline(&self.apply_rigid_pipeline);
            pass.dispatch_workgroups(self.rigid_capacity.div_ceil(64), 1, 1);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        accelerator: &Accelerator,
        solved: &AcceleratorBuffer,
        cellular_materials: &AcceleratorBuffer,
        cellular_amounts: &AcceleratorBuffer,
        cellular_temperatures: &AcceleratorBuffer,
        particles: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        gas_temperatures: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
        rigid_claims: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        rigid_amounts: &AcceleratorBuffer,
        rigid_temperatures: &AcceleratorBuffer,
        rigid_claim_counts: &AcceleratorBuffer,
        external_occupancy: &AcceleratorBuffer,
        cell_count: u32,
        particle_capacity: u32,
        gas_count: u32,
        rigid_capacity: u32,
        empty_space_heat_capacity: f32,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let rigid_temperature_sum = accelerator.allocate::<u32>(rigid_capacity as usize);
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thermal scatter parameters"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vals = [
            0u32,
            0,
            0,
            0,
            0,
            0,
            cell_count,
            particle_capacity,
            gas_count,
            rigid_capacity,
            empty_space_heat_capacity.to_bits(),
            0,
            0,
            0,
            0,
            0,
        ];
        accelerator.wgpu_queue().write_buffer(
            &parameters,
            0,
            &vals
                .iter()
                .flat_map(|v| v.to_le_bytes())
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
        let mut entries: Vec<_> = (0..15)
            .map(|b| storage(b, !matches!(b, 3 | 4 | 6 | 11 | 13)))
            .collect();
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thermal scatter"),
            entries: &entries,
        });
        let buffers = [
            solved,
            cellular_materials,
            cellular_amounts,
            cellular_temperatures,
            particles,
            gas_concentrations,
            gas_temperatures,
            fluid_coverage,
            rigid_claims,
            rigid_cells,
            rigid_amounts,
            rigid_temperatures,
            rigid_claim_counts,
            &rigid_temperature_sum,
            external_occupancy,
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
            binding: 15,
            resource: parameters.as_entire_binding(),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal scatter"),
            layout: &layout,
            entries: &bind_entries,
        });
        let shader = super::create_simulation_shader_module(
            device,
            "thermal scatter shader",
            include_str!("thermal_scatter.wgsl"),
            "engine_physics/src/simulation/thermal_scatter.wgsl",
        );
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("thermal scatter"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            rigid_temperature_sum,
            parameters,
            bind_group,
            cellular_pipeline: pipeline("scatter_cellular_gas"),
            fluid_pipeline: pipeline("scatter_fluid_particles"),
            clear_rigid_pipeline: pipeline("clear_rigid_temperature_sums"),
            accumulate_rigid_pipeline: pipeline("accumulate_rigid_temperature_sums"),
            apply_rigid_pipeline: pipeline("apply_rigid_temperatures"),
            cell_count,
            particle_capacity,
            rigid_capacity,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn scatter(
        &self,
        accelerator: &Accelerator,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        has_rigid: bool,
    ) {
        let vals = [
            origin[0] as u32,
            origin[1] as u32,
            tiles[0],
            tiles[1],
            ring[0],
            ring[1],
        ];
        accelerator.wgpu_queue().write_buffer(
            &self.parameters,
            0,
            &vals
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("thermal scatter"),
                });
        let mut pass = accelerator.begin_compute_pass(&mut encoder, "thermal scatter");
        pass.set_bind_group(0, &self.bind_group, &[]);
        if has_rigid {
            pass.set_pipeline(&self.clear_rigid_pipeline);
            pass.dispatch_workgroups(self.rigid_capacity.div_ceil(64), 1, 1);
        }
        pass.set_pipeline(&self.cellular_pipeline);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.fluid_pipeline);
        pass.dispatch_workgroups(self.particle_capacity.div_ceil(64), 1, 1);
        if has_rigid {
            pass.set_pipeline(&self.accumulate_rigid_pipeline);
            pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
            pass.set_pipeline(&self.apply_rigid_pipeline);
            pass.dispatch_workgroups(self.rigid_capacity.div_ceil(64), 1, 1);
        }
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}

impl Drop for ThermalScatter {
    fn drop(&mut self) {
        self.rigid_temperature_sum.free();
        self.parameters.destroy();
    }
}
