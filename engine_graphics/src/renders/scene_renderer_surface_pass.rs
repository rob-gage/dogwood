// Copyright Rob Gage 2026

use crate::world::scenes::SceneGraphicsCellular;
use engine_host::acceleration::Accelerator;

/// Cells added around the visible rectangle, so lookups just outside it stay valid
const SURFACE_FIELD_MARGIN: i64 = 32;
/// Cells the region may move in a frame before the whole field is written at once
const SURFACE_FIELD_JUMP: i64 = 24;

/// Compute pass deriving, from the live materials, how exposed each visible cell is
///
/// Terrain rarely changes between frames, so alternate rows are written each frame, and the field
/// wraps by world position so that it stays valid as the region moves. Each texel holds closeness to the opposite state
/// (one when touching, falling toward a quarter four cells away, zero beyond), the direction
/// toward open space centered on one half, and whether the cell is solid.
pub struct SceneRendererSurfacePass {
    /// Layout of the uniforms, materials, and field
    bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline writing the field
    pipeline: wgpu::ComputePipeline,
    /// Uniforms of the current view
    uniform_buffer: wgpu::Buffer,
    /// Field view, recreated with the region's size
    field_view: Option<wgpu::TextureView>,
    /// Size of the field in cells
    field_size: (u32, u32),
    /// Bind group of the current field and materials
    bind_group: Option<wgpu::BindGroup>,
    /// Material buffer bound by the bind group
    material_identifier_buffer: Option<wgpu::Buffer>,
    /// Size of the visible rectangle to compute, in workgroups
    workgroups: (u32, u32),
    /// Cell of the region's corner when last prepared
    last_origin: Option<(i64, i64)>,
    /// Which alternate rows to write next
    next_phase: u32,
}

impl SceneRendererSurfacePass {

    /// Creates the surface field resources
    pub fn new(accelerator: &Accelerator) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererSurfacePass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererSurfacePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("scene_renderer_surface.wgsl").into()),
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererSurfacePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let pipeline: wgpu::ComputePipeline = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some("`SceneRendererSurfacePass.pipeline`"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("compute"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            },
        );
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererSurfacePass.uniform_buffer`"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            bind_group_layout,
            pipeline,
            uniform_buffer,
            field_view: None,
            field_size: (0, 0),
            bind_group: None,
            material_identifier_buffer: None,
            workgroups: (0, 0),
            last_origin: None,
            next_phase: 0,
        }
    }

    /// Prepares the field over the visible rectangle of a view, given its size in tiles
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        cellular: &SceneGraphicsCellular<'_>,
        visible_width: f32,
        visible_height: f32,
    ) {
        let device: &wgpu::Device = accelerator.device();
        let tile_size: u32 = cellular.cellular_tile_size;
        let size: (u32, u32) = (cellular.region_width * tile_size, cellular.region_height * tile_size);
        let mut rebind: bool = self.material_identifier_buffer.as_ref()
            != Some(cellular.material_identifier_buffer);
        if self.field_size != size || self.field_view.is_none() {
            let texture: wgpu::Texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("`SceneRendererSurfacePass.field_view`"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.field_view = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.field_size = size;
            self.last_origin = None;
            rebind = true;
        }
        // the rectangle of region-local cells the camera sees
        let cells_per_tile: f32 = tile_size as f32;
        let left: i64 = ((-cellular.region_x - visible_width * 0.5) * cells_per_tile).floor() as i64
            - SURFACE_FIELD_MARGIN;
        let bottom: i64 = ((-cellular.region_y - visible_height * 0.5) * cells_per_tile).floor() as i64
            - SURFACE_FIELD_MARGIN;
        let right: i64 = left + (visible_width * cells_per_tile).ceil() as i64
            + 2 * SURFACE_FIELD_MARGIN;
        let top: i64 = bottom + (visible_height * cells_per_tile).ceil() as i64
            + 2 * SURFACE_FIELD_MARGIN;
        let first_x: u32 = left.clamp(0, i64::from(size.0)) as u32;
        let first_y: u32 = bottom.clamp(0, i64::from(size.1)) as u32;
        let last_x: u32 = right.clamp(0, i64::from(size.0)) as u32;
        let last_y: u32 = top.clamp(0, i64::from(size.1)) as u32;
        self.workgroups = ((last_x - first_x).div_ceil(8), (last_y - first_y).div_ceil(8));
        let cells: i64 = i64::from(tile_size);
        let origin: (i64, i64) = (cellular.region_tile_x * cells, cellular.region_tile_y * cells);
        // a new field or a large move has nothing valid to keep, so all of it is written at once
        let is_jump: bool = self.last_origin.is_none_or(|last| {
            (origin.0 - last.0).abs() > SURFACE_FIELD_JUMP
                || (origin.1 - last.1).abs() > SURFACE_FIELD_JUMP
        });
        self.last_origin = Some(origin);
        self.next_phase = 1 - self.next_phase;
        let phase: u32 = if is_jump { 2 } else { self.next_phase };
        let words: [u32; 10] = [
            cellular.region_width,
            cellular.region_height,
            cellular.ring_offset_x,
            cellular.ring_offset_y,
            tile_size,
            first_x,
            first_y,
            phase,
            origin.0.rem_euclid(i64::from(size.0)) as u32,
            origin.1.rem_euclid(i64::from(size.1)) as u32,
        ];
        let mut bytes: [u8; 48] = [0; 48];
        for (index, word) in words.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &bytes);
        if rebind || self.bind_group.is_none() {
            let Some(field_view) = self.field_view.as_ref() else { return; };
            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`SceneRendererSurfacePass.bind_group`"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: cellular.material_identifier_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(field_view),
                    },
                ],
            }));
            self.material_identifier_buffer = Some(cellular.material_identifier_buffer.clone());
        }
    }

    /// Records the field's computation
    pub fn compute(&self, command_encoder: &mut wgpu::CommandEncoder) {
        let Some(bind_group) = self.bind_group.as_ref() else { return; };
        let mut compute_pass: wgpu::ComputePass<'_> = command_encoder.begin_compute_pass(
            &wgpu::ComputePassDescriptor {
                label: Some("`SceneRendererSurfacePass::compute`"),
                timestamp_writes: None,
            },
        );
        compute_pass.set_pipeline(&self.pipeline);
        compute_pass.set_bind_group(0, bind_group, &[]);
        compute_pass.dispatch_workgroups(self.workgroups.0, self.workgroups.1, 1);
    }

    /// Returns a view of the field, which exists once prepared
    pub fn field_view(&self) -> Option<&wgpu::TextureView> { self.field_view.as_ref() }

}
