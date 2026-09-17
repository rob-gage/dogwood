use engine_compute::{Accelerator, AcceleratorBuffer};

/// Reusable Accelerator gather for sparse rigid state slots.  One dispatch and one
/// contiguous copy replace three tiny copy commands per rigid cell.
pub(crate) struct RigidCellStateGather {
    descriptors: AcceleratorBuffer,
    output: AcceleratorBuffer,
    count: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
}

impl RigidCellStateGather {
    pub(crate) fn new(
        accelerator: &Accelerator,
        integrities: &AcceleratorBuffer,
        amounts: &AcceleratorBuffer,
        temperatures: &AcceleratorBuffer,
        capacity: usize,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let descriptors = accelerator.allocate::<u32>(capacity.max(1));
        let output = accelerator.allocate::<[f32; 4]>(capacity.max(1));
        let count = crate::simulation::create_simulation_uniform_buffer(
            device,
            "rigid cell state gather count",
            4,
        );
        let storage = crate::simulation::storage_bind_group_layout_entry;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rigid cell state gather"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, true),
                storage(3, true),
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
            label: Some("rigid cell state gather"),
            layout: &layout,
            entries: [
                &descriptors.wgpu_buffer(),
                &integrities.wgpu_buffer(),
                &amounts.wgpu_buffer(),
                &temperatures.wgpu_buffer(),
                &output.wgpu_buffer(),
                &count,
            ]
            .into_iter()
            .enumerate()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>()
            .as_slice(),
        });
        let shader = crate::simulation::create_simulation_shader_module(
            device,
            "rigid cell state gather",
            include_str!("rigid_cell_state_gather.wgsl"),
            "engine_physics/src/simulation/rigid_cell_state_gather.wgsl",
        );
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rigid cell state gather"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("gather"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            descriptors,
            output,
            count,
            bind_group,
            pipeline,
        }
    }

    pub(crate) fn submit(&self, accelerator: &Accelerator, slots: &[u32], readback: &wgpu::Buffer) {
        if slots.is_empty() {
            return;
        }
        let bytes: Vec<u8> = slots.iter().flat_map(|slot| slot.to_le_bytes()).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(self.descriptors.wgpu_buffer(), 0, &bytes);
        accelerator
            .wgpu_queue()
            .write_buffer(&self.count, 0, &(slots.len() as u32).to_le_bytes());
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid dormancy gather"),
                });
        let mut pass = accelerator.begin_compute_pass(&mut encoder, "rigid dormancy gather");
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups((slots.len() as u32).div_ceil(64), 1, 1);
        drop(pass);
        encoder.copy_buffer_to_buffer(
            self.output.wgpu_buffer(),
            0,
            readback,
            0,
            slots.len() as u64 * 16,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}

impl Drop for RigidCellStateGather {
    fn drop(&mut self) {
        self.descriptors.free();
        self.output.free();
        self.count.destroy();
    }
}
