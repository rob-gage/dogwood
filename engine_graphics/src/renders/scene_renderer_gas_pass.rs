// Copyright Rob Gage 2026

use super::scene_renderer_lighting_pass::lighting_shader_source;
use crate::world::scenes::SceneGraphicsGas;
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Renderer for gas cells
pub struct SceneRendererGasPass {
    /// Layout for gas uniforms and storage buffers
    bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline layout for gas rendering
    pipeline_layout: wgpu::PipelineLayout,
    /// Shader module for gas rendering
    shader_module: wgpu::ShaderModule,
    /// Render pipelines indexed by target format
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Compute pipeline shading each cell of the shaded rectangle once
    shade_pipeline: wgpu::ComputePipeline,
    /// Compute pipeline cancelling the pressure checker in the shaded colors
    cancel_checker_pipeline: wgpu::ComputePipeline,
    /// Compute pipeline finding the blocks of the shaded rectangle that hold gas
    classify_pipeline: wgpu::ComputePipeline,
    /// Compute pipeline finding the blocks within one block of gas
    dilate_pipeline: wgpu::ComputePipeline,
    /// Whether blocks away from gas are skipped
    is_sparse: bool,
    /// Per-block flags that sparse shading keeps between frames
    block_flag_buffer: wgpu::Buffer,
    /// Width of the shaded rectangle when the block flags were last brought up to date
    processed_width: AtomicU32,
    /// Height of the shaded rectangle when the block flags were last brought up to date
    processed_height: AtomicU32,
    /// Whether the next shading must process every block because the buffers were replaced
    is_force_pending: AtomicBool,
    /// Per-view gas uniform buffer
    uniform_buffer: wgpu::Buffer,
    /// Packed premultiplied color of each cell of the shaded rectangle
    shaded_cell_buffer: wgpu::Buffer,
    /// Packed premultiplied color of each cell with the pressure checker cancelled
    filtered_cell_buffer: wgpu::Buffer,
    /// Width and height of the shaded cell rectangle of the prepared view
    shaded_size: (u32, u32),
    /// Cached gas bind group
    bind_group: Option<wgpu::BindGroup>,
    /// Material identifier buffer used by the cached gas bind group
    material_identifier_buffer: Option<wgpu::Buffer>,
    /// Gas buffer used by the cached gas bind group
    gas_buffer: Option<wgpu::Buffer>,
    /// Material properties buffer used by the cached gas bind group
    material_properties_buffer: Option<wgpu::Buffer>,
    /// Material graphics buffer used by the cached gas bind group
    material_graphics_buffer: Option<wgpu::Buffer>,
    /// Matter buffer used by the cached gas bind group
    matter_buffer: Option<wgpu::Buffer>,
}

impl SceneRendererGasPass {

    /// Creates gas rendering resources
    pub fn new(accelerator: &Accelerator, lighting_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        Self::new_config(accelerator,lighting_bind_group_layout,std::env::var_os("DOGWOOD_GAS_FULL_SCAN").is_none(),std::env::var_os("DOGWOOD_GAS_FULL_SHADE").is_none())
    }
    pub(super) fn new_config(accelerator:&Accelerator,lighting_bind_group_layout:&wgpu::BindGroupLayout,support:bool,sparse:bool)->Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = engine_host::acceleration::create_bind_group_layout(device,
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererGasPass.bind_group_layout`"),
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
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::COMPUTE,
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
        let pipeline_layout: wgpu::PipelineLayout = engine_host::acceleration::create_pipeline_layout(device,
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererGasPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(lighting_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = engine_host::acceleration::create_shader_module(device,
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererGasPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    lighting_shader_source(1, &include_str!("scene_renderer_gas.wgsl").replace("const GAS_EMPTY_SUPPORT: bool = false;",if support {"const GAS_EMPTY_SUPPORT: bool = true;"}else{"const GAS_EMPTY_SUPPORT: bool = false;"}).replace("const GAS_SPARSE: bool = false;",if sparse {"const GAS_SPARSE: bool = true;"}else{"const GAS_SPARSE: bool = false;"})).into(),
                ),
            },
        );
        let pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline> = HashMap::new();
        let create_compute_pipeline = |entry_point: &str| -> wgpu::ComputePipeline {
            engine_host::acceleration::create_compute_pipeline(device, &wgpu::ComputePipelineDescriptor {
                label: Some("`SceneRendererGasPass` compute pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let shade_pipeline: wgpu::ComputePipeline = create_compute_pipeline("shade");
        let cancel_checker_pipeline: wgpu::ComputePipeline =
            create_compute_pipeline("cancel_checker");
        let classify_pipeline: wgpu::ComputePipeline = create_compute_pipeline("classify");
        let dilate_pipeline: wgpu::ComputePipeline = create_compute_pipeline("dilate");
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererGasPass.uniform_buffer`"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shaded_cell_buffer: wgpu::Buffer = Self::new_cell_buffer(device, 32);
        let filtered_cell_buffer: wgpu::Buffer = Self::new_cell_buffer(device, 32);
        let block_flag_buffer: wgpu::Buffer = Self::new_cell_buffer(device, 20);
        Self {
            bind_group_layout,
            pipeline_layout,
            shader_module,
            pipelines,
            shade_pipeline,
            cancel_checker_pipeline,
            classify_pipeline,
            dilate_pipeline,
            is_sparse: sparse,
            block_flag_buffer,
            processed_width: AtomicU32::new(0),
            processed_height: AtomicU32::new(0),
            is_force_pending: AtomicBool::new(true),
            uniform_buffer,
            shaded_cell_buffer,
            filtered_cell_buffer,
            shaded_size: (0, 0),
            bind_group: None,
            material_identifier_buffer: None,
            gas_buffer: None,
            material_properties_buffer: None,
            material_graphics_buffer: None,
            matter_buffer: None,
        }
    }

    /// Creates a storage buffer for per-cell shading data
    fn new_cell_buffer(device: &wgpu::Device, byte_count: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererGasPass` cell buffer"),
            size: byte_count,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
    }

    /// Uploads gas uniforms and prepares the target-format pipeline
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        viewport_x: u32,
        viewport_y: u32,
        viewport_width: u32,
        viewport_height: u32,
        pixels_per_tile: f32,
        gas_graphics: &SceneGraphicsGas<'_>,
        matter_buffer: &wgpu::Buffer,
    ) {
        let device: &wgpu::Device = accelerator.device();
        if !self.pipelines.contains_key(&format) {
            let render_pipeline: wgpu::RenderPipeline = engine_host::acceleration::create_render_pipeline(device,
                &wgpu::RenderPipelineDescriptor {
                    label: Some("`SceneRendererGasPass.pipelines`"),
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
        let camera_relative_region_x: f32 = -gas_graphics.region_x;
        let camera_relative_region_y: f32 = -gas_graphics.region_y;
        let region_width: u32 = gas_graphics.region_width;
        let region_height: u32 = gas_graphics.region_height;
        let ring_offset_x: u32 = gas_graphics.ring_offset_x;
        let ring_offset_y: u32 = gas_graphics.ring_offset_y;
        let cellular_tile_size: u32 = gas_graphics.cellular_tile_size;
        let amount_per_atmosphere: f32 = gas_graphics.amount_per_atmosphere;
        // shade the cells under the viewport, four cells past each edge and at most four cells past
        // the region: the B-spline taps reach two cells and the checker filter reads two more
        let cells_per_tile: f32 = cellular_tile_size as f32;
        let half_width: f32 = viewport_width_as_float * 0.5 / pixels_per_tile;
        let half_height: f32 = viewport_height_as_float * 0.5 / pixels_per_tile;
        let region_cell_width: i32 = (region_width * cellular_tile_size) as i32;
        let region_cell_height: i32 = (region_height * cellular_tile_size) as i32;
        let cell_range = |center: f32, half: f32, region_cells: i32| -> (i32, i32) {
            let minimum: i32 = ((center - half) * cells_per_tile).floor() as i32 - 4;
            let maximum: i32 = ((center + half) * cells_per_tile).floor() as i32 + 4;
            (minimum.max(-4), maximum.min(region_cells + 3))
        };
        let (minimum_x, maximum_x): (i32, i32) =
            cell_range(camera_relative_region_x, half_width, region_cell_width);
        let (minimum_y, maximum_y): (i32, i32) =
            cell_range(camera_relative_region_y, half_height, region_cell_height);
        let mut shaded_width: u32 = (maximum_x - minimum_x + 1).max(0) as u32;
        let mut shaded_height: u32 = (maximum_y - minimum_y + 1).max(0) as u32;
        // sparse shading keeps flags per 8 by 8 block, so it shades whole blocks: the rectangle then
        // only changes size, and restarts the flags, when it crosses a block boundary instead of
        // whenever the camera moves by a cell, and no block holds rows that fell outside it
        if self.is_sparse {
            shaded_width = shaded_width.next_multiple_of(8);
            shaded_height = shaded_height.next_multiple_of(8);
        }
        self.shaded_size = (shaded_width, shaded_height);
        let shaded_cell_count: u64 = u64::from(shaded_width) * u64::from(shaded_height);
        let mut cell_buffers_differ: bool = false;
        if shaded_cell_count * 32 > self.shaded_cell_buffer.size() {
            self.shaded_cell_buffer = Self::new_cell_buffer(device, shaded_cell_count * 32);
            cell_buffers_differ = true;
        }
        if shaded_cell_count * 32 > self.filtered_cell_buffer.size() {
            self.filtered_cell_buffer = Self::new_cell_buffer(device, shaded_cell_count * 32);
            cell_buffers_differ = true;
        }
        // block flags are kept per block of the shaded rectangle, so they restart whenever its size
        // or any of the buffers change, which processes every block once
        let block_flag_bytes: u64 = u64::from(shaded_width.div_ceil(8))
            * u64::from(shaded_height.div_ceil(8)) * 20;
        if block_flag_bytes > self.block_flag_buffer.size() {
            self.block_flag_buffer = Self::new_cell_buffer(device, block_flag_bytes);
            cell_buffers_differ = true;
        }
        if cell_buffers_differ { self.is_force_pending.store(true, Ordering::Relaxed); }
        let is_forced: bool = self.is_force_pending.load(Ordering::Relaxed)
            || self.processed_width.load(Ordering::Relaxed) != shaded_width
            || self.processed_height.load(Ordering::Relaxed) != shaded_height;
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
        uniform_bytes[52..56].copy_from_slice(&amount_per_atmosphere.to_le_bytes());
        uniform_bytes[56..60].copy_from_slice(&minimum_x.to_le_bytes());
        uniform_bytes[60..64].copy_from_slice(&minimum_y.to_le_bytes());
        uniform_bytes[64..68].copy_from_slice(&shaded_width.to_le_bytes());
        uniform_bytes[68..72].copy_from_slice(&shaded_height.to_le_bytes());
        uniform_bytes[72..76].copy_from_slice(&gas_graphics.time.to_le_bytes());
        uniform_bytes[76..80].copy_from_slice(&u32::from(is_forced).to_le_bytes());
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let material_buffers_differ: bool = self.material_identifier_buffer.as_ref()
            != Some(gas_graphics.material_identifier_buffer);
        let gas_buffers_differ: bool = self.gas_buffer.as_ref() != Some(gas_graphics.gas_buffer);
        let properties_buffers_differ: bool = self.material_properties_buffer.as_ref()
            != Some(gas_graphics.material_properties_buffer);
        let graphics_buffers_differ: bool = self.material_graphics_buffer.as_ref()
            != Some(gas_graphics.material_graphics_buffer);
        let matter_buffers_differ: bool = self.matter_buffer.as_ref() != Some(matter_buffer);
        if self.bind_group.is_none() || material_buffers_differ || gas_buffers_differ
            || properties_buffers_differ || graphics_buffers_differ || matter_buffers_differ || cell_buffers_differ
        {
            let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`SceneRendererGasPass.bind_group`"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: gas_graphics.material_identifier_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: gas_graphics.gas_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: gas_graphics.material_properties_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: matter_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: self.shaded_cell_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: self.filtered_cell_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: gas_graphics.material_graphics_buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: self.block_flag_buffer.as_entire_binding(),
                    },
                ],
            });
            self.bind_group = Some(bind_group);
            self.material_identifier_buffer = Some(gas_graphics.material_identifier_buffer.clone());
            self.gas_buffer = Some(gas_graphics.gas_buffer.clone());
            self.material_properties_buffer = Some(gas_graphics.material_properties_buffer.clone());
            self.material_graphics_buffer = Some(gas_graphics.material_graphics_buffer.clone());
            self.matter_buffer = Some(matter_buffer.clone());
        }
    }

    /// Shades the prepared view's gas cells and cancels the pressure checker before the view is
    /// drawn
    pub fn shade(
        &self,
        command_encoder: &mut wgpu::CommandEncoder,
        lighting_bind_group: &wgpu::BindGroup,
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else { return; };
        let (shaded_width, shaded_height): (u32, u32) = self.shaded_size;
        if shaded_width == 0 || shaded_height == 0 { return; }
        let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("`SceneRendererGasPass::shade`"),
                timestamp_writes: None,
            });
        compute_pass.set_bind_group(0, bind_group, &[]);
        compute_pass.set_bind_group(1, lighting_bind_group, &[]);
        let (block_columns, block_rows): (u32, u32) =
            (shaded_width.div_ceil(8), shaded_height.div_ceil(8));
        if self.is_sparse {
            compute_pass.set_pipeline(&self.classify_pipeline);
            compute_pass.dispatch_workgroups(block_columns, block_rows, 1);
            compute_pass.set_pipeline(&self.dilate_pipeline);
            compute_pass.dispatch_workgroups(block_columns.div_ceil(8), block_rows.div_ceil(8), 1);
        }
        compute_pass.set_pipeline(&self.shade_pipeline);
        compute_pass.dispatch_workgroups(block_columns, block_rows, 1);
        compute_pass.set_pipeline(&self.cancel_checker_pipeline);
        compute_pass.dispatch_workgroups(block_columns, block_rows, 1);
        // every block is now consistent with the flags, whatever the shaded rectangle held before
        self.processed_width.store(shaded_width, Ordering::Relaxed);
        self.processed_height.store(shaded_height, Ordering::Relaxed);
        self.is_force_pending.store(false, Ordering::Relaxed);
    }

    /// Draws lit gas cells for one prepared view
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
