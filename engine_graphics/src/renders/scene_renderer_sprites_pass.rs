// Copyright Rob Gage 2026

use crate::world::scenes::SceneGraphicsSprites;
use engine_host::acceleration::Accelerator;
use std::{
    collections::HashMap,
    ops::{
        Range,
        RangeInclusive,
    },
};

/// Renderer for the scene sprites within one range of depths
pub struct SceneRendererSpritesPass {
    /// Depths of the sprites drawn by this pass
    depths: RangeInclusive<i32>,
    /// Layout for sprite uniforms and sampling
    bind_group_layout: wgpu::BindGroupLayout,
    /// Layout for one sprite texture
    texture_bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline layout for sprite rendering
    pipeline_layout: wgpu::PipelineLayout,
    /// Shader module for sprite rendering
    shader_module: wgpu::ShaderModule,
    /// Nearest sampler clamped to each texture edge
    sampler: wgpu::Sampler,
    /// Render pipelines indexed by target format
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Per-view sprite uniform buffer
    uniform_buffer: wgpu::Buffer,
    /// Cached uniform and sampler bind group
    bind_group: Option<wgpu::BindGroup>,
    /// Reused instance vertex buffer
    instance_buffer: wgpu::Buffer,
    /// Capacity of the instance vertex buffer in bytes
    instance_buffer_capacity: usize,
    /// Reused indices of the drawn sprites in depth order
    sprite_indices: Vec<usize>,
    /// Reused packed instance bytes
    instance_bytes: Vec<u8>,
    /// Consecutive sprite batches indexed by instance range
    batches: Vec<(wgpu::TextureView, Range<u32>)>,
    /// Cached texture bind groups
    texture_bind_groups: HashMap<wgpu::TextureView, wgpu::BindGroup>,
}

impl SceneRendererSpritesPass {

    /// Creates sprite rendering resources for the sprites within a range of depths
    pub fn new(accelerator: &Accelerator, depths: RangeInclusive<i32>) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererSpritesPass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            },
        );
        let texture_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererSpritesPass.texture_bind_group_layout`"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererSpritesPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&texture_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererSpritesPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("scene_renderer_sprites.wgsl").into(),
                ),
            },
        );
        let sampler: wgpu::Sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("`SceneRendererSpritesPass.sampler`"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline> = HashMap::new();
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererSpritesPass.uniform_buffer`"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer_capacity: usize = 256 * 56;
        let instance_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererSpritesPass.instance_buffer`"),
            size: instance_buffer_capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_bytes: Vec<u8> = Vec::with_capacity(instance_buffer_capacity);
        let batches: Vec<(wgpu::TextureView, Range<u32>)> = Vec::new();
        let texture_bind_groups: HashMap<wgpu::TextureView, wgpu::BindGroup> = HashMap::new();
        Self {
            depths,
            bind_group_layout,
            texture_bind_group_layout,
            pipeline_layout,
            shader_module,
            sampler,
            pipelines,
            uniform_buffer,
            bind_group: None,
            instance_buffer,
            instance_buffer_capacity,
            sprite_indices: Vec::new(),
            instance_bytes,
            batches,
            texture_bind_groups,
        }
    }

    /// Uploads sprite uniforms and prepares the target-format pipeline
    pub(crate) fn prepare(
        &mut self,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        _viewport_x: u32,
        _viewport_y: u32,
        viewport_width: u32,
        viewport_height: u32,
        pixels_per_tile: f32,
        sprites: &SceneGraphicsSprites<'_>,
    ) {
        let device: &wgpu::Device = accelerator.device();
        if !self.pipelines.contains_key(&format) {
            let render_pipeline: wgpu::RenderPipeline = device.create_render_pipeline(
                &wgpu::RenderPipelineDescriptor {
                    label: Some("`SceneRendererSpritesPass.pipelines`"),
                    layout: Some(&self.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.shader_module,
                        entry_point: Some("vertex"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: 56,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &[
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x4,
                                    offset: 0,
                                    shader_location: 0,
                                },
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32,
                                    offset: 16,
                                    shader_location: 1,
                                },
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x4,
                                    offset: 24,
                                    shader_location: 2,
                                },
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x4,
                                    offset: 40,
                                    shader_location: 3,
                                },
                            ],
                        })],
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.shader_module,
                        entry_point: Some("fragment"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                },
            );
            self.pipelines.insert(format, render_pipeline);
        }
        let viewport_width_as_float: f32 = viewport_width as f32;
        let viewport_height_as_float: f32 = viewport_height as f32;
        let mut uniform_bytes: [u8; 16] = [0; 16];
        uniform_bytes[0..4].copy_from_slice(&viewport_width_as_float.to_le_bytes());
        uniform_bytes[4..8].copy_from_slice(&viewport_height_as_float.to_le_bytes());
        uniform_bytes[8..12].copy_from_slice(&pixels_per_tile.to_le_bytes());
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        if self.bind_group.is_none() {
            let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`SceneRendererSpritesPass.bind_group`"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.bind_group = Some(bind_group);
        }
        // select this pass's sprites in depth order, keeping submission order within a depth
        self.sprite_indices.clear();
        self.sprite_indices.extend((0..sprites.sprites.len()).filter(|sprite_index: &usize| {
            self.depths.contains(&sprites.sprites[*sprite_index].6)
        }));
        self.sprite_indices.sort_by_key(|sprite_index: &usize| sprites.sprites[*sprite_index].6);
        self.instance_bytes.clear();
        self.batches.clear();
        for (instance_index, sprite_index) in self.sprite_indices.iter().enumerate() {
            let (texture_view, x, y, width, height, rotation, _, texture_edges, color): (
                &wgpu::TextureView,
                f32,
                f32,
                f32,
                f32,
                f32,
                i32,
                [f32; 4],
                [f32; 4],
            ) = sprites.sprites[*sprite_index];
            for value in [x, y, width, height, rotation, 0.0] {
                self.instance_bytes.extend_from_slice(&value.to_le_bytes());
            }
            for value in texture_edges.into_iter().chain(color) {
                self.instance_bytes.extend_from_slice(&value.to_le_bytes());
            }
            let instance_index: u32 = instance_index as u32;
            if let Some((batch_texture_view, instance_range)) = self.batches.last_mut() {
                if batch_texture_view == texture_view {
                    instance_range.end = instance_index + 1;
                    continue;
                }
            }
            self.batches.push((texture_view.clone(), instance_index..(instance_index + 1)));
        }
        let required_capacity: usize = self.instance_bytes.len();
        if required_capacity > self.instance_buffer_capacity {
            let instance_buffer_capacity: usize = required_capacity.next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`SceneRendererSpritesPass.instance_buffer`"),
                size: instance_buffer_capacity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buffer_capacity = instance_buffer_capacity;
        }
        if !self.instance_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.instance_buffer, 0, &self.instance_bytes);
        }
        let texture_bind_group_layout: &wgpu::BindGroupLayout = &self.texture_bind_group_layout;
        for (texture_view, _) in &self.batches {
            if !self.texture_bind_groups.contains_key(texture_view) {
                let texture_bind_group: wgpu::BindGroup =
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("`SceneRendererSpritesPass.texture_bind_groups`"),
                        layout: texture_bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(texture_view),
                        }],
                    });
                self.texture_bind_groups.insert(texture_view.clone(), texture_bind_group);
            }
        }
        let batches: &Vec<(wgpu::TextureView, Range<u32>)> = &self.batches;
        self.texture_bind_groups.retain(|texture_view, _| {
            batches.iter().any(|(batch_texture_view, _)| batch_texture_view == texture_view)
        });
    }

    /// Draws prepared sprites for one target format
    pub(crate) fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>, format: wgpu::TextureFormat) {
        let render_pipeline: &wgpu::RenderPipeline = match self.pipelines.get(&format) {
            Some(render_pipeline) => render_pipeline,
            None => return,
        };
        let bind_group: &wgpu::BindGroup = match self.bind_group.as_ref() {
            Some(bind_group) => bind_group,
            None => return,
        };
        if self.batches.is_empty() {
            return;
        }
        render_pass.set_pipeline(render_pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        for (texture_view, instance_range) in &self.batches {
            let texture_bind_group: &wgpu::BindGroup =
                match self.texture_bind_groups.get(texture_view) {
                    Some(texture_bind_group) => texture_bind_group,
                    None => continue,
                };
            render_pass.set_bind_group(1, texture_bind_group, &[]);
            render_pass.draw(0..4, instance_range.clone());
        }
    }

}
