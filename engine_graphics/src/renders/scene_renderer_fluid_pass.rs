// Copyright Rob Gage 2026

use crate::world::scenes::SceneGraphicsFluid;
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Rows classified above the shaded rectangle for the depth search in `scene_renderer_fluid.wgsl`
const FLUID_DEPTH_SEARCH_ROWS: u32 = 8;

/// Renderer for the cell-resolution fluid raster
pub struct SceneRendererFluidPass {
    /// Layout for fluid uniforms, raster, material properties, and particle colors
    bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline layout for fluid rendering
    pipeline_layout: wgpu::PipelineLayout,
    /// Shader module for fluid rendering
    shader_module: wgpu::ShaderModule,
    /// Render pipelines indexed by target format
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Compute pipeline classifying smoothed water cells around the view
    classify_pipeline: wgpu::ComputePipeline,
    /// Compute pipeline shading each visible water cell once
    shade_pipeline: wgpu::ComputePipeline,
    /// Per-view fluid uniform buffer
    uniform_buffer: wgpu::Buffer,
    /// Dominant material of each classified water cell, or zero
    water_cell_buffer: wgpu::Buffer,
    /// Premultiplied half-precision color of each shaded cell
    shaded_cell_buffer: wgpu::Buffer,
    /// Width and height of the shaded cell rectangle of the prepared view
    shaded_size: (u32, u32),
    /// Cached fluid bind group
    bind_group: Option<wgpu::BindGroup>,
    /// Raster buffer used by the cached fluid bind group
    raster_buffer: Option<wgpu::Buffer>,
    /// Material properties buffer used by the cached fluid bind group
    material_properties_buffer: Option<wgpu::Buffer>,
    /// Particle color buffer used by the cached fluid bind group
    color_buffer: Option<wgpu::Buffer>,
}

impl SceneRendererFluidPass {

    /// Creates fluid rendering resources
    pub fn new(accelerator: &Accelerator) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererFluidPass.bind_group_layout`"),
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
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
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
                label: Some("`SceneRendererFluidPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererFluidPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("scene_renderer_fluid.wgsl").into(),
                ),
            },
        );
        let pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline> = HashMap::new();
        let create_compute_pipeline = |entry_point: &str| -> wgpu::ComputePipeline {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("`SceneRendererFluidPass` compute pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let classify_pipeline: wgpu::ComputePipeline = create_compute_pipeline("classify");
        let shade_pipeline: wgpu::ComputePipeline = create_compute_pipeline("shade");
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererFluidPass.uniform_buffer`"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let water_cell_buffer: wgpu::Buffer = Self::new_cell_buffer(device, 4);
        let shaded_cell_buffer: wgpu::Buffer = Self::new_cell_buffer(device, 8);
        Self {
            bind_group_layout,
            pipeline_layout,
            shader_module,
            pipelines,
            classify_pipeline,
            shade_pipeline,
            uniform_buffer,
            water_cell_buffer,
            shaded_cell_buffer,
            shaded_size: (0, 0),
            bind_group: None,
            raster_buffer: None,
            material_properties_buffer: None,
            color_buffer: None,
        }
    }

    /// Creates a storage buffer for per-cell shading data
    fn new_cell_buffer(device: &wgpu::Device, byte_count: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererFluidPass` cell buffer"),
            size: byte_count,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
    }

    /// Uploads fluid uniforms and prepares the target-format pipeline
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        viewport_x: u32,
        viewport_y: u32,
        viewport_width: u32,
        viewport_height: u32,
        pixels_per_tile: f32,
        fluid_graphics: &SceneGraphicsFluid<'_>,
    ) {
        let device: &wgpu::Device = accelerator.device();
        if !self.pipelines.contains_key(&format) {
            let render_pipeline: wgpu::RenderPipeline = device.create_render_pipeline(
                &wgpu::RenderPipelineDescriptor {
                    label: Some("`SceneRendererFluidPass.pipelines`"),
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
        let camera_relative_region_x: f32 = -fluid_graphics.region_x;
        let camera_relative_region_y: f32 = -fluid_graphics.region_y;
        let region_width: u32 = fluid_graphics.region_width;
        let region_height: u32 = fluid_graphics.region_height;
        let ring_offset_x: u32 = fluid_graphics.ring_offset_x;
        let ring_offset_y: u32 = fluid_graphics.ring_offset_y;
        let cellular_tile_size: u32 = fluid_graphics.cellular_tile_size;
        // shade the cells under the viewport, one cell past each edge and at most one cell past
        // the region, whose outer cells still smooth over the region's edge cells
        let cells_per_tile: f32 = cellular_tile_size as f32;
        let half_width: f32 = viewport_width_as_float * 0.5 / pixels_per_tile;
        let half_height: f32 = viewport_height_as_float * 0.5 / pixels_per_tile;
        let region_cell_width: i32 = (region_width * cellular_tile_size) as i32;
        let region_cell_height: i32 = (region_height * cellular_tile_size) as i32;
        let cell_range = |center: f32, half: f32, region_cells: i32| -> (i32, i32) {
            let minimum: i32 = ((center - half) * cells_per_tile).floor() as i32 - 1;
            let maximum: i32 = ((center + half) * cells_per_tile).floor() as i32 + 1;
            (minimum.max(-1), maximum.min(region_cells))
        };
        let (minimum_x, maximum_x): (i32, i32) =
            cell_range(camera_relative_region_x, half_width, region_cell_width);
        let (minimum_y, maximum_y): (i32, i32) =
            cell_range(camera_relative_region_y, half_height, region_cell_height);
        let shaded_width: u32 = (maximum_x - minimum_x + 1).max(0) as u32;
        let shaded_height: u32 = (maximum_y - minimum_y + 1).max(0) as u32;
        self.shaded_size = (shaded_width, shaded_height);
        let shaded_cell_count: u64 = u64::from(shaded_width) * u64::from(shaded_height);
        let water_cell_count: u64 = u64::from(shaded_width)
            * u64::from(shaded_height + FLUID_DEPTH_SEARCH_ROWS);
        let mut cell_buffers_differ: bool = false;
        if water_cell_count * 4 > self.water_cell_buffer.size() {
            self.water_cell_buffer = Self::new_cell_buffer(device, water_cell_count * 4);
            cell_buffers_differ = true;
        }
        if shaded_cell_count * 8 > self.shaded_cell_buffer.size() {
            self.shaded_cell_buffer = Self::new_cell_buffer(device, shaded_cell_count * 8);
            cell_buffers_differ = true;
        }
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
        uniform_bytes[56..60].copy_from_slice(&minimum_x.to_le_bytes());
        uniform_bytes[60..64].copy_from_slice(&minimum_y.to_le_bytes());
        uniform_bytes[64..68].copy_from_slice(&shaded_width.to_le_bytes());
        uniform_bytes[68..72].copy_from_slice(&shaded_height.to_le_bytes());
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let raster_buffer_differs: bool = self.raster_buffer.as_ref()
            != Some(fluid_graphics.raster_buffer);
        let materials_buffer_differs: bool = self.material_properties_buffer.as_ref()
            != Some(fluid_graphics.material_properties_buffer);
        let color_buffer_differs: bool =
            self.color_buffer.as_ref() != Some(fluid_graphics.color_buffer);
        if self.bind_group.is_none() || raster_buffer_differs || materials_buffer_differs
            || color_buffer_differs || cell_buffers_differ
        {
            let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`SceneRendererFluidPass.bind_group`"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: fluid_graphics.raster_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: fluid_graphics.material_properties_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: fluid_graphics.color_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.water_cell_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: self.shaded_cell_buffer.as_entire_binding(),
                    },
                ],
            });
            self.bind_group = Some(bind_group);
            self.raster_buffer = Some(fluid_graphics.raster_buffer.clone());
            self.material_properties_buffer =
                Some(fluid_graphics.material_properties_buffer.clone());
            self.color_buffer = Some(fluid_graphics.color_buffer.clone());
        }
    }

    /// Classifies and shades the prepared view's cells before the view is drawn
    pub fn shade(&self, command_encoder: &mut wgpu::CommandEncoder) {
        let Some(bind_group) = self.bind_group.as_ref() else { return; };
        let (shaded_width, shaded_height): (u32, u32) = self.shaded_size;
        if shaded_width == 0 || shaded_height == 0 { return; }
        let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("`SceneRendererFluidPass::shade`"),
                timestamp_writes: None,
            });
        compute_pass.set_bind_group(0, bind_group, &[]);
        compute_pass.set_pipeline(&self.classify_pipeline);
        compute_pass.dispatch_workgroups(
            shaded_width.div_ceil(8),
            (shaded_height + FLUID_DEPTH_SEARCH_ROWS).div_ceil(8),
            1,
        );
        compute_pass.set_pipeline(&self.shade_pipeline);
        compute_pass.dispatch_workgroups(shaded_width.div_ceil(8), shaded_height.div_ceil(8), 1);
    }

    /// Draws fluid color for one prepared view
    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>, format: wgpu::TextureFormat) {
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
        render_pass.draw(0..3, 0..1);
    }

}
