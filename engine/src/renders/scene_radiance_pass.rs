// Copyright Rob Gage 2026

use engine_compute::{Accelerator, AcceleratorBuffer};
use engine_graphics::SceneGraphics;
use engine_physics::scenes::Scene;

const CASCADE_COUNT: usize = 5;
const PROBE_SPACING: u32 = 4;
const DIRECTION_COUNT: u32 = 4;
const INTERVAL_LENGTH: f32 = 8.0;

/// Persistent optical resolve and Radiance Cascades resources for one scene view.
pub(super) struct SceneRadiancePass {
    size: Option<[u32; 2]>,
    optical: Option<(wgpu::Texture, wgpu::TextureView)>,
    optical_pipeline: Option<wgpu::RenderPipeline>,
    optical_layout: Option<wgpu::BindGroupLayout>,
    optical_uniform: Option<wgpu::Buffer>,
    trace_pipeline: Option<wgpu::ComputePipeline>,
    trace_layout: Option<wgpu::BindGroupLayout>,
    merge_pipeline: Option<wgpu::ComputePipeline>,
    merge_layout: Option<wgpu::BindGroupLayout>,
    integrate_pipeline: Option<wgpu::ComputePipeline>,
    integrate_layout: Option<wgpu::BindGroupLayout>,
    configurations: Vec<wgpu::Buffer>,
    intervals: Vec<AcceleratorBuffer>,
    counts: Vec<u32>,
    trace_groups: Vec<wgpu::BindGroup>,
    merge_groups: Vec<wgpu::BindGroup>,
    integrate_groups: Vec<wgpu::BindGroup>,
    illumination: Option<(wgpu::Texture, wgpu::TextureView)>,
}

impl SceneRadiancePass {
    pub(super) const fn new() -> Self {
        Self {
            size: None,
            optical: None,
            optical_pipeline: None,
            optical_layout: None,
            optical_uniform: None,
            trace_pipeline: None,
            trace_layout: None,
            merge_pipeline: None,
            merge_layout: None,
            integrate_pipeline: None,
            integrate_layout: None,
            configurations: Vec::new(),
            intervals: Vec::new(),
            counts: Vec::new(),
            trace_groups: Vec::new(),
            merge_groups: Vec::new(),
            integrate_groups: Vec::new(),
            illumination: None,
        }
    }

    pub(super) fn size(&self) -> Option<[u32; 2]> {
        self.size
    }

    pub(super) fn illumination(&self) -> Option<&wgpu::TextureView> {
        self.illumination.as_ref().map(|(_, view)| view)
    }

    pub(super) fn compute(
        &mut self,
        accelerator: &Accelerator,
        scene: Option<&Scene>,
        size: [u32; 2],
        camera_position: [f32; 2],
        camera_size: [f32; 2],
        command_encoder: &mut wgpu::CommandEncoder,
    ) {
        self.initialize(accelerator);
        self.resize(accelerator, size);
        let (
            Some(optical_pipeline),
            Some(optical_layout),
            Some(optical_uniform),
            Some((_, optical_view)),
        ) = (
            self.optical_pipeline.as_ref(),
            self.optical_layout.as_ref(),
            self.optical_uniform.as_ref(),
            self.optical.as_ref(),
        )
        else {
            return;
        };
        let Some(scene) = scene else {
            return;
        };
        let graphics: SceneGraphics<'_> = scene.graphics();
        let uniforms: [u32; 16] = [
            camera_position[0].to_bits(),
            camera_position[1].to_bits(),
            (size[0] as f32).to_bits(),
            (size[1] as f32).to_bits(),
            camera_size[0].to_bits(),
            camera_size[1].to_bits(),
            0,
            0,
            graphics.buffered_origin[0] as u32,
            graphics.buffered_origin[1] as u32,
            graphics.buffered_tile_size[0],
            graphics.buffered_tile_size[1],
            graphics.ring_offset[0],
            graphics.ring_offset[1],
            0,
            0,
        ];
        accelerator.wgpu_queue().write_buffer(
            optical_uniform,
            0,
            &uniforms
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let optical_group =
            accelerator
                .wgpu_device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Scene optical resolve bind group"),
                    layout: optical_layout,
                    entries: &[
                        Self::buffer_entry(0, graphics.cellular_material_identifiers),
                        Self::buffer_entry(1, graphics.cellular_appearances),
                        Self::buffer_entry(2, &graphics.material_graphics.cellular_statics),
                        Self::buffer_entry(3, &graphics.material_graphics.cellular_dynamics),
                        Self::buffer_entry(4, &graphics.material_graphics.fluids),
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: optical_uniform.as_entire_binding(),
                        },
                        Self::buffer_entry(6, graphics.fluid_material_identifiers),
                        Self::buffer_entry(7, graphics.fluid_coverage),
                        Self::buffer_entry(8, graphics.cellular_pressure),
                        Self::buffer_entry(9, &graphics.material_graphics.gases),
                        Self::buffer_entry(10, &graphics.material_graphics.gas_properties),
                        Self::buffer_entry(11, graphics.gas_concentrations),
                        Self::buffer_entry(12, graphics.rigid_material_identifiers),
                        Self::buffer_entry(13, graphics.rigid_appearances),
                    ],
                });
        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Scene optical resolve pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: optical_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render_pass.set_pipeline(optical_pipeline);
        render_pass.set_bind_group(0, &optical_group, &[]);
        render_pass.set_viewport(0.0, 0.0, size[0] as f32, size[1] as f32, 0.0, 1.0);
        render_pass.set_scissor_rect(0, 0, size[0], size[1]);
        render_pass.draw(0..3, 0..1);
        drop(render_pass);

        let mut compute_pass = command_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Scene radiance cascades"),
            timestamp_writes: None,
        });
        let (Some(trace), Some(merge), Some(integrate)) = (
            self.trace_pipeline.as_ref(),
            self.merge_pipeline.as_ref(),
            self.integrate_pipeline.as_ref(),
        ) else {
            return;
        };
        compute_pass.set_pipeline(trace);
        for (group, count) in self.trace_groups.iter().zip(&self.counts) {
            compute_pass.set_bind_group(0, group, &[]);
            compute_pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        }
        compute_pass.set_pipeline(merge);
        for index in (0..self.merge_groups.len()).rev() {
            compute_pass.set_bind_group(0, &self.merge_groups[index], &[]);
            compute_pass.dispatch_workgroups(self.counts[index].div_ceil(64), 1, 1);
        }
        compute_pass.set_pipeline(integrate);
        if let Some(group) = self.integrate_groups.first() {
            compute_pass.set_bind_group(0, group, &[]);
            compute_pass.dispatch_workgroups((size[0] * size[1]).div_ceil(64), 1, 1);
        }
    }

    fn initialize(&mut self, accelerator: &Accelerator) {
        if self.optical_pipeline.is_some() {
            return;
        }
        let device = accelerator.wgpu_device();
        let optical_shader = engine_physics::simulation::create_physics_shader_module(
            device,
            "Scene optical shader",
            include_str!("scene_optical.wgsl"),
            "engine/src/renders/scene_optical.wgsl",
        );
        let optical_entries: Vec<wgpu::BindGroupLayoutEntry> = (0..14)
            .map(|binding| {
                if binding == 5 {
                    wgpu::BindGroupLayoutEntry {
                        binding,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }
                } else {
                    Self::buffer_layout_entry(binding)
                }
            })
            .collect();
        let optical_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene optical resolve layout"),
            entries: &optical_entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Scene optical resolve pipeline layout"),
            bind_group_layouts: &[Some(&optical_layout)],
            immediate_size: 0,
        });
        self.optical_pipeline = Some(device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Scene optical resolve pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &optical_shader,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &optical_shader,
                    entry_point: Some("fragment"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            },
        ));
        self.optical_layout = Some(optical_layout);
        self.optical_uniform = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Scene optical uniforms"),
            size: 128,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Scene radiance shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene_radiance.wgsl").into()),
        });
        let trace_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene radiance trace layout"),
            entries: &[
                Self::texture_layout_entry(0),
                Self::uniform_layout_entry(3),
                Self::storage_layout_entry(4, false),
            ],
        });
        let merge_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene radiance merge layout"),
            entries: &[
                Self::uniform_layout_entry(5),
                Self::storage_layout_entry(6, false),
                Self::storage_layout_entry(7, true),
            ],
        });
        let integrate_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene radiance integrate layout"),
            entries: &[
                Self::storage_layout_entry(8, true),
                Self::uniform_layout_entry(9),
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        self.trace_pipeline = Some(Self::compute_pipeline(
            device,
            &shader,
            &trace_layout,
            "trace",
        ));
        self.merge_pipeline = Some(Self::compute_pipeline(
            device,
            &shader,
            &merge_layout,
            "merge",
        ));
        self.integrate_pipeline = Some(Self::compute_pipeline(
            device,
            &shader,
            &integrate_layout,
            "integrate",
        ));
        self.trace_layout = Some(trace_layout);
        self.merge_layout = Some(merge_layout);
        self.integrate_layout = Some(integrate_layout);
    }

    fn resize(&mut self, accelerator: &Accelerator, size: [u32; 2]) {
        if self.size == Some(size) {
            return;
        }
        let device = accelerator.wgpu_device();
        self.optical = Some(Self::texture(
            accelerator,
            size,
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            "Scene optical field",
        ));
        self.illumination = Some(Self::texture(
            accelerator,
            size,
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            "Scene illumination",
        ));
        self.configurations.clear();
        self.intervals.clear();
        self.counts.clear();
        self.trace_groups.clear();
        self.merge_groups.clear();
        self.integrate_groups.clear();
        let Some((_, optical_view)) = self.optical.as_ref() else {
            return;
        };
        let mut start = 0.0;
        for cascade in 0..CASCADE_COUNT {
            let spacing = PROBE_SPACING << cascade;
            let directions = DIRECTION_COUNT << (cascade * 2);
            let probe_size = [size[0].div_ceil(spacing) + 2, size[1].div_ceil(spacing) + 2];
            let end = start + INTERVAL_LENGTH * 4.0_f32.powi(cascade as i32);
            let config = Self::config(size, probe_size, spacing, directions, start, end);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Scene cascade configuration"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            accelerator.wgpu_queue().write_buffer(
                &buffer,
                0,
                &config
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            let count = probe_size[0] * probe_size[1] * directions;
            self.configurations.push(buffer);
            self.intervals
                .push(accelerator.allocate::<[f32; 4]>(count as usize));
            self.counts.push(count);
            start = end;
        }
        let Some(trace_layout) = self.trace_layout.as_ref() else {
            return;
        };
        for cascade in 0..CASCADE_COUNT {
            self.trace_groups
                .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Scene cascade trace bind group"),
                    layout: trace_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(optical_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: self.configurations[cascade].as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: self.intervals[cascade].wgpu_buffer().as_entire_binding(),
                        },
                    ],
                }));
        }
        let Some(merge_layout) = self.merge_layout.as_ref() else {
            return;
        };
        for cascade in 0..CASCADE_COUNT - 1 {
            self.merge_groups.push(
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Scene cascade merge bind group"),
                    layout: merge_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: self.configurations[cascade].as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 6,
                            resource: self.intervals[cascade].wgpu_buffer().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 7,
                            resource: self.intervals[cascade + 1]
                                .wgpu_buffer()
                                .as_entire_binding(),
                        },
                    ],
                }),
            );
        }
        let Some(integrate_layout) = self.integrate_layout.as_ref() else {
            return;
        };
        let Some((_, illumination_view)) = self.illumination.as_ref() else {
            return;
        };
        self.integrate_groups
            .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Scene illumination integrate bind group"),
                layout: integrate_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: self.intervals[0].wgpu_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: self.configurations[0].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: wgpu::BindingResource::TextureView(illumination_view),
                    },
                ],
            }));
        self.size = Some(size);
    }

    fn config(
        size: [u32; 2],
        probes: [u32; 2],
        spacing: u32,
        directions: u32,
        start: f32,
        end: f32,
    ) -> [u32; 16] {
        [
            size[0],
            size[1],
            probes[0],
            probes[1],
            spacing,
            directions,
            start.to_bits(),
            end.to_bits(),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]
    }

    fn texture(
        accelerator: &Accelerator,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
        label: &'static str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = accelerator
            .wgpu_device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn compute_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::BindGroupLayout,
        entry: &'static str,
    ) -> wgpu::ComputePipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Scene radiance pipeline layout"),
            bind_group_layouts: &[Some(layout)],
            immediate_size: 0,
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: Some(&pipeline_layout),
            module: shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    fn buffer_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }

    fn buffer_entry<'a>(binding: u32, buffer: &'a AcceleratorBuffer) -> wgpu::BindGroupEntry<'a> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }

    fn texture_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }
    }

    fn uniform_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }

    fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }
}
