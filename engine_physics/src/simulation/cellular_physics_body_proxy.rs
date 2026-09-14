// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;
use engine_compute::{
    Accelerator,
    AcceleratorBuffer
};

/// Rasterizes the possessed pawn into transient cellular interaction geometry
pub struct CellularPhysicsBodyProxy {
    occupancy: AcceleratorBuffer,
    velocity: AcceleratorBuffer,
    count: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    clear_pipeline: wgpu::ComputePipeline,
    raster_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
}

impl CellularPhysicsBodyProxy {

    pub fn new(accelerator: &Accelerator, buffered_cell_count: usize) -> Self {
        let device = accelerator.wgpu_device();
        let buffered_cell_count = buffered_cell_count as u32;
        let occupancy = accelerator.allocate::<u32>(buffered_cell_count as usize);
        let velocity = accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let count = accelerator.allocate::<u32>(1);
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular physics body proxy parameters"), size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding, visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false, min_binding_size: None }, count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cellular physics body proxy bind group layout"), entries: &[
                storage(0, false), storage(1, false), storage(2, false),
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cellular physics body proxy bind group"), layout: &layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: occupancy.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: velocity.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: count.wgpu_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: parameters.as_entire_binding() },
            ],
        });
        let shader = super::create_simulation_shader_module(
            device,
            "cellular physics body proxy shader",
            include_str!("cellular_physics_body_proxy.wgsl"),
            "engine_physics/src/simulation/cellular_physics_body_proxy.wgsl",
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cellular physics body proxy pipeline layout"), bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point, label| device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some(label), layout: Some(&pipeline_layout), module: &shader,
                entry_point: Some(entry_point), compilation_options: Default::default(), cache: None,
            });
        Self { occupancy, velocity, count, parameters, bind_group,
            clear_pipeline: pipeline("clear_cellular_physics_body_proxy", "cellular physics body proxy clear pipeline"),
            raster_pipeline: pipeline("rasterize_cellular_physics_body_proxy", "cellular physics body proxy raster pipeline"),
            buffered_cell_count }
    }

    pub const fn occupancy_buffer(&self) -> &AcceleratorBuffer { &self.occupancy }
    pub const fn velocity_buffer(&self) -> &AcceleratorBuffer { &self.velocity }
    pub const fn count_buffer(&self) -> &AcceleratorBuffer { &self.count }

    pub fn rasterize(
        &self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        pawn: Option<([f32; 2], [f32; 2], [f32; 2], [f32; 2])>,
        is_fluid_permeable: bool,
    ) {
        let (center, velocity, collider, drive, occupancy) = pawn.map_or(
            ([0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2], 0u32),
            |(center, velocity, collider, drive)| (center, velocity, collider, drive,
                if is_fluid_permeable { 2u32 } else { 1u32 }),
        );
        let values = [origin.x as u32, origin.y as u32, u32::from(width), u32::from(height),
            u32::from(ring_offset_x), u32::from(ring_offset_y), center[0].to_bits(), center[1].to_bits(),
            velocity[0].to_bits(), velocity[1].to_bits(), drive[0].to_bits(), drive[1].to_bits(),
            collider[0].to_bits(), collider[1].to_bits(), gravity[0].to_bits(), gravity[1].to_bits(),
            self.buffered_cell_count, occupancy, 0, 0, 0, 0, 0, 0];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &bytes);
        let mut encoder =
            accelerator.wgpu_device().create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cellular physics body proxy rasterization"),
            });
        for (pipeline, label) in [(&self.clear_pipeline, "clear cellular physics body proxy"),
                (&self.raster_pipeline, "rasterize cellular physics body proxy")] {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(self.buffered_cell_count.div_ceil(64), 1, 1);
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}

impl Drop for CellularPhysicsBodyProxy {

    fn drop(&mut self) {
        self.occupancy.free();
        self.velocity.free();
        self.count.free();
        self.parameters.destroy();
    }
}
