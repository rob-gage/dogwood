// Copyright Rob Gage 2026

use super::scene_renderer_lighting_pass::lighting_shader_source;
use crate::world::scenes::{SceneGraphicsCellular, SceneGraphicsMaterialSurfaces};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Renderer for lit cellular tiles
pub struct SceneRendererCellularPass {
    /// Layout for cellular uniforms and storage buffers
    bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline layout for cellular rendering
    pipeline_layout: wgpu::PipelineLayout,
    /// Shader module for cellular rendering
    shader_module: wgpu::ShaderModule,
    /// Render pipelines indexed by target format
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Per-view cellular uniform buffer
    uniform_buffer: wgpu::Buffer,
    /// Cached cellular bind group
    bind_group: Option<wgpu::BindGroup>,
    /// Material identifier buffer used by the cached cellular bind group
    material_identifier_buffer: Option<wgpu::Buffer>,
    /// Color buffer used by the cached cellular bind group
    color_buffer: Option<wgpu::Buffer>,
    /// Matter buffer used by the cached cellular bind group
    matter_buffer: Option<wgpu::Buffer>,
    /// Surface field used by the cached cellular bind group
    surface_field_view: Option<wgpu::TextureView>,
    /// Surface art used by the cached cellular bind group
    surface_art_view: Option<wgpu::TextureView>,
    /// Empty art bound when no material shows a surface
    default_surface_art_view: wgpu::TextureView,
    /// Layers bound when no material shows a surface, leaving every cell unchanged
    default_surface_layers: wgpu::Buffer,
}

impl SceneRendererCellularPass {

    /// Creates cellular rendering resources sampling the lighting group of a layout
    pub fn new(accelerator: &Accelerator, lighting_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererCellularPass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
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
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererCellularPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(lighting_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererCellularPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    lighting_shader_source(1, include_str!("scene_renderer_cellular.wgsl")).into(),
                ),
            },
        );
        let pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline> = HashMap::new();
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererCellularPass.uniform_buffer`"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let default_surface_art_view: wgpu::TextureView = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("`SceneRendererCellularPass.default_surface_art_view`"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
        // zero-initialized, so no material has a surface
        let default_surface_layers: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererCellularPass.default_surface_layers`"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Self {
            bind_group_layout,
            pipeline_layout,
            shader_module,
            pipelines,
            uniform_buffer,
            bind_group: None,
            material_identifier_buffer: None,
            color_buffer: None,
            matter_buffer: None,
            surface_field_view: None,
            surface_art_view: None,
            default_surface_art_view,
            default_surface_layers,
        }
    }

    /// Uploads cellular uniforms and prepares the target-format pipeline
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        viewport_x: u32,
        viewport_y: u32,
        viewport_width: u32,
        viewport_height: u32,
        pixels_per_tile: f32,
        cellular_graphics: &SceneGraphicsCellular<'_>,
        matter_buffer: &wgpu::Buffer,
        surface_field_view: &wgpu::TextureView,
        material_surfaces: Option<&SceneGraphicsMaterialSurfaces<'_>>,
    ) {
        let device: &wgpu::Device = accelerator.device();
        if !self.pipelines.contains_key(&format) {
            let render_pipeline: wgpu::RenderPipeline = device.create_render_pipeline(
                &wgpu::RenderPipelineDescriptor {
                    label: Some("`SceneRendererCellularPass.pipelines`"),
                    layout: Some(&self.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.shader_module,
                        entry_point: Some("vertex"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[],
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
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
        let viewport_origin_x: f32 = viewport_x as f32;
        let viewport_origin_y: f32 = viewport_y as f32;
        let viewport_width_as_float: f32 = viewport_width as f32;
        let viewport_height_as_float: f32 = viewport_height as f32;
        let camera_relative_region_x: f32 = -cellular_graphics.region_x;
        let camera_relative_region_y: f32 = -cellular_graphics.region_y;
        let region_width: u32 = cellular_graphics.region_width;
        let region_height: u32 = cellular_graphics.region_height;
        let ring_offset_x: u32 = cellular_graphics.ring_offset_x;
        let ring_offset_y: u32 = cellular_graphics.ring_offset_y;
        let cellular_tile_size: u32 = cellular_graphics.cellular_tile_size;
        let mut uniform_bytes: [u8; 80] = [0; 80];
        uniform_bytes[0..4].copy_from_slice(&viewport_origin_x.to_le_bytes());
        uniform_bytes[4..8].copy_from_slice(&viewport_origin_y.to_le_bytes());
        uniform_bytes[8..12].copy_from_slice(&viewport_width_as_float.to_le_bytes());
        uniform_bytes[12..16].copy_from_slice(&viewport_height_as_float.to_le_bytes());
        uniform_bytes[16..20].copy_from_slice(&pixels_per_tile.to_le_bytes());
        uniform_bytes[24..28].copy_from_slice(&camera_relative_region_x.to_le_bytes());
        uniform_bytes[28..32].copy_from_slice(&camera_relative_region_y.to_le_bytes());
        uniform_bytes[32..36].copy_from_slice(&region_width.to_le_bytes());
        uniform_bytes[36..40].copy_from_slice(&region_height.to_le_bytes());
        uniform_bytes[40..44].copy_from_slice(&ring_offset_x.to_le_bytes());
        uniform_bytes[44..48].copy_from_slice(&ring_offset_y.to_le_bytes());
        uniform_bytes[48..52].copy_from_slice(&cellular_tile_size.to_le_bytes());
        // integers keep world alignment exact however far the region lies from the origin
        let art_size: i64 = 256;
        let surface_art_offset_x: u32 = (cellular_graphics.region_tile_x
            * i64::from(cellular_tile_size)).rem_euclid(art_size) as u32;
        let surface_art_offset_y: u32 = (cellular_graphics.region_tile_y
            * i64::from(cellular_tile_size)).rem_euclid(art_size) as u32;
        uniform_bytes[52..56].copy_from_slice(&surface_art_offset_x.to_le_bytes());
        uniform_bytes[56..60].copy_from_slice(&surface_art_offset_y.to_le_bytes());
        let tile_cells: i64 = i64::from(cellular_tile_size);
        let surface_field_offset_x: u32 = (cellular_graphics.region_tile_x * tile_cells)
            .rem_euclid(i64::from(region_width) * tile_cells) as u32;
        let surface_field_offset_y: u32 = (cellular_graphics.region_tile_y * tile_cells)
            .rem_euclid(i64::from(region_height) * tile_cells) as u32;
        uniform_bytes[64..68].copy_from_slice(&surface_field_offset_x.to_le_bytes());
        uniform_bytes[68..72].copy_from_slice(&surface_field_offset_y.to_le_bytes());
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let surface_art_view: &wgpu::TextureView = material_surfaces
            .map_or(&self.default_surface_art_view, |surfaces| surfaces.art);
        let surface_layers: &wgpu::Buffer = material_surfaces
            .map_or(&self.default_surface_layers, |surfaces| surfaces.layers);
        let surfaces_differ: bool = self.surface_field_view.as_ref() != Some(surface_field_view)
            || self.surface_art_view.as_ref() != Some(surface_art_view);
        let material_buffers_differ: bool = self.material_identifier_buffer.as_ref()
            != Some(cellular_graphics.material_identifier_buffer);
        let color_buffers_differ: bool = self.color_buffer.as_ref()
            != Some(cellular_graphics.color_buffer);
        let matter_buffers_differ: bool = self.matter_buffer.as_ref() != Some(matter_buffer);
        if self.bind_group.is_none()
            || material_buffers_differ
            || color_buffers_differ
            || matter_buffers_differ
            || surfaces_differ
        {
            let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`SceneRendererCellularPass.bind_group`"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: cellular_graphics.material_identifier_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: cellular_graphics.color_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: matter_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(surface_field_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: surface_layers.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(surface_art_view),
                    },
                ],
            });
            self.bind_group = Some(bind_group);
            self.material_identifier_buffer =
                Some(cellular_graphics.material_identifier_buffer.clone());
            self.color_buffer = Some(cellular_graphics.color_buffer.clone());
            self.matter_buffer = Some(matter_buffer.clone());
            self.surface_field_view = Some(surface_field_view.clone());
            self.surface_art_view = Some(surface_art_view.clone());
        }
    }

    /// Draws lit cellular appearance for one prepared view
    pub fn draw(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        lighting_bind_group: &wgpu::BindGroup,
    ) {
        let render_pipeline: &wgpu::RenderPipeline = match self.pipelines.get(&format) {
            Some(render_pipeline) => render_pipeline,
            None => return,
        };
        let bind_group: &wgpu::BindGroup = match self.bind_group.as_ref() {
            Some(bind_group) => bind_group,
            None => return,
        };
        render_pass.set_pipeline(render_pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_bind_group(1, lighting_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }

}
