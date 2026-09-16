use engine_compute::{Accelerator, AcceleratorBuffer};

pub(crate) struct ThermalConduction {
    solved: AcceleratorBuffer,
    face_flux: AcceleratorBuffer,
    face_conductance: AcceleratorBuffer,
    conductance_sum: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    flux_pipeline: wgpu::ComputePipeline,
    sum_pipeline: wgpu::ComputePipeline,
    actual_flux_pipeline: wgpu::ComputePipeline,
    resolve_pipeline: wgpu::ComputePipeline,
    cell_count: u32,
}

impl ThermalConduction {
    pub(crate) fn encode(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        dt: f32,
    ) {
        let values = [
            dt.to_bits(),
            self.cell_count,
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
            &values
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut pass = accelerator.begin_compute_pass(encoder, "thermal conduction");
        pass.set_bind_group(0, &self.bind_group, &[]);
        for pipeline in [
            &self.flux_pipeline,
            &self.sum_pipeline,
            &self.actual_flux_pipeline,
            &self.resolve_pipeline,
        ] {
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        }
    }
    pub(crate) fn new(
        accelerator: &Accelerator,
        interaction: &AcceleratorBuffer,
        cell_count: u32,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let solved = accelerator.allocate::<[f32; 4]>(cell_count as usize);
        let face_flux = accelerator.allocate::<[f32; 2]>(cell_count as usize);
        let face_conductance = accelerator.allocate::<[f32; 2]>(cell_count as usize);
        let conductance_sum = accelerator.allocate::<f32>(cell_count as usize);
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thermal conduction parameters"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thermal conduction"),
            entries: &[
                storage(0, true),
                storage(1, false),
                storage(2, false),
                storage(3, false),
                storage(4, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal conduction"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: interaction.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: face_flux.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: face_conductance.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: conductance_sum.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: solved.wgpu_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        let shader = super::create_simulation_shader_module(
            device,
            "thermal conduction shader",
            include_str!("thermal_conduction.wgsl"),
            "engine_physics/src/simulation/thermal_conduction.wgsl",
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("thermal conduction"),
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
        Self {
            solved,
            face_flux,
            face_conductance,
            conductance_sum,
            parameters,
            bind_group,
            flux_pipeline: pipeline("calculate_thermal_face_flux"),
            sum_pipeline: pipeline("calculate_thermal_conductance_sum"),
            actual_flux_pipeline: pipeline("calculate_thermal_actual_flux"),
            resolve_pipeline: pipeline("resolve_thermal_conduction"),
            cell_count,
        }
    }
    pub(crate) fn conduct(
        &self,
        accelerator: &Accelerator,
        origin: [i32; 2],
        tiles: [u32; 2],
        ring: [u32; 2],
        dt: f32,
    ) {
        let values = [
            dt.to_bits(),
            self.cell_count,
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
            &values
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("thermal conduction"),
                });
        let mut pass = accelerator.begin_compute_pass(&mut encoder, "thermal conduction flux");
        pass.set_pipeline(&self.flux_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.sum_pipeline);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.actual_flux_pipeline);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.resolve_pipeline);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
    pub(crate) const fn solved_buffer(&self) -> &AcceleratorBuffer {
        &self.solved
    }
}
impl Drop for ThermalConduction {
    fn drop(&mut self) {
        self.solved.free();
        self.face_flux.free();
        self.face_conductance.free();
        self.conductance_sum.free();
        self.parameters.destroy();
    }
}
