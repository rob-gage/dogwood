// Copyright Rob Gage 2026

use crate::world::scenes::SceneGraphicsCellular;
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Renderer for cellular albedo tiles
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
    /// Immutable material properties used by the cached bind group
    material_properties_buffer: Option<wgpu::Buffer>,
    /// Material atlas used by the cached bind group
    material_atlas_texture_view: Option<wgpu::TextureView>,
    /// Per-view repetition phases as WGSL vec2 records with an 8-byte stride and alignment
    material_phase_buffer: wgpu::Buffer,
    /// Capacity of the repetition phase buffer in bytes
    material_phase_buffer_capacity: u64,
    /// Material table used to prepare the cached repetition phases
    phase_material_properties_buffer: Option<wgpu::Buffer>,
    /// Region tile origin used to prepare the cached repetition phases
    phase_region_tile_origin: Option<(i64, i64)>,
}

impl SceneRendererCellularPass {

    /// Creates cellular rendering resources
    pub fn new(accelerator: &Accelerator) -> Self {
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
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererCellularPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererCellularPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("scene_renderer_cellular.wgsl").into(),
                ),
            },
        );
        let pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline> = HashMap::new();
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererCellularPass.uniform_buffer`"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let material_phase_buffer_capacity: u64 = 8;
        let material_phase_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererCellularPass.material_phase_buffer`"),
            size: material_phase_buffer_capacity,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
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
            material_properties_buffer: None,
            material_atlas_texture_view: None,
            material_phase_buffer,
            material_phase_buffer_capacity,
            phase_material_properties_buffer: None,
            phase_region_tile_origin: None,
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
        let material_properties_buffer: &wgpu::Buffer =
            cellular_graphics.cellular_static_properties_buffer;
        let material_atlas_texture_view: &wgpu::TextureView =
            cellular_graphics.cellular_static_albedo_atlas_texture_view;
        let material_repetition_sizes: &[[f32; 2]] =
            cellular_graphics.cellular_static_repetition_sizes;
        let material_count: u32 = material_repetition_sizes.len() as u32;
        let mut uniform_bytes: [u8; 64] = [0; 64];
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
        uniform_bytes[52..56].copy_from_slice(&material_count.to_le_bytes());
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let material_properties_differ: bool = self.material_properties_buffer.as_ref()
            != Some(material_properties_buffer);
        if self.material_atlas_texture_view.is_none() || material_properties_differ {
            self.material_properties_buffer = Some(material_properties_buffer.clone());
            self.material_atlas_texture_view = Some(material_atlas_texture_view.clone());
            self.bind_group = None;
        }
        let required_phase_buffer_capacity: u64 = u64::from(material_count).max(1) * 8;
        if required_phase_buffer_capacity > self.material_phase_buffer_capacity {
            self.material_phase_buffer_capacity = required_phase_buffer_capacity;
            self.material_phase_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`SceneRendererCellularPass.material_phase_buffer`"),
                size: self.material_phase_buffer_capacity,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.phase_region_tile_origin = None;
            self.bind_group = None;
        }
        let region_tile_origin: (i64, i64) = (
            cellular_graphics.region_tile_x,
            cellular_graphics.region_tile_y,
        );
        let phases_need_upload: bool = self.phase_region_tile_origin != Some(region_tile_origin)
            || self.phase_material_properties_buffer.as_ref()
                != Some(material_properties_buffer);
        if phases_need_upload {
            let mut phase_bytes: Vec<u8> = Vec::with_capacity(material_count as usize * 8);
            for [repetition_width, repetition_height] in material_repetition_sizes {
                let phase_x: f32 = ((region_tile_origin.0 as f64)
                    .rem_euclid(f64::from(*repetition_width))
                    / f64::from(*repetition_width)) as f32;
                let phase_y: f32 = ((region_tile_origin.1 as f64)
                    .rem_euclid(f64::from(*repetition_height))
                    / f64::from(*repetition_height)) as f32;
                phase_bytes.extend_from_slice(&phase_x.to_le_bytes());
                phase_bytes.extend_from_slice(&phase_y.to_le_bytes());
            }
            if phase_bytes.is_empty() { phase_bytes.resize(8, 0); }
            accelerator.queue().write_buffer(&self.material_phase_buffer, 0, &phase_bytes);
            self.phase_material_properties_buffer =
                Some(material_properties_buffer.clone());
            self.phase_region_tile_origin = Some(region_tile_origin);
        }
        let material_buffers_differ: bool = self.material_identifier_buffer.as_ref()
            != Some(cellular_graphics.material_identifier_buffer);
        let color_buffers_differ: bool = self.color_buffer.as_ref()
            != Some(cellular_graphics.color_buffer);
        if self.bind_group.is_none() || material_buffers_differ || color_buffers_differ
            || material_properties_differ
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
                        resource: material_properties_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(material_atlas_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: self.material_phase_buffer.as_entire_binding(),
                    },
                ],
            });
            self.bind_group = Some(bind_group);
            self.material_identifier_buffer =
                Some(cellular_graphics.material_identifier_buffer.clone());
            self.color_buffer = Some(cellular_graphics.color_buffer.clone());
        }
    }

    /// Draws cellular albedo for one prepared view
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
