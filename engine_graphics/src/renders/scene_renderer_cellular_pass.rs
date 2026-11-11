// Copyright Rob Gage 2026

use super::scene_renderer_lighting_pass::lighting_shader_source;
use crate::world::scenes::{SceneGraphicsCellular, SceneGraphicsMaterialSurfaces};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Pixels each cached cell must cover, on average, for caching the cells under a view to cost
/// less than shading the cell of every pixel
const CELL_CACHE_PIXELS_PER_CELL_MINIMUM: f32 = 1.25;

/// Renderer for lit cellular tiles
pub struct SceneRendererCellularPass {
    /// Layout for cellular uniforms and storage buffers
    bind_group_layout: wgpu::BindGroupLayout,
    /// Compute pipeline shading each cell of the cached rectangle once
    cache_pipeline: wgpu::ComputePipeline,
    /// Whether pixels read the cached cell colors instead of shading their cell themselves
    is_cached: bool,
    /// Packed color and radiance of each cell of the cached rectangle
    cell_cache_buffer: wgpu::Buffer,
    /// Width and height of the cached cell rectangle of the prepared view
    cache_size: (u32, u32),
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
        Self::new_config(
            accelerator,
            lighting_bind_group_layout,
            std::env::var_os("DOGWOOD_CELLULAR_FULL_SHADE").is_none(),
        )
    }

    /// Creates cellular rendering resources, caching each cell's shading when `is_cached`
    pub(super) fn new_config(
        accelerator: &Accelerator,
        lighting_bind_group_layout: &wgpu::BindGroupLayout,
        is_cached: bool,
    ) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererCellularPass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
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
                    lighting_shader_source(1, &include_str!("scene_renderer_cellular.wgsl").replace(
                        "const CELLULAR_CACHED: bool = false;",
                        if is_cached { "const CELLULAR_CACHED: bool = true;" } else { "const CELLULAR_CACHED: bool = false;" },
                    )).into(),
                ),
            },
        );
        let cache_pipeline: wgpu::ComputePipeline = engine_host::acceleration::create_compute_pipeline(device,
            &wgpu::ComputePipelineDescriptor {
                label: Some("`SceneRendererCellularPass.cache_pipeline`"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("cache_cells"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            },
        );
        let cell_cache_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererCellularPass.cell_cache_buffer`"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline> = HashMap::new();
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererCellularPass.uniform_buffer`"),
            size: 96,
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
            cache_pipeline,
            is_cached,
            cell_cache_buffer,
            cache_size: (0, 0),
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
            let render_pipeline: wgpu::RenderPipeline = engine_host::acceleration::create_render_pipeline(device,
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
        // every pixel's cell lies within the cells under the viewport, which two more cells on each
        // side cover however the pixel centers round
        let cells_per_tile: f32 = cellular_tile_size as f32;
        let half_width: f32 = viewport_width_as_float * 0.5 / pixels_per_tile;
        let half_height: f32 = viewport_height_as_float * 0.5 / pixels_per_tile;
        let region_cell_width: i32 = (region_width * cellular_tile_size) as i32;
        let region_cell_height: i32 = (region_height * cellular_tile_size) as i32;
        let cell_range = |center: f32, half: f32, region_cells: i32| -> (i32, i32) {
            let minimum: i32 = (((center - half) * cells_per_tile).floor() as i32 - 2).max(0);
            let maximum: i32 = (((center + half) * cells_per_tile).floor() as i32 + 2)
                .min(region_cells - 1);
            (minimum, maximum)
        };
        let (cache_minimum_x, cache_maximum_x): (i32, i32) =
            cell_range(camera_relative_region_x, half_width, region_cell_width);
        let (cache_minimum_y, cache_maximum_y): (i32, i32) =
            cell_range(camera_relative_region_y, half_height, region_cell_height);
        let mut cache_width: u32 = (cache_maximum_x - cache_minimum_x + 1).max(0) as u32;
        let mut cache_height: u32 = (cache_maximum_y - cache_minimum_y + 1).max(0) as u32;
        // zoomed out far enough that a cell covers about a pixel, shading each pixel's cell is
        // cheaper than shading every cell, so no cell is cached and every pixel shades its own
        if (cache_width as f32) * (cache_height as f32) * CELL_CACHE_PIXELS_PER_CELL_MINIMUM
            > viewport_width_as_float * viewport_height_as_float
        {
            cache_width = 0;
            cache_height = 0;
        }
        self.cache_size = (cache_width, cache_height);
        let cache_bytes: u64 = u64::from(cache_width) * u64::from(cache_height) * 16;
        let mut cache_buffer_differs: bool = false;
        if cache_bytes > self.cell_cache_buffer.size() {
            self.cell_cache_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`SceneRendererCellularPass.cell_cache_buffer`"),
                size: cache_bytes,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            cache_buffer_differs = true;
        }
        let mut uniform_bytes: [u8; 96] = [0; 96];
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
        uniform_bytes[72..76].copy_from_slice(&cache_minimum_x.to_le_bytes());
        uniform_bytes[76..80].copy_from_slice(&cache_minimum_y.to_le_bytes());
        uniform_bytes[80..84].copy_from_slice(&cache_width.to_le_bytes());
        uniform_bytes[84..88].copy_from_slice(&cache_height.to_le_bytes());
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
            || cache_buffer_differs
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
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: self.cell_cache_buffer.as_entire_binding(),
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

    /// Shades each cell under the prepared view once, before the view is drawn
    pub fn shade(
        &self,
        command_encoder: &mut wgpu::CommandEncoder,
        lighting_bind_group: &wgpu::BindGroup,
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else { return; };
        let (width, height): (u32, u32) = self.cache_size;
        if !self.is_cached || width == 0 || height == 0 { return; }
        let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("`SceneRendererCellularPass::shade`"),
                timestamp_writes: None,
            });
        compute_pass.set_pipeline(&self.cache_pipeline);
        compute_pass.set_bind_group(0, bind_group, &[]);
        compute_pass.set_bind_group(1, lighting_bind_group, &[]);
        compute_pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
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
