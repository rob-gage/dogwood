use super::CellularStaticState;
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::sync::{Arc, Mutex};

pub(crate) struct CellularStaticStateGather {
    descriptors: AcceleratorBuffer,
    output: AcceleratorBuffer,
    count: wgpu::Buffer,
    readback: wgpu::Buffer,
    status: Arc<Mutex<Option<Result<Vec<CellularStaticState>, String>>>>,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
    capacity: usize,
}

impl CellularStaticStateGather {
    pub(crate) fn new(
        accelerator: &Accelerator,
        material_identifiers: &AcceleratorBuffer,
        appearances: &AcceleratorBuffer,
        integrities: &AcceleratorBuffer,
        amounts: &AcceleratorBuffer,
        temperatures: &AcceleratorBuffer,
        capacity: usize,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let capacity: usize = capacity.max(1);
        let descriptors: AcceleratorBuffer = accelerator.allocate::<u32>(capacity);
        let output: AcceleratorBuffer = accelerator.allocate::<[u32; 8]>(capacity);
        let count: wgpu::Buffer = crate::simulation::create_simulation_uniform_buffer(
            device,
            "cellular static state gather count",
            4,
        );
        let readback: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular static state gather readback"),
            size: capacity as u64 * 32,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let status: Arc<Mutex<Option<Result<Vec<CellularStaticState>, String>>>> =
            Arc::new(Mutex::new(None));
        let storage: fn(u32, bool) -> wgpu::BindGroupLayoutEntry =
            crate::simulation::storage_bind_group_layout_entry;
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular static state gather"),
                entries: &[
                    storage(0, true),
                    storage(1, true),
                    storage(2, true),
                    storage(3, true),
                    storage(4, true),
                    storage(5, true),
                    storage(6, false),
                    crate::simulation::uniform_bind_group_layout_entry(7),
                ],
            });
        let bind_group_entries: [wgpu::BindingResource<'_>; 8] = [
            descriptors.wgpu_buffer().as_entire_binding(),
            material_identifiers.wgpu_buffer().as_entire_binding(),
            appearances.wgpu_buffer().as_entire_binding(),
            integrities.wgpu_buffer().as_entire_binding(),
            amounts.wgpu_buffer().as_entire_binding(),
            temperatures.wgpu_buffer().as_entire_binding(),
            output.wgpu_buffer().as_entire_binding(),
            count.as_entire_binding(),
        ];
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular static state gather"),
            layout: &layout,
            entries: bind_group_entries
                .iter()
                .enumerate()
                .map(|(binding, resource)| wgpu::BindGroupEntry {
                    binding: binding as u32,
                    resource: resource.clone(),
                })
                .collect::<Vec<_>>()
                .as_slice(),
        });
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "cellular static state gather",
            include_str!("cellular_static_state_gather.wgsl"),
            "engine_physics/src/simulation_cellulars/cellular_static_state_gather.wgsl",
        );
        let pipeline: wgpu::ComputePipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("cellular static state gather"),
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
            readback,
            status,
            bind_group,
            pipeline,
            capacity,
        }
    }

    pub(crate) fn submit(&self, accelerator: &Accelerator, indices: &[u32]) -> bool {
        if indices.is_empty() || indices.len() > self.capacity {
            return false;
        }
        let Ok(mut status) = self.status.lock() else {
            return false;
        };
        if status.is_some() {
            return false;
        }
        let bytes: Vec<u8> = indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect();
        accelerator
            .wgpu_queue()
            .write_buffer(self.descriptors.wgpu_buffer(), 0, &bytes);
        accelerator.wgpu_queue().write_buffer(
            &self.count,
            0,
            &(indices.len() as u32).to_le_bytes(),
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cellular static state gather"),
                });
        let mut pass = accelerator.begin_compute_pass(&mut encoder, "cellular static state gather");
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups((indices.len() as u32).div_ceil(64), 1, 1);
        drop(pass);
        let byte_count = indices.len() as u64 * 32;
        encoder.copy_buffer_to_buffer(self.output.wgpu_buffer(), 0, &self.readback, 0, byte_count);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let readback = self.readback.clone();
        let callback_status = self.status.clone();
        readback
            .clone()
            .slice(0..byte_count)
            .map_async(wgpu::MapMode::Read, move |result| {
                let result = result.map_err(|error| error.to_string()).map(|_| {
                    let mapped = readback.slice(0..byte_count).get_mapped_range().unwrap();
                    let values = mapped
                        .as_chunks::<32>()
                        .0
                        .iter()
                        .map(|bytes| CellularStaticState {
                            material: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                            appearance: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                            integrity: f32::from_bits(u32::from_le_bytes(
                                bytes[8..12].try_into().unwrap(),
                            )),
                            amount: f32::from_bits(u32::from_le_bytes(
                                bytes[12..16].try_into().unwrap(),
                            )),
                            temperature: f32::from_bits(u32::from_le_bytes(
                                bytes[16..20].try_into().unwrap(),
                            )),
                        })
                        .collect();
                    drop(mapped);
                    readback.unmap();
                    values
                });
                if let Ok(mut status) = callback_status.lock() {
                    *status = Some(result);
                }
            });
        *status = Some(Err("pending".to_owned()));
        true
    }

    pub(crate) fn take_completed(&self) -> Option<Result<Vec<CellularStaticState>, String>> {
        let Ok(mut status) = self.status.lock() else {
            return Some(Err(
                "cellular static state gather status unavailable".to_owned()
            ));
        };
        match status.as_ref() {
            Some(Err(error)) if error == "pending" => None,
            Some(_) => status.take(),
            None => None,
        }
    }
}

impl Drop for CellularStaticStateGather {
    fn drop(&mut self) {
        self.descriptors.free();
        self.output.free();
        self.count.destroy();
        self.readback.destroy();
    }
}
