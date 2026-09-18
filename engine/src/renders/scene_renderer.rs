// Copyright Rob Gage 2026

use super::create_render_shader_module;
use super::scene_radiance_pass::SceneRadiancePass;
use engine_compute::Accelerator;
use engine_graphics::{SceneGraphics, SceneOverlay};
use engine_physics::scenes::Scene;

/// Handles the rendering of `Scene`s
pub struct SceneRenderer {
    /// Persistent optical resolve and radiance-cascade resources.
    radiance_pass: SceneRadiancePass,
    /// The format of the current render pipeline
    format: Option<wgpu::TextureFormat>,
    /// The layout of scene render resources
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    /// The scene render pipeline
    pipeline: Option<wgpu::RenderPipeline>,
    /// The scene render uniforms
    uniform_buffer: Option<wgpu::Buffer>,
    /// Growable storage for active generic actor graphics.
    actor_buffer: Option<wgpu::Buffer>,
    actor_buffer_capacity: usize,
    /// Growable storage for transient world overlays.
    overlay_buffer: Option<wgpu::Buffer>,
    overlay_buffer_capacity: usize,
}

impl SceneRenderer {
    /// Creates a `SceneRenderer`
    pub const fn new() -> Self {
        Self {
            radiance_pass: SceneRadiancePass::new(),
            format: None,
            bind_group_layout: None,
            pipeline: None,
            uniform_buffer: None,
            actor_buffer: None,
            actor_buffer_capacity: 0,
            overlay_buffer: None,
            overlay_buffer_capacity: 0,
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
        overlays: &[SceneOverlay],
        command_encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let lighting_size: [u32; 2] = [
            viewport[2].max(1).div_ceil(2),
            viewport[3].max(1).div_ceil(2),
        ];
        self.radiance_pass.compute(
            accelerator,
            scene,
            lighting_size,
            camera_position,
            camera_size,
            command_encoder,
        );
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 14,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 15,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 16,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
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
                size: 128,
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
        let bind_group: Option<wgpu::BindGroup> =
            self.radiance_pass.illumination().and_then(|illumination| {
                scene.map(|scene| {
                    let graphics: SceneGraphics<'_> = scene.graphics();
                    let (walking_pawn_position, walking_pawn_size): ([f32; 2], [f32; 2]) =
                        graphics.walking_pawn.unwrap_or(([0.0; 2], [0.0; 2]));
                    let uniforms: [u32; 32] = [
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
                        graphics.actors.len() as u32,
                        overlays.len() as u32,
                        0,
                        0,
                        self.radiance_pass.size().unwrap_or([0; 2])[0],
                        self.radiance_pass.size().unwrap_or([0; 2])[1],
                        0,
                        0,
                    ];
                    let required_actor_capacity: usize = graphics.actors.len().max(1) * 32;
                    if self.actor_buffer_capacity < required_actor_capacity {
                        self.actor_buffer = Some(accelerator.wgpu_device().create_buffer(
                            &wgpu::BufferDescriptor {
                                label: Some("Scene actor graphics"),
                                size: required_actor_capacity as u64,
                                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                                mapped_at_creation: false,
                            },
                        ));
                        self.actor_buffer_capacity = required_actor_capacity;
                    }
                    let actor_data: Vec<u8> = graphics
                        .actors
                        .iter()
                        .flat_map(|actor| {
                            actor
                                .position
                                .into_iter()
                                .chain(actor.size)
                                .chain(actor.color)
                                .flat_map(f32::to_le_bytes)
                        })
                        .collect();
                    if !actor_data.is_empty() {
                        accelerator.wgpu_queue().write_buffer(
                            self.actor_buffer.as_ref().unwrap(),
                            0,
                            &actor_data,
                        );
                    }
                    let overlay_data: Vec<u8> = overlays
                        .iter()
                        .flat_map(|overlay| match overlay {
                            SceneOverlay::CircleOutline {
                                center,
                                radius,
                                color,
                            } => center
                                .iter()
                                .copied()
                                .chain([*radius, 0.0])
                                .chain([
                                    f32::from(color.red()) / 255.0,
                                    f32::from(color.green()) / 255.0,
                                    f32::from(color.blue()) / 255.0,
                                    f32::from(color.alpha()) / 255.0,
                                ])
                                .flat_map(f32::to_le_bytes),
                        })
                        .collect();
                    let required_overlay_capacity: usize = overlays.len().max(1) * 32;
                    if self.overlay_buffer_capacity < required_overlay_capacity {
                        self.overlay_buffer = Some(accelerator.wgpu_device().create_buffer(
                            &wgpu::BufferDescriptor {
                                label: Some("Scene overlays"),
                                size: required_overlay_capacity as u64,
                                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                                mapped_at_creation: false,
                            },
                        ));
                        self.overlay_buffer_capacity = required_overlay_capacity;
                    }
                    if !overlay_data.is_empty() {
                        accelerator.wgpu_queue().write_buffer(
                            self.overlay_buffer.as_ref().unwrap(),
                            0,
                            &overlay_data,
                        );
                    }
                    let mut uniform_data: Vec<u8> = Vec::with_capacity(128);
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
                                    resource: graphics
                                        .fluid_coverage
                                        .wgpu_buffer()
                                        .as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 8,
                                    resource: graphics
                                        .cellular_pressure
                                        .wgpu_buffer()
                                        .as_entire_binding(),
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
                                    resource: graphics
                                        .rigid_appearances
                                        .wgpu_buffer()
                                        .as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 14,
                                    resource: self
                                        .actor_buffer
                                        .as_ref()
                                        .unwrap()
                                        .as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 15,
                                    resource: self
                                        .overlay_buffer
                                        .as_ref()
                                        .unwrap()
                                        .as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 16,
                                    resource: wgpu::BindingResource::TextureView(illumination),
                                },
                            ],
                        })
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
