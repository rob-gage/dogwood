// Copyright Rob Gage 2026

use super::create_render_shader_module;
use engine_compute::Accelerator;
use engine_physics::scenes::Scene;

/// Handles the rendering of `Scene`s
pub struct SceneRenderer {
    /// The format of the current render pipeline
    format: Option<wgpu::TextureFormat>,
    /// The layout of scene render resources
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    /// The scene render pipeline
    pipeline: Option<wgpu::RenderPipeline>,
    /// The scene render uniforms
    uniform_buffer: Option<wgpu::Buffer>,
}

impl SceneRenderer {
    /// Creates a `SceneRenderer`
    pub const fn new() -> Self {
        Self {
            format: None,
            bind_group_layout: None,
            pipeline: None,
            uniform_buffer: None,
        }
    }

    /// Renders a `Scene`
    pub fn render(
        &mut self,
        accelerator: &Accelerator,
        scene: Option<&Scene>,
        format: wgpu::TextureFormat,
        viewport: [u32; 4],
        camera_position: [f32; 2],
        camera_size: [f32; 2],
        editor_viewport: bool,
        view_mode: u32,
        show_tile_borders: bool,
        show_chunk_borders: bool,
        command_encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        if self.format != Some(format) {
            let device: &wgpu::Device = accelerator.wgpu_device();
            let shader: wgpu::ShaderModule = create_render_shader_module(
                device,
                "Scene shader",
                include_str!("scene.wgsl"),
                "engine/src/renders/scene.wgsl",
            );
            let bind_group_layout: wgpu::BindGroupLayout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Scene bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 6,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 7,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 8,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 9,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 10,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 11,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 12,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 13,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });
            let pipeline_layout: wgpu::PipelineLayout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Scene pipeline layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });
            self.pipeline = Some(
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Scene pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("render_scene_fullscreen_triangle_vertex"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("render_scene_fragment"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                }),
            );
            self.bind_group_layout = Some(bind_group_layout);
            self.uniform_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Scene uniforms"),
                size: 96,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.format = Some(format);
        }
        let (Some(bind_group_layout), Some(pipeline), Some(uniform_buffer)) = (
            self.bind_group_layout.as_ref(),
            self.pipeline.as_ref(),
            self.uniform_buffer.as_ref(),
        ) else {
            return;
        };
        let bind_group: Option<wgpu::BindGroup> = scene.map(|scene| {
            let graphics = scene.graphics();
            let (walking_pawn_position, walking_pawn_size): ([f32; 2], [f32; 2]) =
                graphics.walking_pawn.unwrap_or(([0.0; 2], [0.0; 2]));
            let uniforms: [u32; 24] = [
                camera_position[0].to_bits(),
                camera_position[1].to_bits(),
                (viewport[2] as f32).to_bits(),
                (viewport[3] as f32).to_bits(),
                camera_size[0].to_bits(),
                camera_size[1].to_bits(),
                walking_pawn_position[0].to_bits(),
                walking_pawn_position[1].to_bits(),
                graphics.buffered_origin[0] as u32,
                graphics.buffered_origin[1] as u32,
                graphics.buffered_tile_size[0],
                graphics.buffered_tile_size[1],
                graphics.ring_offset[0],
                graphics.ring_offset[1],
                walking_pawn_size[0].to_bits(),
                walking_pawn_size[1].to_bits(),
                (viewport[0] as f32).to_bits(),
                (viewport[1] as f32).to_bits(),
                view_mode,
                show_tile_borders.into(),
                show_chunk_borders.into(),
                graphics.gas_count,
                0,
                0,
            ];
            let mut uniform_data: Vec<u8> = Vec::with_capacity(96);
            for value in uniforms {
                uniform_data.extend_from_slice(&value.to_le_bytes());
            }
            accelerator
                .wgpu_queue()
                .write_buffer(uniform_buffer, 0, &uniform_data);
            accelerator
                .wgpu_device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Scene bind group"),
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: graphics
                                .cellular_material_identifiers
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: graphics
                                .cellular_appearances
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: graphics
                                .material_graphics
                                .cellular_statics
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: graphics
                                .material_graphics
                                .cellular_dynamics
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: graphics
                                .material_graphics
                                .fluids
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 6,
                            resource: graphics
                                .fluid_material_identifiers
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 7,
                            resource: graphics.fluid_coverage.wgpu_buffer().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 8,
                            resource: graphics.cellular_pressure.wgpu_buffer().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 9,
                            resource: graphics
                                .material_graphics
                                .gases
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 10,
                            resource: graphics
                                .material_graphics
                                .gas_properties
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 11,
                            resource: graphics
                                .gas_concentrations
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 12,
                            resource: graphics
                                .rigid_material_identifiers
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 13,
                            resource: graphics.rigid_appearances.wgpu_buffer().as_entire_binding(),
                        },
                    ],
                })
        });
        let mut render_pass: wgpu::RenderPass =
            command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if editor_viewport {
                            wgpu::Color {
                                r: 30.0 / 255.0,
                                g: 32.0 / 255.0,
                                b: 36.0 / 255.0,
                                a: 1.0,
                            }
                        } else {
                            wgpu::Color::BLACK
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: accelerator.render_pass_timestamp_writes("Scene render pass"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
        if let Some(bind_group) = bind_group.as_ref() {
            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.set_viewport(
                viewport[0] as f32,
                viewport[1] as f32,
                viewport[2] as f32,
                viewport[3] as f32,
                0.0,
                1.0,
            );
            render_pass.set_scissor_rect(viewport[0], viewport[1], viewport[2], viewport[3]);
            render_pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use engine_graphics::{Color, MaterialAppearance};
    use engine_physics::{
        materials::{Material, MaterialRegistry},
        simulation::SceneSimulationConfiguration,
    };
    use std::sync::Arc;

    #[test]
    fn scene_shader_builds_without_a_window_surface() {
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        materials.register(Material::Gas {
            name: "Test Gas".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
            density: 0.7,
            diffusivity: 0.1,
            extinction: 0.1,
            dissipation: 0.0,
            compressibility: 0.05,
        });
        let scene: Scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -1.0],
                width: 1,
                height: 1,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let texture: wgpu::Texture =
            accelerator
                .wgpu_device()
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("scene shader test target"),
                    size: wgpu::Extent3d {
                        width: 4,
                        height: 4,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
        let view: wgpu::TextureView = texture.create_view(&Default::default());
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("scene shader test"),
                });
        SceneRenderer::new().render(
            accelerator.as_ref(),
            Some(&scene),
            wgpu::TextureFormat::Rgba8Unorm,
            [0, 0, 4, 4],
            [0.0; 2],
            [1.0; 2],
            true,
            0,
            false,
            false,
            &mut encoder,
            &view,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}
