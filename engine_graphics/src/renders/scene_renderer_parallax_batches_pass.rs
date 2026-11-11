// Copyright Rob Gage 2026

use super::scene_renderer_lighting_pass::lighting_shader_source;
use crate::world::scenes::{SceneGraphicsCellular, SceneGraphicsParallaxBatch};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Distance between the uniforms of neighboring batches, meeting every device's offset alignment
const BATCH_UNIFORM_STRIDE: usize = 256;

/// Renderer of static world-aligned sprite batches behind the cellular content, sharing the sprite
/// shader and instance layout and using the surface field for shadows cast by solid cells
pub struct SceneRendererParallaxBatchesPass {
    /// Layout of one batch's textures
    texture_bind_group_layout: wgpu::BindGroupLayout,
    /// Layout of the batch uniforms and surface field
    batch_bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline layout
    pipeline_layout: wgpu::PipelineLayout,
    /// Sprite shader
    shader_module: wgpu::ShaderModule,
    /// Pipelines indexed by target format
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// View uniforms
    view_uniform_buffer: wgpu::Buffer,
    /// View uniforms and sampler bound together
    view_bind_group: wgpu::BindGroup,
    /// One block of uniforms per batch
    batch_uniform_buffer: wgpu::Buffer,
    /// Capacity of the batch uniform buffer in batches
    batch_capacity: usize,
    /// Batch uniforms and surface field bound together
    batch_bind_group: Option<wgpu::BindGroup>,
    /// Surface field in the batch bind group
    surface_field_view: Option<wgpu::TextureView>,
    /// Texture bind groups of the prepared batches
    texture_bind_groups: HashMap<(wgpu::TextureView, wgpu::TextureView), wgpu::BindGroup>,
    /// What to draw
    draws: Vec<(wgpu::TextureView, wgpu::TextureView, wgpu::Buffer, u32)>,
}

impl SceneRendererParallaxBatchesPass {

    /// Creates the resources, sampling the lighting group of a layout
    pub fn new(accelerator: &Accelerator, lighting_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let view_bind_group_layout: wgpu::BindGroupLayout = engine_host::acceleration::create_bind_group_layout(device,
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererParallaxBatchesPass.view_bind_group_layout`"),
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
        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let texture_bind_group_layout: wgpu::BindGroupLayout = engine_host::acceleration::create_bind_group_layout(device,
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererParallaxBatchesPass.texture_bind_group_layout`"),
                entries: &[texture_entry(0), texture_entry(1)],
            },
        );
        let batch_bind_group_layout: wgpu::BindGroupLayout = engine_host::acceleration::create_bind_group_layout(device,
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererParallaxBatchesPass.batch_bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(32),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = engine_host::acceleration::create_pipeline_layout(device,
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererParallaxBatchesPass.pipeline_layout`"),
                bind_group_layouts: &[
                    Some(&view_bind_group_layout),
                    Some(&texture_bind_group_layout),
                    Some(lighting_bind_group_layout),
                    Some(&batch_bind_group_layout),
                ],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = engine_host::acceleration::create_shader_module(device,
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererParallaxBatchesPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    lighting_shader_source(2, include_str!("scene_renderer_sprites.wgsl")).into(),
                ),
            },
        );
        let sampler: wgpu::Sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("`SceneRendererParallaxBatchesPass.sampler`"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let view_uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererParallaxBatchesPass.view_uniform_buffer`"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`SceneRendererParallaxBatchesPass.view_bind_group`"),
            layout: &view_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: view_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let batch_capacity: usize = 64;
        let batch_uniform_buffer: wgpu::Buffer = Self::create_batch_uniform_buffer(device, batch_capacity);
        Self {
            texture_bind_group_layout,
            batch_bind_group_layout,
            pipeline_layout,
            shader_module,
            pipelines: HashMap::new(),
            view_uniform_buffer,
            view_bind_group,
            batch_uniform_buffer,
            batch_capacity,
            batch_bind_group: None,
            surface_field_view: None,
            texture_bind_groups: HashMap::new(),
            draws: Vec::new(),
        }
    }

    /// Creates uniforms holding a number of batches
    fn create_batch_uniform_buffer(device: &wgpu::Device, batches: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererParallaxBatchesPass.batch_uniform_buffer`"),
            size: (batches * BATCH_UNIFORM_STRIDE) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Prepares the batches of one view for drawing over a cellular region and its surface field
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        viewport_width: u32,
        viewport_height: u32,
        pixels_per_tile: f32,
        batches: &[SceneGraphicsParallaxBatch<'_>],
        cellular: &SceneGraphicsCellular<'_>,
        surface_field_view: &wgpu::TextureView,
        contact_shadows: bool,
    ) {
        let device: &wgpu::Device = accelerator.device();
        self.draws.clear();
        if batches.is_empty() { return; }
        if !self.pipelines.contains_key(&format) {
            let render_pipeline: wgpu::RenderPipeline = self.create_pipeline(device, format);
            self.pipelines.insert(format, render_pipeline);
        }
        let mut view_bytes: [u8; 16] = [0; 16];
        view_bytes[0..4].copy_from_slice(&(viewport_width as f32).to_le_bytes());
        view_bytes[4..8].copy_from_slice(&(viewport_height as f32).to_le_bytes());
        view_bytes[8..12].copy_from_slice(&pixels_per_tile.to_le_bytes());
        accelerator.queue().write_buffer(&self.view_uniform_buffer, 0, &view_bytes);
        let mut rebind: bool = self.surface_field_view.as_ref() != Some(surface_field_view);
        if batches.len() > self.batch_capacity {
            self.batch_capacity = batches.len().next_power_of_two();
            self.batch_uniform_buffer = Self::create_batch_uniform_buffer(device, self.batch_capacity);
            rebind = true;
        }
        if rebind || self.batch_bind_group.is_none() {
            self.batch_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`SceneRendererParallaxBatchesPass.batch_bind_group`"),
                layout: &self.batch_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.batch_uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(32),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(surface_field_view),
                    },
                ],
            }));
            self.surface_field_view = Some(surface_field_view.clone());
        }
        let mut uniform_bytes: Vec<u8> = vec![0; batches.len() * BATCH_UNIFORM_STRIDE];
        for (index, batch) in batches.iter().enumerate() {
            let start: usize = index * BATCH_UNIFORM_STRIDE;
            let cell_side: i64 = i64::from(cellular.cellular_tile_size);
            let floats: [f32; 8] = [
                batch.origin[0],
                batch.origin[1],
                cellular.region_x,
                cellular.region_y,
                cellular.cellular_tile_size as f32,
                if contact_shadows { 0.6 } else { 0.0 },
                (cellular.region_tile_x * cell_side)
                    .rem_euclid(i64::from(cellular.region_width) * cell_side) as f32,
                (cellular.region_tile_y * cell_side)
                    .rem_euclid(i64::from(cellular.region_height) * cell_side) as f32,
            ];
            for (float_index, value) in floats.iter().enumerate() {
                uniform_bytes[start + float_index * 4..start + float_index * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
            let textures: (wgpu::TextureView, wgpu::TextureView) =
                (batch.texture.clone(), batch.radiance_texture.clone());
            if !self.texture_bind_groups.contains_key(&textures) {
                let texture_bind_group: wgpu::BindGroup =
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("`SceneRendererParallaxBatchesPass.texture_bind_groups`"),
                        layout: &self.texture_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(batch.texture),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(batch.radiance_texture),
                            },
                        ],
                    });
                self.texture_bind_groups.insert(textures, texture_bind_group);
            }
            self.draws.push((
                batch.texture.clone(),
                batch.radiance_texture.clone(),
                batch.instance_buffer.clone(),
                batch.instance_count,
            ));
        }
        accelerator.queue().write_buffer(&self.batch_uniform_buffer, 0, &uniform_bytes);
        let draws = &self.draws;
        self.texture_bind_groups.retain(|(texture, radiance), _| {
            draws.iter().any(|draw| &draw.0 == texture && &draw.1 == radiance)
        });
    }

    /// Creates the pipeline of a target format
    fn create_pipeline(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        engine_host::acceleration::create_render_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some("`SceneRendererParallaxBatchesPass.pipelines`"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader_module,
                entry_point: Some("vertex_batch"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 80,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
                        1 => Float32,
                        4 => Float32,
                        2 => Float32x4,
                        3 => Float32x4,
                        5 => Float32x4,
                        6 => Float32,
                    ],
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
                entry_point: Some("fragment_batch"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Draws the prepared batches
    pub fn draw(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        lighting_bind_group: &wgpu::BindGroup,
    ) {
        let (Some(pipeline), Some(batch_bind_group)) =
            (self.pipelines.get(&format), self.batch_bind_group.as_ref())
        else { return; };
        if self.draws.is_empty() { return; }
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, &self.view_bind_group, &[]);
        render_pass.set_bind_group(2, lighting_bind_group, &[]);
        for (index, (texture, radiance, instance_buffer, instance_count)) in
            self.draws.iter().enumerate()
        {
            let Some(texture_bind_group) =
                self.texture_bind_groups.get(&(texture.clone(), radiance.clone()))
            else { continue; };
            render_pass.set_bind_group(1, texture_bind_group, &[]);
            render_pass.set_bind_group(
                3,
                batch_bind_group,
                &[(index * BATCH_UNIFORM_STRIDE) as u32],
            );
            render_pass.set_vertex_buffer(0, instance_buffer.slice(..));
            render_pass.draw(0..4, 0..*instance_count);
        }
    }

}
