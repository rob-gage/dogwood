// Copyright Rob Gage 2026

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
        delta_time: f32,
    ) {
        let parameter_values: [u32; 8] = [
            delta_time.to_bits(),
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
            &parameter_values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "thermal conduction");
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
        let device: &wgpu::Device = accelerator.wgpu_device();
        let solved: AcceleratorBuffer = accelerator.allocate::<[f32; 4]>(cell_count as usize);
        let face_flux: AcceleratorBuffer = accelerator.allocate::<[f32; 2]>(cell_count as usize);
        let face_conductance: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(cell_count as usize);
        let conductance_sum: AcceleratorBuffer = accelerator.allocate::<f32>(cell_count as usize);
        let parameters: wgpu::Buffer = crate::simulation::create_simulation_uniform_buffer(
            device,
            "thermal conduction parameters",
            64,
        );
        let storage = crate::simulation::storage_bind_group_layout_entry;
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("thermal conduction"),
                entries: &[
                    storage(0, true),
                    storage(1, false),
                    storage(2, false),
                    storage(3, false),
                    storage(4, false),
                    crate::simulation::uniform_bind_group_layout_entry(5),
                ],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal conduction"),
            layout: &layout,
            entries: &[
                crate::simulation::accelerator_buffer_bind_group_entry(0, &interaction),
                crate::simulation::accelerator_buffer_bind_group_entry(1, &face_flux),
                crate::simulation::accelerator_buffer_bind_group_entry(2, &face_conductance),
                crate::simulation::accelerator_buffer_bind_group_entry(3, &conductance_sum),
                crate::simulation::accelerator_buffer_bind_group_entry(4, &solved),
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "thermal conduction shader",
            include_str!("thermal_conduction.wgsl"),
            "engine_physics/src/simulation/thermal_conduction.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("thermal conduction"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let pipeline = |entry: &'static str| {
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
        delta_time: f32,
    ) {
        let parameter_values: [u32; 8] = [
            delta_time.to_bits(),
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
            &parameter_values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("thermal conduction"),
                });
        let mut pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(&mut encoder, "thermal conduction flux");
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
