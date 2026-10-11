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
    /// Static material atlas regions and repetitions indexed by material index
    material_buffer: wgpu::Buffer,
    /// Capacity of the material buffer in bytes
    material_buffer_capacity: usize,
    /// Reused packed material bytes
    material_bytes: Vec<u8>,
    /// Atlas holding every static material frame, absent until first prepared
    material_atlas_texture_view: Option<wgpu::TextureView>,
    /// Texture and texture region of each static material copied into the atlas
    material_atlas_frames: Vec<Option<(wgpu::TextureView, [f32; 4])>>,
    /// Atlas left, top, width, and height of each static material frame
    material_atlas_regions: Vec<[u32; 4]>,
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
        let material_buffer_capacity: usize = 256 * 32;
        let material_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererCellularPass.material_buffer`"),
            size: material_buffer_capacity as u64,
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
            material_buffer,
            material_buffer_capacity,
            material_bytes: Vec::with_capacity(material_buffer_capacity),
            material_atlas_texture_view: None,
            material_atlas_frames: Vec::new(),
            material_atlas_regions: Vec::new(),
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
        let material_count: u32 = cellular_graphics.materials.len() as u32;
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
        // rebuild the atlas only when the static material frames change
        let materials: &[Option<(&wgpu::TextureView, [f32; 4], f32, f32)>] =
            &cellular_graphics.materials;
        let material_frames_differ: bool = self.material_atlas_frames.len() != materials.len()
            || self.material_atlas_frames.iter().zip(materials).any(|(atlas_frame, material)| {
                atlas_frame.as_ref().map(|(texture_view, texture_region)| {
                    (texture_view, *texture_region)
                }) != material.map(|(texture_view, texture_region, _, _)| {
                    (texture_view, texture_region)
                })
            });
        if self.material_atlas_texture_view.is_none() || material_frames_differ {
            self.prepare_material_atlas(accelerator, materials);
        }
        // pack each atlas region with its repetition size and phase at the region's origin
        self.material_bytes.clear();
        for (material, atlas_region) in materials.iter().zip(&self.material_atlas_regions) {
            let Some((_, _, width, height)) = material
                .filter(|(_, _, width, height)| *width > 0.0 && *height > 0.0)
            else {
                self.material_bytes.extend_from_slice(&[0; 32]);
                continue;
            };
            let phase_x: f32 = ((cellular_graphics.region_tile_x as f64)
                .rem_euclid(f64::from(width)) / f64::from(width)) as f32;
            let phase_y: f32 = ((cellular_graphics.region_tile_y as f64)
                .rem_euclid(f64::from(height)) / f64::from(height)) as f32;
            for word in [
                atlas_region[0],
                atlas_region[1],
                atlas_region[2],
                atlas_region[3],
                width.to_bits(),
                height.to_bits(),
                phase_x.to_bits(),
                phase_y.to_bits(),
            ] { self.material_bytes.extend_from_slice(&word.to_le_bytes()); }
        }
        if self.material_bytes.len() > self.material_buffer_capacity {
            self.material_buffer_capacity = self.material_bytes.len().next_power_of_two();
            self.material_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`SceneRendererCellularPass.material_buffer`"),
                size: self.material_buffer_capacity as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.bind_group = None;
        }
        if !self.material_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.material_buffer, 0, &self.material_bytes);
        }
        let material_buffers_differ: bool = self.material_identifier_buffer.as_ref()
            != Some(cellular_graphics.material_identifier_buffer);
        let color_buffers_differ: bool = self.color_buffer.as_ref()
            != Some(cellular_graphics.color_buffer);
        if self.bind_group.is_none() || material_buffers_differ || color_buffers_differ {
            let Some(material_atlas_texture_view) = self.material_atlas_texture_view.as_ref()
            else { return; };
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
                        resource: self.material_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(material_atlas_texture_view),
                    },
                ],
            });
            self.bind_group = Some(bind_group);
            self.material_identifier_buffer =
                Some(cellular_graphics.material_identifier_buffer.clone());
            self.color_buffer = Some(cellular_graphics.color_buffer.clone());
        }
    }

    /// Copies every static material frame into a new atlas of left-to-right rows
    fn prepare_material_atlas(
        &mut self,
        accelerator: &Accelerator,
        materials: &[Option<(&wgpu::TextureView, [f32; 4], f32, f32)>],
    ) {
        let device: &wgpu::Device = accelerator.device();
        let maximum_dimension: u32 = device.limits().max_texture_dimension_2d;
        // place frames in rows, leaving frames that do not fit with an empty region
        let mut atlas_width: u32 = 1;
        let mut atlas_height: u32 = 1;
        let mut row_x: u32 = 0;
        let mut row_y: u32 = 0;
        let mut row_height: u32 = 0;
        self.material_atlas_regions.clear();
        for material in materials {
            let Some((texture_view, texture_region, _, _)) = material else {
                self.material_atlas_regions.push([0; 4]);
                continue;
            };
            let texture: &wgpu::Texture = texture_view.texture();
            let frame_width: u32 =
                ((texture_region[2] - texture_region[0]) * texture.width() as f32).round() as u32;
            let frame_height: u32 =
                ((texture_region[3] - texture_region[1]) * texture.height() as f32).round() as u32;
            if row_x + frame_width > maximum_dimension {
                row_x = 0;
                row_y += row_height;
                row_height = 0;
            }
            if frame_width == 0 || frame_height == 0 || row_y + frame_height > maximum_dimension {
                self.material_atlas_regions.push([0; 4]);
                continue;
            }
            self.material_atlas_regions.push([row_x, row_y, frame_width, frame_height]);
            row_x += frame_width;
            row_height = row_height.max(frame_height);
            atlas_width = atlas_width.max(row_x);
            atlas_height = atlas_height.max(row_y + frame_height);
        }
        // copy every placed frame from its sprite texture into the atlas
        let atlas_texture: wgpu::Texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("`SceneRendererCellularPass.material_atlas_texture_view`"),
            size: wgpu::Extent3d {
                width: atlas_width,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut command_encoder: wgpu::CommandEncoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("`SceneRendererCellularPass` material atlas"),
            });
        for (material, atlas_region) in materials.iter().zip(&self.material_atlas_regions) {
            let Some((texture_view, texture_region, _, _)) = material else { continue; };
            if atlas_region[2] == 0 { continue; }
            let texture: &wgpu::Texture = texture_view.texture();
            command_encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: (texture_region[0] * texture.width() as f32).round() as u32,
                        y: (texture_region[1] * texture.height() as f32).round() as u32,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: atlas_region[0], y: atlas_region[1], z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: atlas_region[2],
                    height: atlas_region[3],
                    depth_or_array_layers: 1,
                },
            );
        }
        accelerator.queue().submit([command_encoder.finish()]);
        self.material_atlas_texture_view =
            Some(atlas_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.material_atlas_frames = materials
            .iter()
            .map(|material| {
                material.map(|(texture_view, texture_region, _, _)| {
                    (texture_view.clone(), texture_region)
                })
            })
            .collect();
        self.bind_group = None;
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
