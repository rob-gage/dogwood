// Copyright Rob Gage 2026

use crate::world::{
    materials::MATERIAL_RADIANCE_WGSL,
    scenes::{
        SceneGraphics,
        SceneGraphicsParallax,
    },
};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Length of the finest cascade intervals in cells
const LIGHTING_INTERVAL: f32 = 2.0;

/// Directions traced by the finest cascade level
const LIGHTING_DIRECTIONS: u32 = 4;

/// Cells along each side of one backlight texel
const LIGHTING_BACKLIGHT_CELLS: u32 = 4;

/// Bytes between the dynamic uniform records of cascade levels and spread passes
const LIGHTING_UNIFORM_STRIDE: u64 = 256;

/// Largest number of cascade levels
const LIGHTING_LEVEL_MAXIMUM: usize = 16;

/// Bytes of one sprite instance, matching the sprite pass
const LIGHTING_INSTANCE_SIZE: usize = 64;

/// One cascade level's placement in the lighting domain
#[derive(Clone, Copy, Debug)]
struct LightingLevel {
    /// Number of probes along each axis
    probe_count: [u32; 2],
    /// Domain position of the first probe center in cells
    probe_origin: [f32; 2],
    /// Distance between neighboring probes in cells
    spacing: f32,
    /// Number of directions traced from each probe
    direction_count: u32,
    /// Distance from a probe where its intervals start in cells
    interval_start: f32,
    /// Distance from a probe where its intervals end in cells
    interval_end: f32,
}

/// Resources sized to one lighting domain
struct LightingDomain {
    /// Domain width in cells
    width: u32,
    /// Domain height in cells
    height: u32,
    /// Emission and transmission per cell of actors and lights
    actor_optics_view: wgpu::TextureView,
    /// Light arriving from the backgrounds
    backlight_view: wgpu::TextureView,
    /// Emission and extinction per cell of everything in the domain
    optical_view: wgpu::TextureView,
    /// Uniform block classification bindings
    block_bind_group: wgpu::BindGroup,
    /// Spread light with every mip level
    light_view: wgpu::TextureView,
    /// Number of mip levels of the light texture
    light_mip_count: u32,
    /// Light texture mip downsampling bind groups, from the second level
    mip_bind_groups: Vec<wgpu::BindGroup>,
    /// Light texture mip sizes, from the second level
    mip_sizes: Vec<[u32; 2]>,
    /// Cascade bind groups writing the first and the second interval buffer
    cascade_bind_groups: [wgpu::BindGroup; 2],
    /// Spread bind groups of the row, column, and final row passes
    spread_bind_groups: [wgpu::BindGroup; 3],
}

/// Computes the light of a scene's resident region and shares it with the appearance passes
pub struct SceneRendererLightingPass {
    /// Layout of the lighting group sampled by the appearance passes
    lighting_bind_group_layout: wgpu::BindGroupLayout,
    /// Lighting group of the prepared view
    lighting_bind_group: Option<wgpu::BindGroup>,
    /// Lighting view uniform shared by the appearance passes
    view_buffer: wgpu::Buffer,
    /// Linear sampler of the light texture
    light_sampler: wgpu::Sampler,
    /// Light texture sampled while no domain is lit
    empty_light_view: wgpu::TextureView,
    /// Storage buffer bound in place of absent scene buffers
    empty_buffer: wgpu::Buffer,
    /// Layout of one sprite radiance texture
    sprite_texture_bind_group_layout: wgpu::BindGroupLayout,
    /// Sprite rasterization uniforms
    sprite_bind_group: wgpu::BindGroup,
    /// Domain placement of the sprite rasterizations
    sprite_buffer: wgpu::Buffer,
    /// Pipeline adding gameplay sprite emission and transmission
    actor_pipeline: wgpu::RenderPipeline,
    /// Pipeline adding light emission
    light_pipeline: wgpu::RenderPipeline,
    /// Pipeline compositing background emission
    background_pipeline: wgpu::RenderPipeline,
    /// Instances of gameplay sprites, then lights, then backgrounds
    instance_buffer: wgpu::Buffer,
    /// Capacity of the instance buffer in bytes
    instance_buffer_capacity: usize,
    /// Prepared instance bytes
    instance_bytes: Vec<u8>,
    /// Radiance texture and instance range of every gameplay sprite batch
    actor_batches: Vec<(wgpu::TextureView, std::ops::Range<u32>)>,
    /// Instance range of the lights
    light_instances: std::ops::Range<u32>,
    /// Radiance texture and instance range of every background batch
    background_batches: Vec<(wgpu::TextureView, std::ops::Range<u32>)>,
    /// Radiance texture bind groups of the prepared batches
    texture_bind_groups: HashMap<wgpu::TextureView, wgpu::BindGroup>,
    /// Layout of the optical field construction
    optical_bind_group_layout: wgpu::BindGroupLayout,
    /// Optical field construction pipeline
    optical_pipeline: wgpu::ComputePipeline,
    /// Optical field construction uniforms
    optical_buffer: wgpu::Buffer,
    /// Optical field construction bindings of the prepared view
    optical_bind_group: Option<wgpu::BindGroup>,
    /// Layout of the cascade passes
    cascade_bind_group_layout: wgpu::BindGroupLayout,
    /// Cascade tracing and merging pipeline
    trace_pipeline: wgpu::ComputePipeline,
    /// Cascade integration pipeline
    integrate_pipeline: wgpu::ComputePipeline,
    /// Cascade level uniforms at `LIGHTING_UNIFORM_STRIDE`
    cascade_buffer: wgpu::Buffer,
    /// Prepared cascade levels, the finest first
    levels: Vec<LightingLevel>,
    /// Layout of the spread passes
    spread_bind_group_layout: wgpu::BindGroupLayout,
    /// Spread pipeline
    spread_pipeline: wgpu::ComputePipeline,
    /// Spread pass uniforms at `LIGHTING_UNIFORM_STRIDE`
    spread_buffer: wgpu::Buffer,
    /// Layout of the uniform block classification
    block_bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform block classification pipeline
    block_pipeline: wgpu::ComputePipeline,
    /// Layout of the light mip downsampling
    mip_bind_group_layout: wgpu::BindGroupLayout,
    /// Light mip downsampling pipeline
    mip_pipeline: wgpu::ComputePipeline,
    /// Resources of the current domain
    domain: Option<LightingDomain>,
    /// Whether the prepared view lights a domain
    is_lit: bool,
}

/// Returns an appearance shader's source with the lighting functions bound at a group prepended
pub fn lighting_shader_source(lighting_group: u32, source: &str) -> String {
    [
        MATERIAL_RADIANCE_WGSL,
        &format!("const LIGHTING_GROUP: u32 = {lighting_group}u;"),
        include_str!("scene_renderer_lighting.wgsl"),
        source,
    ].join("\n")
}

/// Returns a bind group layout entry for a buffer
fn buffer_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BufferBindingType,
    has_dynamic_offset: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset, min_binding_size: None },
        count: None,
    }
}

/// Returns a bind group layout entry for a filterable float texture
fn texture_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// Returns a bind group layout entry for a write-only `Rgba16Float` storage texture
fn storage_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba16Float,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}

/// Returns a bind group entry for a whole buffer
fn buffer_binding(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource: buffer.as_entire_binding() }
}

/// Returns a bind group entry for one dynamic uniform record
fn uniform_record_binding(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer,
            offset: 0,
            size: wgpu::BufferSize::new(64),
        }),
    }
}

/// Returns the number of workgroups covering a count, split across two dimensions
fn workgroup_counts(count: u32, workgroup_size: u32) -> (u32, u32) {
    let workgroups: u32 = count.div_ceil(workgroup_size).max(1);
    let x: u32 = workgroups.min(65535);
    (x, workgroups.div_ceil(x))
}

impl SceneRendererLightingPass {

    /// Creates the lighting resources
    pub fn new(accelerator: &Accelerator) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let fragment_and_compute: wgpu::ShaderStages =
            wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE;
        let lighting_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.lighting_bind_group_layout`"),
                entries: &[
                    buffer_entry(0, fragment_and_compute, wgpu::BufferBindingType::Uniform, false),
                    texture_entry(1, fragment_and_compute),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: fragment_and_compute,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    buffer_entry(
                        3,
                        fragment_and_compute,
                        wgpu::BufferBindingType::Storage { read_only: true },
                        false,
                    ),
                ],
            },
        );
        let view_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererLightingPass.view_buffer`"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let light_sampler: wgpu::Sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("`SceneRendererLightingPass.light_sampler`"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let radiance_sampler: wgpu::Sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("`SceneRendererLightingPass.radiance_sampler`"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let empty_light_view: wgpu::TextureView = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("`SceneRendererLightingPass.empty_light_view`"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }).create_view(&wgpu::TextureViewDescriptor::default());
        let empty_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererLightingPass.empty_buffer`"),
            size: 64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        // sprite rasterization
        let sprite_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.sprite_bind_group_layout`"),
                entries: &[
                    buffer_entry(
                        0,
                        wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        wgpu::BufferBindingType::Uniform,
                        false,
                    ),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            },
        );
        let sprite_texture_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.sprite_texture_bind_group_layout`"),
                entries: &[texture_entry(
                    0,
                    wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                )],
            });
        let sprite_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererLightingPass.sprite_buffer`"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sprite_bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`SceneRendererLightingPass.sprite_bind_group`"),
            layout: &sprite_bind_group_layout,
            entries: &[
                buffer_binding(0, &sprite_buffer),
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&radiance_sampler),
                },
            ],
        });
        let sprite_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.sprite_pipeline_layout`"),
                bind_group_layouts: &[
                    Some(&sprite_bind_group_layout),
                    Some(&sprite_texture_bind_group_layout),
                ],
                immediate_size: 0,
            },
        );
        let sprite_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererLightingPass.sprite_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("scene_renderer_lighting_sprites.wgsl").into(),
                ),
            },
        );
        let additive: wgpu::BlendState = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            // transmission per cell multiplies
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let over: wgpu::BlendState = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let sprite_pipeline = |entry_point: &str, blend: wgpu::BlendState| -> wgpu::RenderPipeline {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("`SceneRendererLightingPass.sprite_pipelines`"),
                layout: Some(&sprite_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &sprite_module,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: LIGHTING_INSTANCE_SIZE as u64,
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
                                format: wgpu::VertexFormat::Float32,
                                offset: 20,
                                shader_location: 2,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 24,
                                shader_location: 3,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 40,
                                shader_location: 4,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32,
                                offset: 56,
                                shader_location: 5,
                            },
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
                    module: &sprite_module,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let actor_pipeline: wgpu::RenderPipeline = sprite_pipeline("actor_fragment", additive);
        let light_pipeline: wgpu::RenderPipeline = sprite_pipeline("light_fragment", additive);
        let background_pipeline: wgpu::RenderPipeline = sprite_pipeline("background_fragment", over);
        let instance_buffer_capacity: usize = 256 * LIGHTING_INSTANCE_SIZE;
        let instance_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererLightingPass.instance_buffer`"),
            size: instance_buffer_capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // optical field
        let read_storage: wgpu::BufferBindingType = wgpu::BufferBindingType::Storage { read_only: true };
        let optical_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.optical_bind_group_layout`"),
                entries: &[
                    buffer_entry(0, wgpu::ShaderStages::COMPUTE, wgpu::BufferBindingType::Uniform, false),
                    buffer_entry(1, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(2, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(3, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(4, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(5, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(6, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(7, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(8, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(9, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    texture_entry(10, wgpu::ShaderStages::COMPUTE),
                    storage_texture_entry(11),
                ],
            },
        );
        let optical_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.optical_pipeline_layout`"),
                bind_group_layouts: &[Some(&optical_bind_group_layout), Some(&lighting_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let optical_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererLightingPass.optical_module`"),
                source: wgpu::ShaderSource::Wgsl(lighting_shader_source(
                    1,
                    include_str!("scene_renderer_lighting_optical.wgsl"),
                ).into()),
            },
        );
        let compute_pipeline = |
            label: &str,
            layout: &wgpu::PipelineLayout,
            module: &wgpu::ShaderModule,
            entry_point: &str,
        | -> wgpu::ComputePipeline {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let optical_pipeline: wgpu::ComputePipeline = compute_pipeline(
            "`SceneRendererLightingPass.optical_pipeline`",
            &optical_pipeline_layout,
            &optical_module,
            "build",
        );
        let optical_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererLightingPass.optical_buffer`"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // cascades
        let read_write_storage: wgpu::BufferBindingType =
            wgpu::BufferBindingType::Storage { read_only: false };
        let cascade_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.cascade_bind_group_layout`"),
                entries: &[
                    buffer_entry(0, wgpu::ShaderStages::COMPUTE, wgpu::BufferBindingType::Uniform, true),
                    texture_entry(1, wgpu::ShaderStages::COMPUTE),
                    buffer_entry(2, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(3, wgpu::ShaderStages::COMPUTE, read_write_storage, false),
                    buffer_entry(4, wgpu::ShaderStages::COMPUTE, read_write_storage, false),
                    texture_entry(5, wgpu::ShaderStages::COMPUTE),
                ],
            },
        );
        let cascade_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.cascade_pipeline_layout`"),
                bind_group_layouts: &[Some(&cascade_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let cascade_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererLightingPass.cascade_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("scene_renderer_lighting_cascades.wgsl").into(),
                ),
            },
        );
        let trace_pipeline: wgpu::ComputePipeline = compute_pipeline(
            "`SceneRendererLightingPass.trace_pipeline`",
            &cascade_pipeline_layout,
            &cascade_module,
            "trace_merge",
        );
        let integrate_pipeline: wgpu::ComputePipeline = compute_pipeline(
            "`SceneRendererLightingPass.integrate_pipeline`",
            &cascade_pipeline_layout,
            &cascade_module,
            "integrate",
        );
        let cascade_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererLightingPass.cascade_buffer`"),
            size: LIGHTING_UNIFORM_STRIDE * LIGHTING_LEVEL_MAXIMUM as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // spread
        let spread_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.spread_bind_group_layout`"),
                entries: &[
                    buffer_entry(0, wgpu::ShaderStages::COMPUTE, wgpu::BufferBindingType::Uniform, true),
                    texture_entry(1, wgpu::ShaderStages::COMPUTE),
                    buffer_entry(2, wgpu::ShaderStages::COMPUTE, read_storage, false),
                    buffer_entry(3, wgpu::ShaderStages::COMPUTE, read_write_storage, false),
                    storage_texture_entry(4),
                    texture_entry(5, wgpu::ShaderStages::COMPUTE),
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            },
        );
        let spread_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.spread_pipeline_layout`"),
                bind_group_layouts: &[Some(&spread_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let spread_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererLightingPass.spread_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("scene_renderer_lighting_spread.wgsl").into(),
                ),
            },
        );
        let spread_pipeline: wgpu::ComputePipeline = compute_pipeline(
            "`SceneRendererLightingPass.spread_pipeline`",
            &spread_pipeline_layout,
            &spread_module,
            "spread_lines",
        );
        let spread_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererLightingPass.spread_buffer`"),
            size: LIGHTING_UNIFORM_STRIDE * 3,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // uniform blocks
        let block_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.block_bind_group_layout`"),
                entries: &[texture_entry(0, wgpu::ShaderStages::COMPUTE), storage_texture_entry(1)],
            },
        );
        let block_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.block_pipeline_layout`"),
                bind_group_layouts: &[Some(&block_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let block_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererLightingPass.block_module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("scene_renderer_lighting_blocks.wgsl").into()),
            },
        );
        let block_pipeline: wgpu::ComputePipeline = compute_pipeline(
            "`SceneRendererLightingPass.block_pipeline`",
            &block_pipeline_layout,
            &block_module,
            "classify",
        );
        // light mips
        let mip_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.mip_bind_group_layout`"),
                entries: &[texture_entry(0, wgpu::ShaderStages::COMPUTE), storage_texture_entry(1)],
            },
        );
        let mip_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererLightingPass.mip_pipeline_layout`"),
                bind_group_layouts: &[Some(&mip_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let mip_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererLightingPass.mip_module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("scene_renderer_lighting_mips.wgsl").into()),
            },
        );
        let mip_pipeline: wgpu::ComputePipeline = compute_pipeline(
            "`SceneRendererLightingPass.mip_pipeline`",
            &mip_pipeline_layout,
            &mip_module,
            "downsample",
        );
        Self {
            lighting_bind_group_layout,
            lighting_bind_group: None,
            view_buffer,
            light_sampler,
            empty_light_view,
            empty_buffer,
            sprite_texture_bind_group_layout,
            sprite_bind_group,
            sprite_buffer,
            actor_pipeline,
            light_pipeline,
            background_pipeline,
            instance_buffer,
            instance_buffer_capacity,
            instance_bytes: Vec::new(),
            actor_batches: Vec::new(),
            light_instances: 0..0,
            background_batches: Vec::new(),
            texture_bind_groups: HashMap::new(),
            optical_bind_group_layout,
            optical_pipeline,
            optical_buffer,
            optical_bind_group: None,
            cascade_bind_group_layout,
            trace_pipeline,
            integrate_pipeline,
            cascade_buffer,
            levels: Vec::new(),
            spread_bind_group_layout,
            spread_pipeline,
            spread_buffer,
            block_bind_group_layout,
            block_pipeline,
            mip_bind_group_layout,
            mip_pipeline,
            domain: None,
            is_lit: false,
        }
    }

    /// Returns the layout of the lighting group the appearance passes bind
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout { &self.lighting_bind_group_layout }

    /// Returns the lighting group of the prepared view
    pub fn bind_group(&self) -> Option<&wgpu::BindGroup> { self.lighting_bind_group.as_ref() }

    /// Creates the resources of a domain size
    fn create_domain(&self, accelerator: &Accelerator, width: u32, height: u32) -> LightingDomain {
        let device: &wgpu::Device = accelerator.device();
        let texture = |label: &str, width: u32, height: u32, mip_level_count: u32,
            usage: wgpu::TextureUsages| -> wgpu::Texture {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage,
                view_formats: &[],
            })
        };
        let attachment: wgpu::TextureUsages =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let storage: wgpu::TextureUsages =
            wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        let actor_optics_view: wgpu::TextureView =
            texture("`LightingDomain.actor_optics_view`", width, height, 1, attachment)
                .create_view(&wgpu::TextureViewDescriptor::default());
        let backlight_view: wgpu::TextureView = texture(
            "`LightingDomain.backlight_view`",
            width.div_ceil(LIGHTING_BACKLIGHT_CELLS),
            height.div_ceil(LIGHTING_BACKLIGHT_CELLS),
            1,
            attachment,
        ).create_view(&wgpu::TextureViewDescriptor::default());
        let optical_view: wgpu::TextureView =
            texture("`LightingDomain.optical_view`", width, height, 1, storage)
                .create_view(&wgpu::TextureViewDescriptor::default());
        let block_view: wgpu::TextureView = texture(
            "`LightingDomain.block_view`",
            width.div_ceil(8),
            height.div_ceil(8),
            1,
            storage,
        ).create_view(&wgpu::TextureViewDescriptor::default());
        let block_bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`LightingDomain.block_bind_group`"),
            layout: &self.block_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&optical_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&block_view),
                },
            ],
        });
        let light_mip_count: u32 = 32 - width.max(height).leading_zeros();
        let light_texture: wgpu::Texture =
            texture("`LightingDomain.light_view`", width, height, light_mip_count, storage);
        let light_view: wgpu::TextureView =
            light_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mip_view = |level: u32| -> wgpu::TextureView {
            light_texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let mut mip_bind_groups: Vec<wgpu::BindGroup> = Vec::new();
        let mut mip_sizes: Vec<[u32; 2]> = Vec::new();
        for level in 1..light_mip_count {
            mip_bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`LightingDomain.mip_bind_groups`"),
                layout: &self.mip_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&mip_view(level - 1)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&mip_view(level)),
                    },
                ],
            }));
            mip_sizes.push([(width >> level).max(1), (height >> level).max(1)]);
        }
        let light_buffer = || -> wgpu::Buffer {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`LightingDomain.light_buffers`"),
                size: u64::from(width) * u64::from(height) * 8,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        let light_buffers: [wgpu::Buffer; 3] = [light_buffer(), light_buffer(), light_buffer()];
        // the second level has the most intervals, and each later level about half as many
        let interval_count: u64 = (u64::from(width) / 2 + 4) * (u64::from(height) / 2 + 4)
            * u64::from(LIGHTING_DIRECTIONS * 2);
        let interval_buffer = || -> wgpu::Buffer {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`LightingDomain.interval_buffers`"),
                size: interval_count * 8,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        let interval_buffers: [wgpu::Buffer; 2] = [interval_buffer(), interval_buffer()];
        let cascade_bind_group = |output: usize| -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`LightingDomain.cascade_bind_groups`"),
                layout: &self.cascade_bind_group_layout,
                entries: &[
                    uniform_record_binding(0, &self.cascade_buffer),
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&optical_view),
                    },
                    buffer_binding(2, &interval_buffers[1 - output]),
                    buffer_binding(3, &interval_buffers[output]),
                    buffer_binding(4, &light_buffers[0]),
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&block_view),
                    },
                ],
            })
        };
        let cascade_bind_groups: [wgpu::BindGroup; 2] = [cascade_bind_group(0), cascade_bind_group(1)];
        let light_storage_view: wgpu::TextureView = mip_view(0);
        let spread_bind_group = |input: usize, output: usize| -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`LightingDomain.spread_bind_groups`"),
                layout: &self.spread_bind_group_layout,
                entries: &[
                    uniform_record_binding(0, &self.spread_buffer),
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&optical_view),
                    },
                    buffer_binding(2, &light_buffers[input]),
                    buffer_binding(3, &light_buffers[output]),
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&light_storage_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&backlight_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&self.light_sampler),
                    },
                ],
            })
        };
        let spread_bind_groups: [wgpu::BindGroup; 3] =
            [spread_bind_group(0, 1), spread_bind_group(1, 2), spread_bind_group(2, 1)];
        LightingDomain {
            width,
            height,
            actor_optics_view,
            backlight_view,
            optical_view,
            block_bind_group,
            light_view,
            light_mip_count,
            mip_bind_groups,
            mip_sizes,
            cascade_bind_groups,
            spread_bind_groups,
        }
    }

    /// Returns the cascade levels of a domain whose bottom-left cell has a world position,
    /// keeping every level's probes at fixed world positions as the domain moves
    fn domain_levels(width: u32, height: u32, world_origin: [i64; 2]) -> Vec<LightingLevel> {
        let diagonal: f32 = (width as f32).hypot(height as f32);
        let mut levels: Vec<LightingLevel> = Vec::new();
        for level in 0..LIGHTING_LEVEL_MAXIMUM as u32 {
            let spacing: i64 = 1 << level;
            let mut probe_count: [u32; 2] = [width, height];
            let mut probe_origin: [f32; 2] = [0.5, 0.5];
            if level > 0 {
                for axis in 0..2 {
                    // probe centers sit at world cells `k * spacing + spacing / 2`, starting one
                    // spacing outside the domain
                    let first: i64 = (world_origin[axis] - spacing).div_euclid(spacing);
                    let origin: f32 =
                        (first * spacing - world_origin[axis]) as f32 + spacing as f32 / 2.0;
                    let size: f32 = [width, height][axis] as f32;
                    probe_origin[axis] = origin;
                    probe_count[axis] = ((size + spacing as f32 - origin) / spacing as f32) as u32 + 1;
                }
            }
            let interval_start: f32 = LIGHTING_INTERVAL * ((1u32 << level) - 1) as f32;
            let interval_end: f32 = LIGHTING_INTERVAL * ((2u32 << level) - 1) as f32;
            levels.push(LightingLevel {
                probe_count,
                probe_origin,
                spacing: spacing as f32,
                direction_count: LIGHTING_DIRECTIONS << level,
                interval_start,
                interval_end,
            });
            if interval_end >= diagonal {
                break;
            }
        }
        levels
    }

    /// Prepares the lighting of one view: uploads its uniforms and instances and binds its scene
    /// buffers, where `backgrounds` are the visible parallax copies sorted far to near
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        graphics: &SceneGraphics<'_>,
        backgrounds: &[SceneGraphicsParallax<'_>],
    ) {
        let device: &wgpu::Device = accelerator.device();
        let queue: &wgpu::Queue = accelerator.queue();
        let lighting = &graphics.lighting;
        let cellular = graphics.cellular.as_ref().filter(|cellular| {
            cellular.region_width > 0 && cellular.region_height > 0 && cellular.cellular_tile_size > 0
        });
        let temperature = graphics.temperature.as_ref();
        let maximum_dimension: u32 = device.limits().max_texture_dimension_2d;
        let domain_size: Option<[u32; 2]> = cellular.and_then(|cellular| {
            let width: u32 = cellular.region_width * cellular.cellular_tile_size;
            let height: u32 = cellular.region_height * cellular.cellular_tile_size;
            (width <= maximum_dimension && height <= maximum_dimension && temperature.is_some())
                .then_some([width, height])
        });
        self.is_lit = domain_size.is_some();
        let cellular_tile_size: f32 = graphics.cellular_tile_size.max(1) as f32;
        let mut view_bytes: [u8; 64] = [0; 64];
        let mut domain_origin: [f32; 2] = [0.0; 2];
        if let (Some([width, height]), Some(cellular)) = (domain_size, cellular) {
            if self.domain.as_ref().is_none_or(|domain| domain.width != width || domain.height != height) {
                self.domain = Some(self.create_domain(accelerator, width, height));
            }
            domain_origin = [cellular.region_x, cellular.region_y];
            view_bytes[0..4].copy_from_slice(&(width as f32).to_le_bytes());
            view_bytes[4..8].copy_from_slice(&(height as f32).to_le_bytes());
        }
        view_bytes[8..12].copy_from_slice(&domain_origin[0].to_le_bytes());
        view_bytes[12..16].copy_from_slice(&domain_origin[1].to_le_bytes());
        for (channel, value) in lighting.ambient.iter().enumerate() {
            view_bytes[16 + channel * 4..20 + channel * 4].copy_from_slice(&value.to_le_bytes());
        }
        view_bytes[28..32].copy_from_slice(&cellular_tile_size.to_le_bytes());
        for (form, offset) in lighting.thermal_offsets.iter().enumerate() {
            view_bytes[32 + form * 4..36 + form * 4].copy_from_slice(&offset.to_le_bytes());
        }
        view_bytes[48..52].copy_from_slice(&u32::from(self.is_lit).to_le_bytes());
        let light_mip_count: f32 = self.domain.as_ref()
            .filter(|_| self.is_lit)
            .map_or(1.0, |domain| domain.light_mip_count as f32);
        view_bytes[52..56].copy_from_slice(&light_mip_count.to_le_bytes());
        queue.write_buffer(&self.view_buffer, 0, &view_bytes);
        let light_view: &wgpu::TextureView = match self.domain.as_ref().filter(|_| self.is_lit) {
            Some(domain) => &domain.light_view,
            None => &self.empty_light_view,
        };
        self.lighting_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`SceneRendererLightingPass.lighting_bind_group`"),
            layout: &self.lighting_bind_group_layout,
            entries: &[
                buffer_binding(0, &self.view_buffer),
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(light_view) },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.light_sampler),
                },
                buffer_binding(3, lighting.temperature_color_buffer),
            ],
        }));
        let (Some([width, height]), Some(cellular), Some(temperature)) = (domain_size, cellular, temperature)
        else {
            self.optical_bind_group = None;
            return;
        };
        let Some(domain) = self.domain.as_ref() else { return; };
        // sprite rasterization
        let mut sprite_bytes: [u8; 32] = [0; 32];
        sprite_bytes[0..4].copy_from_slice(&domain_origin[0].to_le_bytes());
        sprite_bytes[4..8].copy_from_slice(&domain_origin[1].to_le_bytes());
        sprite_bytes[8..12].copy_from_slice(&(width as f32 / cellular_tile_size).to_le_bytes());
        sprite_bytes[12..16].copy_from_slice(&(height as f32 / cellular_tile_size).to_le_bytes());
        sprite_bytes[16..20].copy_from_slice(&cellular_tile_size.to_le_bytes());
        queue.write_buffer(&self.sprite_buffer, 0, &sprite_bytes);
        self.instance_bytes.clear();
        self.actor_batches.clear();
        self.background_batches.clear();
        let mut instance_count: u32 = 0;
        // placement, rotation, depth, color, radiance region, and radiance intensity
        let push_instance = |bytes: &mut Vec<u8>, values: [f32; 16]| {
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        };
        for &(
            _, radiance_view, x, y, sprite_width, sprite_height, rotation, _,
            _, radiance_region, color, radiance_intensity,
        ) in &graphics.sprites.sprites
        {
            push_instance(&mut self.instance_bytes, [
                x, y, sprite_width, sprite_height, rotation, 0.0,
                color[0], color[1], color[2], color[3],
                radiance_region[0], radiance_region[1], radiance_region[2], radiance_region[3],
                radiance_intensity, 0.0,
            ]);
            match self.actor_batches.last_mut() {
                Some((view, range)) if view == radiance_view => range.end = instance_count + 1,
                _ => self.actor_batches.push((radiance_view.clone(), instance_count..instance_count + 1)),
            }
            instance_count += 1;
        }
        let light_start: u32 = instance_count;
        for light in &lighting.lights {
            push_instance(&mut self.instance_bytes, [
                light.center[0], light.center[1], light.radius * 2.0, light.radius * 2.0, 0.0, 0.0,
                light.emission[0], light.emission[1], light.emission[2], 0.0,
                0.0, 0.0, 1.0, 1.0,
                1.0, 0.0,
            ]);
            instance_count += 1;
        }
        self.light_instances = light_start..instance_count;
        for background in backgrounds {
            push_instance(&mut self.instance_bytes, [
                background.center[0], background.center[1], background.size[0], background.size[1],
                background.rotation, background.depth,
                background.color[0], background.color[1], background.color[2], background.color[3],
                background.radiance_texture_region[0], background.radiance_texture_region[1],
                background.radiance_texture_region[2], background.radiance_texture_region[3],
                background.radiance_intensity, 0.0,
            ]);
            match self.background_batches.last_mut() {
                Some((view, range)) if view == background.radiance_texture => range.end = instance_count + 1,
                _ => self.background_batches
                    .push((background.radiance_texture.clone(), instance_count..instance_count + 1)),
            }
            instance_count += 1;
        }
        if self.instance_bytes.len() > self.instance_buffer_capacity {
            self.instance_buffer_capacity = self.instance_bytes.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`SceneRendererLightingPass.instance_buffer`"),
                size: self.instance_buffer_capacity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !self.instance_bytes.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, &self.instance_bytes);
        }
        let empty_light_view: &wgpu::TextureView = &self.empty_light_view;
        let views: Vec<&wgpu::TextureView> = self.actor_batches.iter()
            .chain(&self.background_batches)
            .map(|(view, _)| view)
            .chain([empty_light_view])
            .collect();
        for view in &views {
            if !self.texture_bind_groups.contains_key(*view) {
                let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("`SceneRendererLightingPass.texture_bind_groups`"),
                    layout: &self.sprite_texture_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    }],
                });
                self.texture_bind_groups.insert((*view).clone(), bind_group);
            }
        }
        self.texture_bind_groups.retain(|view, _| views.contains(&view));
        // optical field
        let fluid = graphics.fluid.as_ref();
        let gas = graphics.gas.as_ref();
        let mut optical_bytes: [u8; 48] = [0; 48];
        for (index, value) in [
            width,
            height,
            cellular.region_width,
            cellular.region_height,
            cellular.ring_offset_x,
            cellular.ring_offset_y,
            cellular.cellular_tile_size,
        ].into_iter().enumerate() {
            optical_bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        let amount_per_atmosphere: f32 = gas.map_or(1.0, |gas| gas.amount_per_atmosphere);
        optical_bytes[28..32].copy_from_slice(&amount_per_atmosphere.to_le_bytes());
        optical_bytes[32..36].copy_from_slice(&u32::from(fluid.is_some()).to_le_bytes());
        optical_bytes[36..40].copy_from_slice(&u32::from(gas.is_some()).to_le_bytes());
        queue.write_buffer(&self.optical_buffer, 0, &optical_bytes);
        let empty_buffer: &wgpu::Buffer = &self.empty_buffer;
        self.optical_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`SceneRendererLightingPass.optical_bind_group`"),
            layout: &self.optical_bind_group_layout,
            entries: &[
                buffer_binding(0, &self.optical_buffer),
                buffer_binding(1, cellular.material_identifier_buffer),
                buffer_binding(2, cellular.color_buffer),
                buffer_binding(3, temperature.matter_buffer),
                buffer_binding(4, fluid.map_or(empty_buffer, |fluid| fluid.raster_buffer)),
                buffer_binding(5, fluid.map_or(empty_buffer, |fluid| fluid.color_buffer)),
                buffer_binding(6, fluid.map_or(empty_buffer, |fluid| fluid.heat_buffer)),
                buffer_binding(7, fluid.map_or(empty_buffer, |fluid| fluid.material_properties_buffer)),
                buffer_binding(8, gas.map_or(empty_buffer, |gas| gas.gas_buffer)),
                buffer_binding(9, gas.map_or(empty_buffer, |gas| gas.material_properties_buffer)),
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&domain.actor_optics_view),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::TextureView(&domain.optical_view),
                },
            ],
        }));
        // cascades, whose probes stay at fixed world cells as the domain moves
        let world_origin: [i64; 2] = [
            cellular.region_tile_x * cellular.cellular_tile_size as i64,
            cellular.region_tile_y * cellular.cellular_tile_size as i64,
        ];
        self.levels = Self::domain_levels(width, height, world_origin);
        let mut cascade_bytes: Vec<u8> =
            vec![0; LIGHTING_UNIFORM_STRIDE as usize * LIGHTING_LEVEL_MAXIMUM];
        for (index, level) in self.levels.iter().enumerate() {
            let upper: Option<&LightingLevel> = self.levels.get(index + 1);
            let record: &mut [u8] = &mut cascade_bytes[index * LIGHTING_UNIFORM_STRIDE as usize..];
            let words: [[u8; 4]; 16] = [
                width.to_le_bytes(),
                height.to_le_bytes(),
                level.probe_count[0].to_le_bytes(),
                level.probe_count[1].to_le_bytes(),
                level.probe_origin[0].to_le_bytes(),
                level.probe_origin[1].to_le_bytes(),
                level.spacing.to_le_bytes(),
                level.direction_count.to_le_bytes(),
                level.interval_start.to_le_bytes(),
                level.interval_end.to_le_bytes(),
                upper.map_or(0, |upper| upper.probe_count[0]).to_le_bytes(),
                upper.map_or(0, |upper| upper.probe_count[1]).to_le_bytes(),
                upper.map_or(0.0, |upper| upper.probe_origin[0]).to_le_bytes(),
                upper.map_or(0.0, |upper| upper.probe_origin[1]).to_le_bytes(),
                upper.map_or(1.0, |upper| upper.spacing).to_le_bytes(),
                u32::from(upper.is_some()).to_le_bytes(),
            ];
            for (word_index, word) in words.iter().enumerate() {
                record[word_index * 4..word_index * 4 + 4].copy_from_slice(word);
            }
        }
        queue.write_buffer(&self.cascade_buffer, 0, &cascade_bytes);
        // spread passes along rows, then columns, then rows into the light texture
        let mut spread_bytes: Vec<u8> = vec![0; LIGHTING_UNIFORM_STRIDE as usize * 3];
        for (pass, (axis, adds_backlight, writes_light)) in
            [(0u32, 1u32, 0u32), (1, 0, 0), (0, 0, 1)].into_iter().enumerate()
        {
            let record: &mut [u8] = &mut spread_bytes[pass * LIGHTING_UNIFORM_STRIDE as usize..];
            let words: [[u8; 4]; 8] = [
                width.to_le_bytes(),
                height.to_le_bytes(),
                axis.to_le_bytes(),
                lighting.penetration.max(1.0).to_le_bytes(),
                cellular_tile_size.to_le_bytes(),
                adds_backlight.to_le_bytes(),
                writes_light.to_le_bytes(),
                0u32.to_le_bytes(),
            ];
            for (word_index, word) in words.iter().enumerate() {
                record[word_index * 4..word_index * 4 + 4].copy_from_slice(word);
            }
        }
        queue.write_buffer(&self.spread_buffer, 0, &spread_bytes);
    }

    /// Records the prepared lighting computation
    pub fn compute(&self, command_encoder: &mut wgpu::CommandEncoder) {
        let (Some(domain), Some(optical_bind_group), Some(lighting_bind_group), true) = (
            self.domain.as_ref(),
            self.optical_bind_group.as_ref(),
            self.lighting_bind_group.as_ref(),
            self.is_lit,
        ) else {
            return;
        };
        {
            let mut render_pass: wgpu::RenderPass<'_> =
                command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("`SceneRendererLightingPass.actor_optics`"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &domain.actor_optics_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            render_pass.set_bind_group(0, &self.sprite_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            render_pass.set_pipeline(&self.actor_pipeline);
            for (view, instances) in &self.actor_batches {
                if let Some(bind_group) = self.texture_bind_groups.get(view) {
                    render_pass.set_bind_group(1, bind_group, &[]);
                    render_pass.draw(0..4, instances.clone());
                }
            }
            if !self.light_instances.is_empty() {
                if let Some(bind_group) = self.texture_bind_groups.get(&self.empty_light_view) {
                    render_pass.set_pipeline(&self.light_pipeline);
                    render_pass.set_bind_group(1, bind_group, &[]);
                    render_pass.draw(0..4, self.light_instances.clone());
                }
            }
        }
        {
            let mut render_pass: wgpu::RenderPass<'_> =
                command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("`SceneRendererLightingPass.backlight`"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &domain.backlight_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            render_pass.set_bind_group(0, &self.sprite_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            render_pass.set_pipeline(&self.background_pipeline);
            for (view, instances) in &self.background_batches {
                if let Some(bind_group) = self.texture_bind_groups.get(view) {
                    render_pass.set_bind_group(1, bind_group, &[]);
                    render_pass.draw(0..4, instances.clone());
                }
            }
        }
        // the optical field samples the lighting group, whose light texture the later passes write
        {
            let mut compute_pass: wgpu::ComputePass<'_> =
                command_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("`SceneRendererLightingPass.optical`"),
                    timestamp_writes: None,
                });
            compute_pass.set_pipeline(&self.optical_pipeline);
            compute_pass.set_bind_group(0, optical_bind_group, &[]);
            compute_pass.set_bind_group(1, lighting_bind_group, &[]);
            compute_pass.dispatch_workgroups(domain.width.div_ceil(8), domain.height.div_ceil(8), 1);
        }
        let mut compute_pass: wgpu::ComputePass<'_> =
            command_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("`SceneRendererLightingPass.compute`"),
                timestamp_writes: None,
            });
        // coarsest level first, each merging the coarser level written before it
        // uniform blocks let the rays cross them at once
        compute_pass.set_pipeline(&self.block_pipeline);
        compute_pass.set_bind_group(0, &domain.block_bind_group, &[]);
        compute_pass.dispatch_workgroups(domain.width.div_ceil(8), domain.height.div_ceil(8), 1);
        compute_pass.set_pipeline(&self.trace_pipeline);
        for index in (1..self.levels.len()).rev() {
            let level: &LightingLevel = &self.levels[index];
            let offset: u32 = (index as u64 * LIGHTING_UNIFORM_STRIDE) as u32;
            compute_pass.set_bind_group(0, &domain.cascade_bind_groups[index % 2], &[offset]);
            let count: u32 = level.probe_count[0] * level.probe_count[1] * level.direction_count;
            let (x, y): (u32, u32) = workgroup_counts(count, 64);
            compute_pass.dispatch_workgroups(x, y, 1);
        }
        compute_pass.set_pipeline(&self.integrate_pipeline);
        compute_pass.set_bind_group(0, &domain.cascade_bind_groups[0], &[0]);
        compute_pass.dispatch_workgroups(domain.width.div_ceil(8), domain.height.div_ceil(8), 1);
        compute_pass.set_pipeline(&self.spread_pipeline);
        for (pass, bind_group) in domain.spread_bind_groups.iter().enumerate() {
            let offset: u32 = (pass as u64 * LIGHTING_UNIFORM_STRIDE) as u32;
            compute_pass.set_bind_group(0, bind_group, &[offset]);
            let lines: u32 = if pass == 1 { domain.width } else { domain.height };
            compute_pass.dispatch_workgroups(lines, 1, 1);
        }
        compute_pass.set_pipeline(&self.mip_pipeline);
        for (bind_group, size) in domain.mip_bind_groups.iter().zip(&domain.mip_sizes) {
            compute_pass.set_bind_group(0, bind_group, &[]);
            compute_pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
        }
    }

}
