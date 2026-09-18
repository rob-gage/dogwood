// Copyright Rob Gage 2026

use engine_compute::Accelerator;

struct DistanceJump {
    _uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub(super) struct SceneDistancePass {
    size: Option<[u32; 2]>,
    seed_pipeline: Option<wgpu::ComputePipeline>,
    seed_layout: Option<wgpu::BindGroupLayout>,
    seed_group: Option<wgpu::BindGroup>,
    jump_pipeline: Option<wgpu::ComputePipeline>,
    jump_layout: Option<wgpu::BindGroupLayout>,
    jumps: Vec<DistanceJump>,
    nearest: Option<[(wgpu::Texture, wgpu::TextureView); 2]>,
    finalize_pipeline: Option<wgpu::ComputePipeline>,
    finalize_layout: Option<wgpu::BindGroupLayout>,
    finalize_group: Option<wgpu::BindGroup>,
    distance: Option<(wgpu::Texture, wgpu::TextureView)>,
}

impl SceneDistancePass {
    pub(super) const fn new() -> Self {
        Self {
            size: None,
            seed_pipeline: None,
            seed_layout: None,
            seed_group: None,
            jump_pipeline: None,
            jump_layout: None,
            jumps: Vec::new(),
            nearest: None,
            finalize_pipeline: None,
            finalize_layout: None,
            finalize_group: None,
            distance: None,
        }
    }

    pub(super) fn distance(&self) -> Option<&wgpu::TextureView> {
        self.distance.as_ref().map(|(_, view)| view)
    }

    pub(super) fn compute(
        &mut self,
        accelerator: &Accelerator,
        size: [u32; 2],
        optical: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        self.initialize(accelerator);
        self.resize(accelerator, size, optical);
        let (Some(seed), Some(seed_group), Some(jump), Some(finalize), Some(finalize_group)) = (
            self.seed_pipeline.as_ref(),
            self.seed_group.as_ref(),
            self.jump_pipeline.as_ref(),
            self.finalize_pipeline.as_ref(),
            self.finalize_group.as_ref(),
        ) else {
            return;
        };
        let groups = [size[0].div_ceil(8), size[1].div_ceil(8)];
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Scene optical distance field"),
            timestamp_writes: None,
        });
        pass.set_pipeline(seed);
        pass.set_bind_group(0, seed_group, &[]);
        pass.dispatch_workgroups(groups[0], groups[1], 1);
        pass.set_pipeline(jump);
        for jump in &self.jumps {
            pass.set_bind_group(0, &jump.bind_group, &[]);
            pass.dispatch_workgroups(groups[0], groups[1], 1);
        }
        pass.set_pipeline(finalize);
        pass.set_bind_group(0, finalize_group, &[]);
        pass.dispatch_workgroups(groups[0], groups[1], 1);
    }

    fn initialize(&mut self, accelerator: &Accelerator) {
        if self.seed_pipeline.is_some() {
            return;
        }
        let device = accelerator.wgpu_device();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Scene optical distance shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene_distance.wgsl").into()),
        });
        let seed_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene distance seed layout"),
            entries: &[
                Self::texture(0, wgpu::TextureSampleType::Float { filterable: false }),
                Self::storage_texture(1, wgpu::TextureFormat::Rg32Sint),
            ],
        });
        let jump_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene distance jump layout"),
            entries: &[
                Self::texture(2, wgpu::TextureSampleType::Sint),
                Self::storage_texture(3, wgpu::TextureFormat::Rg32Sint),
                Self::uniform(4),
            ],
        });
        let finalize_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene distance finalize layout"),
            entries: &[
                Self::texture(5, wgpu::TextureSampleType::Sint),
                Self::storage_texture(6, wgpu::TextureFormat::R32Float),
            ],
        });
        self.seed_pipeline = Some(Self::pipeline(device, &shader, &seed_layout, "seed"));
        self.jump_pipeline = Some(Self::pipeline(device, &shader, &jump_layout, "jump"));
        self.finalize_pipeline = Some(Self::pipeline(
            device,
            &shader,
            &finalize_layout,
            "finalize",
        ));
        self.seed_layout = Some(seed_layout);
        self.jump_layout = Some(jump_layout);
        self.finalize_layout = Some(finalize_layout);
    }

    fn resize(&mut self, accelerator: &Accelerator, size: [u32; 2], optical: &wgpu::TextureView) {
        if self.size == Some(size) {
            return;
        }
        let device = accelerator.wgpu_device();
        let nearest = [
            Self::texture_resource(
                accelerator,
                size,
                wgpu::TextureFormat::Rg32Sint,
                "Distance A",
            ),
            Self::texture_resource(
                accelerator,
                size,
                wgpu::TextureFormat::Rg32Sint,
                "Distance B",
            ),
        ];
        let distance = Self::texture_resource(
            accelerator,
            size,
            wgpu::TextureFormat::R32Float,
            "Distance field",
        );
        let Some(seed_layout) = self.seed_layout.as_ref() else {
            return;
        };
        self.seed_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene distance seed bind group"),
            layout: seed_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(optical),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&nearest[0].1),
                },
            ],
        }));
        let Some(jump_layout) = self.jump_layout.as_ref() else {
            return;
        };
        self.jumps.clear();
        let mut step = size[0].max(size[1]).next_power_of_two() / 2;
        let mut index = 0;
        while step > 0 {
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Scene distance jump uniform"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            accelerator
                .wgpu_queue()
                .write_buffer(&uniform, 0, &step.to_le_bytes());
            let source = index % 2;
            let destination = (index + 1) % 2;
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Scene distance jump bind group"),
                layout: jump_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&nearest[source].1),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&nearest[destination].1),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            });
            self.jumps.push(DistanceJump {
                _uniform: uniform,
                bind_group,
            });
            step /= 2;
            index += 1;
        }
        let final_nearest = &nearest[index % 2].1;
        let Some(finalize_layout) = self.finalize_layout.as_ref() else {
            return;
        };
        self.finalize_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene distance finalize bind group"),
            layout: finalize_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(final_nearest),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&distance.1),
                },
            ],
        }));
        self.nearest = Some(nearest);
        self.distance = Some(distance);
        self.size = Some(size);
    }

    fn pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::BindGroupLayout,
        entry: &'static str,
    ) -> wgpu::ComputePipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Scene distance pipeline layout"),
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

    fn texture(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }
    }

    fn storage_texture(binding: u32, format: wgpu::TextureFormat) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        }
    }

    fn uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

    fn texture_resource(
        accelerator: &Accelerator,
        size: [u32; 2],
        format: wgpu::TextureFormat,
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
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }
}
