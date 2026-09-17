use engine_compute::{Accelerator, AcceleratorBuffer};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Applies sparse externally-authored temperature deltas directly to authoritative state.
pub(crate) struct ThermalEdits {
    requests: AcceleratorBuffer,
    count: AcceleratorBuffer,
    deltas: AcceleratorBuffer,
    rigid_flags: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    request_pipeline: wgpu::ComputePipeline,
    fluid_pipeline: wgpu::ComputePipeline,
    rigid_pipeline: wgpu::ComputePipeline,
    capacity: u32,
    particle_capacity: u32,
    rigid_capacity: u32,
    generation: AtomicU32,
}

impl ThermalEdits {
    pub(crate) fn new(
        accelerator: &Accelerator,
        cellular_materials: &AcceleratorBuffer,
        cellular_temperatures: &AcceleratorBuffer,
        gas_temperatures: &AcceleratorBuffer,
        particles: &AcceleratorBuffer,
        rigid_claims: &AcceleratorBuffer,
        rigid_cells: &AcceleratorBuffer,
        rigid_temperatures: &AcceleratorBuffer,
        capacity: u32,
        particle_capacity: u32,
        rigid_capacity: u32,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let requests = accelerator.allocate::<[u32; 2]>(capacity as usize);
        let count = accelerator.allocate::<u32>(1);
        let deltas = accelerator.allocate::<[u32; 2]>(capacity as usize);
        let rigid_flags = accelerator.allocate::<u32>(rigid_capacity as usize);
        // `u32::MAX` means no raster request selected this authoritative slot.
        let zero_rigid = vec![u32::MAX; rigid_capacity as usize];
        accelerator.wgpu_queue().write_buffer(
            rigid_flags.wgpu_buffer(),
            0,
            &zero_rigid
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thermal edit parameters"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = crate::simulation::storage_bind_group_layout_entry;
        let mut layout_entries: Vec<_> = (0..11)
            .map(|b| storage(b, matches!(b, 0 | 1 | 6 | 7)))
            .collect();
        layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 11,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thermal edits"),
            entries: &layout_entries,
        });
        let buffers = [
            &requests,
            &count,
            cellular_materials,
            cellular_temperatures,
            gas_temperatures,
            particles,
            rigid_claims,
            rigid_cells,
            rigid_temperatures,
            &deltas,
            &rigid_flags,
        ];
        let mut entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.wgpu_buffer().as_entire_binding(),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 11,
            resource: parameters.as_entire_binding(),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal edits"),
            layout: &layout,
            entries: &entries,
        });
        let shader = crate::simulation::create_simulation_shader_module(
            device,
            "thermal edits shader",
            include_str!("thermal_edits.wgsl"),
            "engine_physics/src/simulation/thermal_edits.wgsl",
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("thermal edits"),
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
            requests,
            count,
            deltas,
            rigid_flags,
            parameters,
            bind_group,
            request_pipeline: pipeline("apply_thermal_requests"),
            fluid_pipeline: pipeline("apply_thermal_fluid"),
            rigid_pipeline: pipeline("apply_thermal_rigid"),
            capacity,
            particle_capacity,
            rigid_capacity,
            generation: AtomicU32::new(0),
        }
    }

    pub(crate) fn apply(
        &self,
        accelerator: &Accelerator,
        physical_indices: &[u32],
        deltas: &BTreeMap<usize, f32>,
        ring_origin: [i32; 2],
        ring_tiles: [u32; 2],
        ring_offset: [u32; 2],
    ) {
        if physical_indices.is_empty() {
            return;
        }
        let count = physical_indices.len().min(self.capacity as usize) as u32;
        let generation = self
            .generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let mut bytes = Vec::with_capacity(count as usize * 8);
        for &index in physical_indices.iter().take(count as usize) {
            bytes.extend_from_slice(&index.to_le_bytes());
            bytes.extend_from_slice(&deltas[&(index as usize)].to_bits().to_le_bytes());
        }
        accelerator
            .wgpu_queue()
            .write_buffer(self.requests.wgpu_buffer(), 0, &bytes);
        accelerator
            .wgpu_queue()
            .write_buffer(self.count.wgpu_buffer(), 0, &count.to_le_bytes());
        let mut params = Vec::new();
        for v in [
            ring_origin[0] as u32,
            ring_origin[1] as u32,
            ring_tiles[0],
            ring_tiles[1],
            ring_offset[0],
            ring_offset[1],
            self.capacity,
            count,
            generation,
            0,
            0,
            0,
        ] {
            params.extend_from_slice(&v.to_le_bytes());
        }
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 0, &params);
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("thermal edits"),
                });
        let dispatch =
            |pass: &mut wgpu::ComputePass<'_>, pipeline: &wgpu::ComputePipeline, n: u32| {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.dispatch_workgroups(n.div_ceil(64), 1, 1);
            };
        {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, "thermal cellular and gas");
            dispatch(&mut pass, &self.request_pipeline, count);
        }
        {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, "thermal fluid");
            dispatch(&mut pass, &self.fluid_pipeline, self.particle_capacity);
        }
        {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, "thermal rigid");
            dispatch(&mut pass, &self.rigid_pipeline, self.rigid_capacity);
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}

impl Drop for ThermalEdits {
    fn drop(&mut self) {
        self.requests.free();
        self.count.free();
        self.deltas.free();
        self.rigid_flags.free();
        self.parameters.destroy();
    }
}
