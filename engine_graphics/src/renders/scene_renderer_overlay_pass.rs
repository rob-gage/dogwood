// Copyright Rob Gage 2026

use crate::world::scenes::{SceneGraphicsOverlay, SceneGraphicsOverlayShape};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Renderer for cell-aligned scene overlays
pub struct SceneRendererOverlayPass {
    /// Pipeline layout for overlay rendering
    pipeline_layout: wgpu::PipelineLayout,
    /// Shader module for overlay rendering
    shader_module: wgpu::ShaderModule,
    /// Render pipelines indexed by target format
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Per-view overlay uniform buffer
    uniform_buffer: wgpu::Buffer,
    /// Overlay instance buffer
    instance_buffer: wgpu::Buffer,
    /// Number of prepared overlay instances
    instance_count: u32,
    /// Bind group for overlay uniforms
    bind_group: wgpu::BindGroup,
}

impl SceneRendererOverlayPass {

    /// Creates overlay rendering resources
    pub fn new(accelerator: &Accelerator) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererOverlayPass.bind_group_layout`"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererOverlayPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererOverlayPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("scene_renderer_overlay.wgsl").into(),
                ),
            },
        );
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererOverlayPass.uniform_buffer`"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererOverlayPass.instance_buffer`"),
            size: 24,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`SceneRendererOverlayPass.bind_group`"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        Self {
            pipeline_layout,
            shader_module,
            pipelines: HashMap::new(),
            uniform_buffer,
            instance_buffer,
            instance_count: 0,
            bind_group,
        }
    }

    /// Uploads overlay instances and prepares the target-format pipeline
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        viewport_x: u32,
        viewport_y: u32,
        viewport_width: u32,
        viewport_height: u32,
        pixels_per_tile: f32,
        visible_width: f32,
        visible_height: f32,
        cellular_tile_size: u32,
        cell_grid_origin: [f32; 2],
        overlays: &[SceneGraphicsOverlay],
    ) {
        let device: &wgpu::Device = accelerator.device();
        if !self.pipelines.contains_key(&format) {
            let attributes: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
                0 => Float32x2, 1 => Uint32, 2 => Float32x2, 3 => Uint32
            ];
            let render_pipeline: wgpu::RenderPipeline = device.create_render_pipeline(
                &wgpu::RenderPipelineDescriptor {
                    label: Some("`SceneRendererOverlayPass.pipelines`"),
                    layout: Some(&self.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.shader_module,
                        entry_point: Some("vertex"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: 24,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &attributes,
                        })],
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
                    multisample: wgpu::MultisampleState::default(),
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
        let viewport_values: [f32; 12] = [
            viewport_x as f32,
            viewport_y as f32,
            viewport_width as f32,
            viewport_height as f32,
            pixels_per_tile,
            1.0 / cellular_tile_size as f32,
            visible_width,
            visible_height,
            cell_grid_origin[0],
            cell_grid_origin[1],
            0.0,
            0.0,
        ];
        let mut uniform_bytes: [u8; 48] = [0; 48];
        for (index, value) in viewport_values.iter().enumerate() {
            uniform_bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        self.instance_count = overlays.len() as u32;
        if self.instance_count == 0 { return; }
        // the shader takes zero for a circle with its radius and one for a rectangle with its half
        // width and half height
        let instance_bytes: Vec<u8> = overlays.iter().flat_map(|overlay| {
            let (shape, size): (u32, [f32; 2]) = match overlay.shape {
                SceneGraphicsOverlayShape::Circle { radius } => (0, [radius, 0.0]),
                SceneGraphicsOverlayShape::Rectangle { width, height } =>
                    (1, [width / 2.0, height / 2.0]),
            };
            overlay.center_x.to_le_bytes()
                .into_iter()
                .chain(overlay.center_y.to_le_bytes())
                .chain(shape.to_le_bytes())
                .chain(size[0].to_le_bytes())
                .chain(size[1].to_le_bytes())
                .chain(overlay.color.to_le_bytes())
        }).collect();
        if self.instance_buffer.size() < instance_bytes.len() as u64 {
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`SceneRendererOverlayPass.instance_buffer`"),
                size: instance_bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        accelerator.queue().write_buffer(&self.instance_buffer, 0, &instance_bytes);
    }

    /// Draws prepared overlay instances
    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>, format: wgpu::TextureFormat) {
        if self.instance_count == 0 { return; }
        let Some(render_pipeline) = self.pipelines.get(&format) else { return; };
        render_pass.set_pipeline(render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..self.instance_count);
    }

}

