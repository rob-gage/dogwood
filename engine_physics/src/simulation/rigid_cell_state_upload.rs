use engine_compute::{Accelerator, AcceleratorBuffer};

pub(crate) struct RigidCellStateUpload {
    records: AcceleratorBuffer,
    count: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
}

impl RigidCellStateUpload {
    pub(crate) fn new(
        accelerator: &Accelerator,
        integrities: &AcceleratorBuffer,
        amounts: &AcceleratorBuffer,
        temperatures: &AcceleratorBuffer,
        capacity: usize,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let records = accelerator.allocate::<[u32; 4]>(capacity.max(1));
        let count = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rigid cell state upload count"),
            size: 4,
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
            label: Some("rigid cell state upload"),
            entries: &[
                storage(0, true),
                storage(1, false),
                storage(2, false),
                storage(3, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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
            label: Some("rigid cell state upload"),
            layout: &layout,
            entries: [
                &records.wgpu_buffer(),
                &integrities.wgpu_buffer(),
                &amounts.wgpu_buffer(),
                &temperatures.wgpu_buffer(),
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
        let shader = super::create_simulation_shader_module(
            device,
            "rigid cell state upload",
            include_str!("rigid_cell_state_upload.wgsl"),
            "engine_physics/src/simulation/rigid_cell_state_upload.wgsl",
        );
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rigid cell state upload"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("upload"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            records,
            count,
            bind_group,
            pipeline,
        }
    }

    pub(crate) fn apply(&self, accelerator: &Accelerator, records: &[[u32; 4]]) {
        if records.is_empty() {
            return;
        }
        let bytes: Vec<u8> = records
            .iter()
            .flat_map(|r| r.iter().flat_map(|v| v.to_le_bytes()))
            .collect();
        accelerator
            .wgpu_queue()
            .write_buffer(self.records.wgpu_buffer(), 0, &bytes);
        accelerator.wgpu_queue().write_buffer(
            &self.count,
            0,
            &(records.len() as u32).to_le_bytes(),
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid cell state upload"),
                });
        let mut pass = accelerator.begin_compute_pass(&mut encoder, "rigid cell state upload");
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups((records.len() as u32).div_ceil(64), 1, 1);
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}

impl Drop for RigidCellStateUpload {
    fn drop(&mut self) {
        self.records.free();
        self.count.destroy();
    }
}
