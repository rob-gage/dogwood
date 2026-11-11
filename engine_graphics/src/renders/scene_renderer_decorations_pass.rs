// Copyright Rob Gage 2026

use super::scene_renderer_lighting_pass::{
    lighting_shader_source,
    LIGHTING_OPTICS_BLEND,
    LIGHTING_OPTICS_FORMAT,
};
use crate::world::scenes::{
    SceneGraphicsCellular,
    SceneGraphicsDecorations,
    SceneGraphicsGas,
};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Distance between the uniforms of neighboring batches, meeting every device's offset alignment
const BATCH_UNIFORM_STRIDE: usize = 256;

/// Bytes of the view uniforms
const VIEW_UNIFORM_BYTE_COUNT: usize = 48;

/// Bytes of the motion field's uniforms
const FIELD_UNIFORM_BYTE_COUNT: usize = 304;

/// Motion field texels along one tile
const FIELD_TEXELS_PER_TILE: u32 = 2;

/// Width and height of a motion field compute group in texels
const FIELD_WORKGROUP_SIZE: u32 = 16;

/// Tiles past the edges of the view the motion field keeps stepping, so that what scrolls into view
/// is already moving
const FIELD_VIEW_MARGIN: f32 = 8.0;

/// Longest step of the motion field in seconds, which keeps it stable through a hitch
const FIELD_STEP_MAXIMUM: f32 = 1.0 / 30.0;

/// The motion field of one resident region: a spring at every texel, in a ring aligned to the
/// world like the cell buffers, costing the same however many plants read it
struct DecorationField {
    /// Size in texels
    size: (u32, u32),
    /// The field plants read
    current: wgpu::Texture,
    /// The field being computed
    next: wgpu::Texture,
    /// View of the field plants read
    view: wgpu::TextureView,
    /// Gas buffer the field reads
    gas: wgpu::Buffer,
    /// Resources of a compute step
    bind_group: wgpu::BindGroup,
}

/// One chunk's prepared decorations
struct DecorationDraw {
    /// Instances, the back layer first
    buffer: wgpu::Buffer,
    /// Number of instances in the back layer
    back_count: u32,
    /// Number of instances in the front layer
    front_count: u32,
}

/// Renderer of composed plants, whose static pieces bend with a motion field and vanish with their
/// support cells
pub struct SceneRendererDecorationsPass {
    /// Layout of the view uniforms, cell buffers, and atlas
    view_bind_group_layout: wgpu::BindGroupLayout,
    /// Layout of one batch's uniforms
    batch_bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline layout
    pipeline_layout: wgpu::PipelineLayout,
    /// Shader
    shader_module: wgpu::ShaderModule,
    /// Pipelines of decorations by target format
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// View uniforms
    view_uniform_buffer: wgpu::Buffer,
    /// View bind group and the resources it holds
    view_bind_group: Option<(wgpu::BindGroup, wgpu::Buffer, wgpu::Buffer, wgpu::TextureView, wgpu::TextureView)>,
    /// Layout of a motion field step
    field_bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline of a motion field step
    field_pipeline: wgpu::ComputePipeline,
    /// Uniforms of a motion field step
    field_uniform_buffer: wgpu::Buffer,
    /// Sampler plants read the motion field with
    field_sampler: wgpu::Sampler,
    /// Motion field of the resident region
    field: Option<DecorationField>,
    /// Simulated time of the field's last step
    field_time: f32,
    /// Whether this frame advances the field
    field_is_due: bool,
    /// Texels this frame's step covers
    field_step_size: (u32, u32),
    /// One block of uniforms per batch
    batch_uniform_buffer: wgpu::Buffer,
    /// Capacity of the batch uniform buffer in batches
    batch_capacity: usize,
    /// Batch uniforms bound with a dynamic offset
    batch_bind_group: wgpu::BindGroup,
    /// What to draw
    draws: Vec<DecorationDraw>,
    /// Pipeline drawing glow into the lighting domain
    optics_pipeline: Option<wgpu::RenderPipeline>,
    /// Glowing pieces this frame: their batch uniform slot, instances, and number of instances
    glowing: Vec<(usize, wgpu::Buffer, u32)>,
}

impl SceneRendererDecorationsPass {

    /// Creates the resources, sampling the lighting group of a layout
    pub fn new(accelerator: &Accelerator, lighting_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let storage = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let view_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererDecorationsPass.view_bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    storage(1),
                    storage(2),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            },
        );
        let batch_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererDecorationsPass.batch_bind_group_layout`"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                }],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererDecorationsPass.pipeline_layout`"),
                bind_group_layouts: &[
                    Some(&view_bind_group_layout),
                    Some(&batch_bind_group_layout),
                    Some(lighting_bind_group_layout),
                ],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererDecorationsPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    lighting_shader_source(2, include_str!("scene_renderer_decorations.wgsl")).into(),
                ),
            },
        );
        let view_uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererDecorationsPass.view_uniform_buffer`"),
            size: VIEW_UNIFORM_BYTE_COUNT as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let field_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererDecorationsPass.field_bind_group_layout`"),
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
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba16Float,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            },
        );
        let field_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererDecorationsPass.field_pipeline_layout`"),
                bind_group_layouts: &[Some(&field_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let field_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererDecorationsPass.field_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("scene_renderer_decoration_field.wgsl").into(),
                ),
            },
        );
        let field_pipeline: wgpu::ComputePipeline = engine_host::acceleration::create_compute_pipeline(device,
            &wgpu::ComputePipelineDescriptor {
                label: Some("`SceneRendererDecorationsPass.field_pipeline`"),
                layout: Some(&field_pipeline_layout),
                module: &field_module,
                entry_point: Some("step_field"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            },
        );
        let field_uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererDecorationsPass.field_uniform_buffer`"),
            size: FIELD_UNIFORM_BYTE_COUNT as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let field_sampler: wgpu::Sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("`SceneRendererDecorationsPass.field_sampler`"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let batch_capacity: usize = 64;
        let batch_uniform_buffer: wgpu::Buffer = Self::batch_uniforms(device, batch_capacity);
        let batch_bind_group: wgpu::BindGroup =
            Self::batch_group(device, &batch_bind_group_layout, &batch_uniform_buffer);
        Self {
            view_bind_group_layout,
            batch_bind_group_layout,
            pipeline_layout,
            shader_module,
            pipelines: HashMap::new(),
            view_uniform_buffer,
            view_bind_group: None,
            field_bind_group_layout,
            field_pipeline,
            field_uniform_buffer,
            field_sampler,
            field: None,
            field_time: 0.0,
            field_is_due: false,
            field_step_size: (0, 0),
            batch_uniform_buffer,
            batch_capacity,
            batch_bind_group,
            draws: Vec::new(),
            optics_pipeline: None,
            glowing: Vec::new(),
        }
    }

    /// Creates uniforms holding a number of batches
    fn batch_uniforms(device: &wgpu::Device, batches: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererDecorationsPass.batch_uniform_buffer`"),
            size: (batches * BATCH_UNIFORM_STRIDE) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Binds batch uniforms
    fn batch_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`SceneRendererDecorationsPass.batch_bind_group`"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(16),
                }),
            }],
        })
    }

    /// Prepares nothing to draw
    pub fn clear(&mut self) {
        self.draws.clear();
        self.glowing.clear();
        self.field_is_due = false;
    }

    /// Creates the motion field of a region of a size in tiles
    fn create_field(&self, device: &wgpu::Device, size: (u32, u32), gas: &wgpu::Buffer) -> DecorationField {
        let texture = |usage: wgpu::TextureUsages| device.create_texture(&wgpu::TextureDescriptor {
            label: Some("`SceneRendererDecorationsPass.field`"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage,
            view_formats: &[],
        });
        let current: wgpu::Texture = texture(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let next: wgpu::Texture = texture(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC);
        let view: wgpu::TextureView = current.create_view(&wgpu::TextureViewDescriptor::default());
        let next_view: wgpu::TextureView = next.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`SceneRendererDecorationsPass.field_bind_group`"),
            layout: &self.field_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.field_uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: gas.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&next_view) },
            ],
        });
        DecorationField { size, current, next, view, gas: gas.clone(), bind_group }
    }

    /// Advances the motion field, if this frame's simulated time moved on
    pub fn compute(&self, command_encoder: &mut wgpu::CommandEncoder) {
        let (true, Some(field)) = (self.field_is_due, self.field.as_ref()) else { return; };
        {
            let mut pass: wgpu::ComputePass<'_> = command_encoder.begin_compute_pass(
                &wgpu::ComputePassDescriptor {
                    label: Some("`SceneRendererDecorationsPass::compute`"),
                    timestamp_writes: None,
                },
            );
            pass.set_pipeline(&self.field_pipeline);
            pass.set_bind_group(0, &field.bind_group, &[]);
            pass.dispatch_workgroups(
                self.field_step_size.0.div_ceil(FIELD_WORKGROUP_SIZE),
                self.field_step_size.1.div_ceil(FIELD_WORKGROUP_SIZE),
                1,
            );
        }
        command_encoder.copy_texture_to_texture(
            field.next.as_image_copy(),
            field.current.as_image_copy(),
            wgpu::Extent3d { width: field.size.0, height: field.size.1, depth_or_array_layers: 1 },
        );
    }

    /// Prepares one view's art for drawing over a resident region
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        viewport_width: u32,
        viewport_height: u32,
        pixels_per_tile: f32,
        cell_grid_origin: [f32; 2],
        decorations: &SceneGraphicsDecorations<'_>,
        cellular: &SceneGraphicsCellular<'_>,
        gas: &SceneGraphicsGas<'_>,
        matter_buffer: &wgpu::Buffer,
    ) {
        let device: &wgpu::Device = accelerator.device();
        self.clear();
        if !self.pipelines.contains_key(&format) {
            let pipeline: wgpu::RenderPipeline = self.create_pipeline(
                device,
                "vertex_decoration",
                "fragment",
                format,
                None,
            );
            self.pipelines.insert(format, pipeline);
        }
        if self.optics_pipeline.is_none() {
            self.optics_pipeline = Some(self.create_pipeline(
                device,
                "vertex_decoration_optics",
                "fragment_optics",
                LIGHTING_OPTICS_FORMAT,
                Some(LIGHTING_OPTICS_BLEND),
            ));
        }
        let field_size: (u32, u32) = (
            cellular.region_width * FIELD_TEXELS_PER_TILE,
            cellular.region_height * FIELD_TEXELS_PER_TILE,
        );
        if self.field.as_ref().is_none_or(|field| field.size != field_size || &field.gas != gas.gas_buffer) {
            self.field = Some(self.create_field(device, field_size, gas.gas_buffer));
        }
        let field_view: wgpu::TextureView =
            self.field.as_ref().expect("the field was just created").view.clone();
        let is_bound: bool = self.view_bind_group.as_ref().is_some_and(|(_, materials, matter, atlas, field)| {
            materials == cellular.material_identifier_buffer
                && matter == matter_buffer
                && atlas == decorations.atlas
                && field == &field_view
        });
        if !is_bound {
            let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`SceneRendererDecorationsPass.view_bind_group`"),
                layout: &self.view_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.view_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: cellular.material_identifier_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry { binding: 2, resource: matter_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(decorations.atlas),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&field_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&self.field_sampler),
                    },
                ],
            });
            self.view_bind_group = Some((
                bind_group,
                cellular.material_identifier_buffer.clone(),
                matter_buffer.clone(),
                decorations.atlas.clone(),
                field_view,
            ));
        }
        let mut view: Vec<u8> = Vec::with_capacity(VIEW_UNIFORM_BYTE_COUNT);
        for value in [
            viewport_width as f32,
            viewport_height as f32,
            pixels_per_tile,
            decorations.time,
            cell_grid_origin[0],
            cell_grid_origin[1],
            cellular.region_x,
            cellular.region_y,
        ] { view.extend_from_slice(&value.to_le_bytes()); }
        for value in [
            cellular.region_width,
            cellular.region_height,
            cellular.ring_offset_x,
            cellular.ring_offset_y,
        ] { view.extend_from_slice(&value.to_le_bytes()); }
        accelerator.queue().write_buffer(&self.view_uniform_buffer, 0, &view);
        let elapsed: f32 = (decorations.time - self.field_time).clamp(0.0, FIELD_STEP_MAXIMUM);
        // without plants in view, nothing reads the field
        self.field_is_due = decorations.time != self.field_time && !decorations.batches.is_empty();
        self.field_time = decorations.time;
        if self.field_is_due {
            let mut field: Vec<u8> = Vec::with_capacity(FIELD_UNIFORM_BYTE_COUNT);
            for value in [cellular.region_x, cellular.region_y, elapsed, 0.0] {
                field.extend_from_slice(&value.to_le_bytes());
            }
            for value in [
                cellular.region_width,
                cellular.region_height,
                cellular.ring_offset_x,
                cellular.ring_offset_y,
            ] { field.extend_from_slice(&value.to_le_bytes()); }
            // only what can be seen steps; camera-relative tiles start at the middle of the view
            let half_view: [f32; 2] = [
                viewport_width as f32 * 0.5 / pixels_per_tile + FIELD_VIEW_MARGIN,
                viewport_height as f32 * 0.5 / pixels_per_tile + FIELD_VIEW_MARGIN,
            ];
            let span = |origin: f32, half: f32, tiles: u32| -> (u32, u32) {
                let last: u32 = tiles * FIELD_TEXELS_PER_TILE;
                let low: u32 = (((-half - origin) * FIELD_TEXELS_PER_TILE as f32).floor().max(0.0) as u32).min(last);
                let high: u32 = (((half - origin) * FIELD_TEXELS_PER_TILE as f32).ceil().max(0.0) as u32).min(last);
                (low, high - low)
            };
            let (start_x, count_x): (u32, u32) = span(cellular.region_x, half_view[0], cellular.region_width);
            let (start_y, count_y): (u32, u32) = span(cellular.region_y, half_view[1], cellular.region_height);
            self.field_step_size = (count_x, count_y);
            for value in [start_x, start_y, count_x, count_y] {
                field.extend_from_slice(&value.to_le_bytes());
            }
            for mover in &decorations.movers {
                for value in [
                    mover.position[0], mover.position[1], mover.half_size[0], mover.half_size[1],
                    mover.velocity[0], mover.velocity[1], 0.0, 0.0,
                ] { field.extend_from_slice(&value.to_le_bytes()); }
            }
            accelerator.queue().write_buffer(&self.field_uniform_buffer, 0, &field);
        }
        let batch_count: usize = decorations.batches.len() + decorations.glowing.len();
        if batch_count > self.batch_capacity {
            self.batch_capacity = batch_count.next_power_of_two();
            self.batch_uniform_buffer = Self::batch_uniforms(device, self.batch_capacity);
            self.batch_bind_group = Self::batch_group(
                device,
                &self.batch_bind_group_layout,
                &self.batch_uniform_buffer,
            );
        }
        let mut uniforms: Vec<u8> = vec![0; batch_count * BATCH_UNIFORM_STRIDE];
        for (index, batch) in decorations.batches.iter().enumerate() {
            let start: usize = index * BATCH_UNIFORM_STRIDE;
            uniforms[start..start + 4].copy_from_slice(&batch.origin[0].to_le_bytes());
            uniforms[start + 4..start + 8].copy_from_slice(&batch.origin[1].to_le_bytes());
            self.draws.push(DecorationDraw {
                buffer: batch.buffer.clone(),
                back_count: batch.back_count,
                front_count: batch.front_count,
            });
        }
        for (index, batch) in decorations.glowing.iter().enumerate() {
            let slot: usize = decorations.batches.len() + index;
            let start: usize = slot * BATCH_UNIFORM_STRIDE;
            uniforms[start..start + 4].copy_from_slice(&batch.origin[0].to_le_bytes());
            uniforms[start + 4..start + 8].copy_from_slice(&batch.origin[1].to_le_bytes());
            self.glowing.push((slot, batch.buffer.clone(), batch.count));
        }
        if !uniforms.is_empty() {
            accelerator.queue().write_buffer(&self.batch_uniform_buffer, 0, &uniforms);
        }
    }

    /// Creates the pipeline of a target format
    fn create_pipeline(
        &self,
        device: &wgpu::Device,
        vertex_entry_point: &str,
        fragment_entry_point: &str,
        format: wgpu::TextureFormat,
        blend: Option<wgpu::BlendState>,
    ) -> wgpu::RenderPipeline {
        engine_host::acceleration::create_render_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some("`SceneRendererDecorationsPass.pipelines`"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader_module,
                entry_point: Some(vertex_entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Uint32x4, 1 => Uint32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.shader_module,
                entry_point: Some(fragment_entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Draws the glow of every resident plant into the lighting domain, as emission
    pub fn draw_optics(&self, render_pass: &mut wgpu::RenderPass<'_>, lighting_bind_group: &wgpu::BindGroup) {
        let (Some(pipeline), Some((view, ..))) =
            (self.optics_pipeline.as_ref(), self.view_bind_group.as_ref())
        else { return; };
        if self.glowing.is_empty() { return; }
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, view, &[]);
        render_pass.set_bind_group(2, lighting_bind_group, &[]);
        for (slot, buffer, count) in &self.glowing {
            render_pass.set_bind_group(
                1,
                &self.batch_bind_group,
                &[(*slot * BATCH_UNIFORM_STRIDE) as u32],
            );
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..4, 0..*count);
        }
    }

    /// Draws the art behind fluid, gas, and cells
    pub fn draw_back(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        lighting_bind_group: &wgpu::BindGroup,
    ) {
        let (Some(pipeline), Some((view, ..))) =
            (self.pipelines.get(&format), self.view_bind_group.as_ref())
        else { return; };
        render_pass.set_bind_group(0, view, &[]);
        render_pass.set_bind_group(1, &self.batch_bind_group, &[0]);
        render_pass.set_bind_group(2, lighting_bind_group, &[]);
        render_pass.set_pipeline(pipeline);
        for (index, draw) in self.draws.iter().enumerate() {
            if draw.back_count == 0 { continue; }
            render_pass.set_bind_group(
                1,
                &self.batch_bind_group,
                &[(index * BATCH_UNIFORM_STRIDE) as u32],
            );
            render_pass.set_vertex_buffer(0, draw.buffer.slice(..));
            render_pass.draw(0..4, 0..draw.back_count);
        }
    }

    /// Draws the art in front of everything in the gameplay plane
    pub fn draw_front(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        lighting_bind_group: &wgpu::BindGroup,
    ) {
        let (Some(pipeline), Some((view, ..))) =
            (self.pipelines.get(&format), self.view_bind_group.as_ref())
        else { return; };
        if self.draws.iter().all(|draw| draw.front_count == 0) { return; }
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, view, &[]);
        render_pass.set_bind_group(2, lighting_bind_group, &[]);
        for (index, draw) in self.draws.iter().enumerate() {
            if draw.front_count == 0 { continue; }
            render_pass.set_bind_group(
                1,
                &self.batch_bind_group,
                &[(index * BATCH_UNIFORM_STRIDE) as u32],
            );
            render_pass.set_vertex_buffer(0, draw.buffer.slice(..));
            render_pass.draw(0..4, draw.back_count..draw.back_count + draw.front_count);
        }
    }

}
