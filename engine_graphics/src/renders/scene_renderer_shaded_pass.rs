// Copyright Rob Gage 2026

use super::scene_renderer_lighting_pass::{
    lighting_shader_source,
    LIGHTING_OPTICS_BLEND,
    LIGHTING_OPTICS_FORMAT,
};
use crate::world::scenes::SceneGraphicsShaded;
use engine_host::acceleration::Accelerator;
use std::{collections::HashMap, ops::Range};

/// Floats of one vertex: position, first outline point, point count, whether in whole cells, deepest
/// distance, center, and four vectors of parameters
const VERTEX_FLOATS: usize = 24;
/// Bytes of one vertex
const VERTEX_BYTE_COUNT: usize = VERTEX_FLOATS * 4;

/// Renderer of bodies drawn with shaders the game supplies, one pipeline for each shader source
pub struct SceneRendererShadedPass {
    /// Layout of the uniforms and outlines
    bind_group_layout: wgpu::BindGroupLayout,
    /// Pipeline layout of every shader
    pipeline_layout: wgpu::PipelineLayout,
    /// Uniforms of the view
    uniform_buffer: wgpu::Buffer,
    /// Outlines of every body this frame
    outline_buffer: wgpu::Buffer,
    /// Capacity of the outline buffer in bytes
    outline_buffer_capacity: usize,
    /// Bind group of the uniforms and outlines
    bind_group: wgpu::BindGroup,
    /// Pipelines by shader source and target format
    pipelines: HashMap<(String, wgpu::TextureFormat), wgpu::RenderPipeline>,
    /// Pipelines drawing into the lighting domain by shader source
    optics_pipelines: HashMap<String, wgpu::RenderPipeline>,
    /// Reused vertex buffer
    vertex_buffer: wgpu::Buffer,
    /// Capacity of the vertex buffer in bytes
    vertex_buffer_capacity: usize,
    /// Reused packed vertices
    vertex_bytes: Vec<u8>,
    /// Reused packed outlines
    outline_bytes: Vec<u8>,
    /// What to draw this frame: a shader source and its range of vertices
    draws: Vec<(String, Range<u32>)>,
}

/// Creates the bind group of the uniforms and outlines
fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    outlines: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("`SceneRendererShadedPass.bind_group`"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: outlines.as_entire_binding() },
        ],
    })
}

/// Creates the buffer of the outlines
fn create_outline_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("`SceneRendererShadedPass.outline_buffer`"),
        size: capacity as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl SceneRendererShadedPass {

    /// Creates the pass's resources
    pub fn new(accelerator: &Accelerator, lighting_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`SceneRendererShadedPass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`SceneRendererShadedPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(lighting_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererShadedPass.uniform_buffer`"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let outline_buffer_capacity: usize = 4096 * 8;
        let outline_buffer: wgpu::Buffer = create_outline_buffer(device, outline_buffer_capacity);
        let bind_group: wgpu::BindGroup =
            create_bind_group(device, &bind_group_layout, &uniform_buffer, &outline_buffer);
        let vertex_buffer_capacity: usize = 256 * VERTEX_BYTE_COUNT;
        let vertex_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneRendererShadedPass.vertex_buffer`"),
            size: vertex_buffer_capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            bind_group_layout,
            pipeline_layout,
            uniform_buffer,
            outline_buffer,
            outline_buffer_capacity,
            bind_group,
            pipelines: HashMap::new(),
            optics_pipelines: HashMap::new(),
            vertex_buffer,
            vertex_buffer_capacity,
            vertex_bytes: Vec::new(),
            outline_bytes: Vec::new(),
            draws: Vec::new(),
        }
    }

    /// Uploads this frame's vertices and creates the pipelines of shaders seen for the first time
    pub fn prepare(
        &mut self,
        accelerator: &Accelerator,
        format: wgpu::TextureFormat,
        viewport: [f32; 2],
        pixels_per_tile: f32,
        cellular_tile_size: f32,
        cell_grid_origin: [f32; 2],
        shaded: &[SceneGraphicsShaded<'_>],
    ) {
        self.draws.clear();
        self.vertex_bytes.clear();
        self.outline_bytes.clear();
        let device: &wgpu::Device = accelerator.device();
        for body in shaded {
            if body.outline.len() < 3 { continue; }
            let first: f32 = (self.outline_bytes.len() / 8) as f32;
            let (mut low, mut high): ([f32; 2], [f32; 2]) = ([f32::MAX; 2], [f32::MIN; 2]);
            let (mut sum_x, mut sum_y): (f32, f32) = (0.0, 0.0);
            for point in &body.outline {
                self.outline_bytes.extend_from_slice(&point[0].to_le_bytes());
                self.outline_bytes.extend_from_slice(&point[1].to_le_bytes());
                low = [low[0].min(point[0]), low[1].min(point[1])];
                high = [high[0].max(point[0]), high[1].max(point[1])];
                sum_x += point[0];
                sum_y += point[1];
            }
            let count: f32 = body.outline.len() as f32;
            let center: [f32; 2] = [sum_x / count, sum_y / count];
            // how far in a body goes, nearly the half of its smaller extent for the round shapes it has
            let depth: f32 = (high[0] - low[0]).min(high[1] - low[1]) * 0.5;
            let margin: f32 = 2.0 / cellular_tile_size;
            let corners: [[f32; 2]; 6] = [
                [low[0] - margin, low[1] - margin],
                [high[0] + margin, low[1] - margin],
                [high[0] + margin, high[1] + margin],
                [low[0] - margin, low[1] - margin],
                [high[0] + margin, high[1] + margin],
                [low[0] - margin, high[1] + margin],
            ];
            let start: u32 = (self.vertex_bytes.len() / VERTEX_BYTE_COUNT) as u32;
            for corner in corners {
                let snapped: f32 = if body.is_cell_snapped { 1.0 } else { 0.0 };
                for value in [corner[0], corner[1], first, count, snapped, depth, center[0], center[1]] {
                    self.vertex_bytes.extend_from_slice(&value.to_le_bytes());
                }
                for parameter in &body.parameters {
                    for value in parameter { self.vertex_bytes.extend_from_slice(&value.to_le_bytes()); }
                }
            }
            let end: u32 = (self.vertex_bytes.len() / VERTEX_BYTE_COUNT) as u32;
            self.draws.push((body.fragment.to_owned(), start..end));
            let key: (String, wgpu::TextureFormat) = (body.fragment.to_owned(), format);
            if self.pipelines.contains_key(&key) && self.optics_pipelines.contains_key(body.fragment) {
                continue;
            }
            let source: String = lighting_shader_source(
                1,
                &[include_str!("scene_renderer_shaded.wgsl"), body.fragment].join("\n"),
            );
            let module: wgpu::ShaderModule = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("`SceneRendererShadedPass` shader"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            const ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
                0 => Float32x2,
                1 => Float32x4,
                2 => Float32x2,
                3 => Float32x4,
                4 => Float32x4,
                5 => Float32x4,
                6 => Float32x4,
            ];
            let pipeline_layout: &wgpu::PipelineLayout = &self.pipeline_layout;
            let create_pipeline = |
                vertex_entry_point: &str,
                fragment_entry_point: &str,
                format: wgpu::TextureFormat,
                blend: wgpu::BlendState,
            | -> wgpu::RenderPipeline {
                engine_host::acceleration::create_render_pipeline(device, &wgpu::RenderPipelineDescriptor {
                    label: Some("`SceneRendererShadedPass.pipelines`"),
                    layout: Some(pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &module,
                        entry_point: Some(vertex_entry_point),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: VERTEX_BYTE_COUNT as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &ATTRIBUTES,
                        })],
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &module,
                        entry_point: Some(fragment_entry_point),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(blend),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
            let pipeline: wgpu::RenderPipeline = create_pipeline(
                "vertex",
                "fragment",
                format,
                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            );
            let optics_pipeline: wgpu::RenderPipeline = create_pipeline(
                "vertex_optics",
                "fragment_optics",
                LIGHTING_OPTICS_FORMAT,
                LIGHTING_OPTICS_BLEND,
            );
            self.pipelines.insert(key, pipeline);
            self.optics_pipelines.insert(body.fragment.to_owned(), optics_pipeline);
        }
        let mut uniforms: [u8; 32] = [0; 32];
        for (index, value) in [
            viewport[0],
            viewport[1],
            pixels_per_tile,
            cellular_tile_size,
            cell_grid_origin[0],
            cell_grid_origin[1],
        ].into_iter().enumerate() {
            uniforms[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniforms);
        if self.vertex_bytes.len() > self.vertex_buffer_capacity {
            self.vertex_buffer_capacity = self.vertex_bytes.len().next_power_of_two();
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("`SceneRendererShadedPass.vertex_buffer`"),
                size: self.vertex_buffer_capacity as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !self.vertex_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.vertex_buffer, 0, &self.vertex_bytes);
        }
        if self.outline_bytes.len() > self.outline_buffer_capacity {
            self.outline_buffer_capacity = self.outline_bytes.len().next_power_of_two();
            self.outline_buffer = create_outline_buffer(device, self.outline_buffer_capacity);
            self.bind_group = create_bind_group(
                device,
                &self.bind_group_layout,
                &self.uniform_buffer,
                &self.outline_buffer,
            );
        }
        if !self.outline_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.outline_buffer, 0, &self.outline_bytes);
        }
    }

    /// Draws this frame's bodies into the lighting domain, as emission and occlusion
    pub fn draw_optics(&self, render_pass: &mut wgpu::RenderPass<'_>, lighting_bind_group: &wgpu::BindGroup) {
        if self.draws.is_empty() { return; }
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, lighting_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for (fragment, range) in &self.draws {
            let Some(pipeline) = self.optics_pipelines.get(fragment) else { continue; };
            render_pass.set_pipeline(pipeline);
            render_pass.draw(range.clone(), 0..1);
        }
    }

    /// Draws this frame's bodies
    pub fn draw(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        format: wgpu::TextureFormat,
        lighting_bind_group: &wgpu::BindGroup,
    ) {
        if self.draws.is_empty() { return; }
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, lighting_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for (fragment, range) in &self.draws {
            let Some(pipeline) = self.pipelines.get(&(fragment.clone(), format)) else { continue; };
            render_pass.set_pipeline(pipeline);
            render_pass.draw(range.clone(), 0..1);
        }
    }

}
