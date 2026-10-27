// Copyright Rob Gage 2026

use super::scene_renderer_lighting_pass::lighting_shader_source;
use crate::world::scenes::SceneGraphicsGas;
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

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
    /// Per-view gas uniform buffer
    uniform_buffer: wgpu::Buffer,
    /// Cached gas bind group
    bind_group: Option<wgpu::BindGroup>,
    /// Material identifier buffer used by the cached gas bind group
    material_identifier_buffer: Option<wgpu::Buffer>,
    /// Gas buffer used by the cached gas bind group
    gas_buffer: Option<wgpu::Buffer>,
    /// Material properties buffer used by the cached gas bind group
    material_properties_buffer: Option<wgpu::Buffer>,
    /// Matter buffer used by the cached gas bind group
    matter_buffer: Option<wgpu::Buffer>,
}

impl SceneRendererGasPass {

    /// Creates gas rendering resources
    pub fn new(accelerator: &Accelerator, lighting_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererGasPass.bind_group_layout`"),
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
                label: Some("`SceneRendererGasPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(lighting_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererGasPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    lighting_shader_source(1, include_str!("scene_renderer_gas.wgsl")).into(),
                ),
            },
        );
        let pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline> = HashMap::new();
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererGasPass.uniform_buffer`"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
            gas_buffer: None,
            material_properties_buffer: None,
            matter_buffer: None,
        }
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
            let render_pipeline: wgpu::RenderPipeline = device.create_render_pipeline(
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
        uniform_bytes[52..56].copy_from_slice(&amount_per_atmosphere.to_le_bytes());
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let material_buffers_differ: bool = self.material_identifier_buffer.as_ref()
            != Some(gas_graphics.material_identifier_buffer);
        let gas_buffers_differ: bool = self.gas_buffer.as_ref() != Some(gas_graphics.gas_buffer);
        let properties_buffers_differ: bool = self.material_properties_buffer.as_ref()
            != Some(gas_graphics.material_properties_buffer);
        let matter_buffers_differ: bool = self.matter_buffer.as_ref() != Some(matter_buffer);
        if self.bind_group.is_none() || material_buffers_differ || gas_buffers_differ
            || properties_buffers_differ || matter_buffers_differ
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
                ],
            });
            self.bind_group = Some(bind_group);
            self.material_identifier_buffer = Some(gas_graphics.material_identifier_buffer.clone());
            self.gas_buffer = Some(gas_graphics.gas_buffer.clone());
            self.material_properties_buffer = Some(gas_graphics.material_properties_buffer.clone());
            self.matter_buffer = Some(matter_buffer.clone());
        }
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
