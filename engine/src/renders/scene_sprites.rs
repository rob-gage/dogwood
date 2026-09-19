// Copyright Rob Gage 2026

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::Arc;

use engine_compute::Accelerator;
use engine_graphics::{SceneSpriteGraphics, SceneSpriteSheetGraphics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SpriteTextureKey {
    address: usize,
    width: u32,
    height: u32,
}

struct SpriteTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    _source: Arc<[u8]>,
}

struct SpriteTextureCache {
    textures: HashMap<SpriteTextureKey, SpriteTexture>,
    black_texture: Option<SpriteTexture>,
}

impl SpriteTextureCache {
    fn new() -> Self {
        Self {
            textures: HashMap::new(),
            black_texture: None,
        }
    }

    fn key(sheet: &SceneSpriteSheetGraphics) -> SpriteTextureKey {
        SpriteTextureKey {
            address: sheet.rgba_data.as_ptr() as usize,
            width: sheet.width,
            height: sheet.height,
        }
    }

    fn get_or_upload<'a>(
        &'a mut self,
        accelerator: &Accelerator,
        sheet: &SceneSpriteSheetGraphics,
    ) -> &'a wgpu::TextureView {
        let key = Self::key(sheet);
        if !self.textures.contains_key(&key) {
            self.textures.insert(key, Self::upload(accelerator, sheet));
        }
        &self.textures.get(&key).unwrap().view
    }

    fn ensure_black(&mut self, accelerator: &Accelerator) {
        if self.black_texture.is_none() {
            let sheet = SceneSpriteSheetGraphics {
                width: 1,
                height: 1,
                rgba_data: vec![0, 0, 0, 0].into(),
            };
            self.black_texture = Some(Self::upload(accelerator, &sheet));
        }
    }

    fn black_view(&self) -> &wgpu::TextureView {
        &self.black_texture.as_ref().unwrap().view
    }

    fn view(&self, key: SpriteTextureKey) -> &wgpu::TextureView {
        &self.textures.get(&key).unwrap().view
    }

    fn upload(accelerator: &Accelerator, sheet: &SceneSpriteSheetGraphics) -> SpriteTexture {
        let texture = accelerator
            .wgpu_device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Actor sprite sheet"),
                size: wgpu::Extent3d {
                    width: sheet.width,
                    height: sheet.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
        accelerator.wgpu_queue().write_texture(
            texture.as_image_copy(),
            &sheet.rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(sheet.width * 4),
                rows_per_image: Some(sheet.height),
            },
            wgpu::Extent3d {
                width: sheet.width,
                height: sheet.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        SpriteTexture {
            _texture: texture,
            view,
            _source: Arc::clone(&sheet.rgba_data),
        }
    }
}

pub(super) struct SceneSpriteRenderer {
    cache: SpriteTextureCache,
    sampler: Option<wgpu::Sampler>,
    layout: Option<wgpu::BindGroupLayout>,
    uniform_buffer: Option<wgpu::Buffer>,
    instance_buffer: Option<wgpu::Buffer>,
    instance_buffer_capacity: usize,
    optical_pipeline: Option<wgpu::RenderPipeline>,
    visible_pipeline: Option<wgpu::RenderPipeline>,
    visible_format: Option<wgpu::TextureFormat>,
}

impl SceneSpriteRenderer {
    pub(super) fn new() -> Self {
        Self {
            cache: SpriteTextureCache::new(),
            sampler: None,
            layout: None,
            uniform_buffer: None,
            instance_buffer: None,
            instance_buffer_capacity: 0,
            optical_pipeline: None,
            visible_pipeline: None,
            visible_format: None,
        }
    }

    pub(super) fn render_optical(
        &mut self,
        accelerator: &Accelerator,
        sprites: &[SceneSpriteGraphics],
        lighting_origin: [i32; 2],
        lighting_size: [u32; 2],
        optical_view: &wgpu::TextureView,
        command_encoder: &mut wgpu::CommandEncoder,
    ) {
        let optical_sprites: Vec<SceneSpriteGraphics> = sprites
            .iter()
            .filter(|sprite| sprite.radiance_sprite_sheet.is_some())
            .cloned()
            .collect();
        let groups = self.prepare(
            accelerator,
            &optical_sprites,
            None,
            lighting_origin,
            lighting_size,
        );
        if groups.is_empty() {
            return;
        }
        let Some(pipeline) = self.optical_pipeline.as_ref() else {
            return;
        };
        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Actor sprite optical pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: optical_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render_pass.set_pipeline(pipeline);
        render_pass.set_viewport(
            0.0,
            0.0,
            lighting_size[0] as f32,
            lighting_size[1] as f32,
            0.0,
            1.0,
        );
        render_pass.set_scissor_rect(0, 0, lighting_size[0], lighting_size[1]);
        for group in groups {
            render_pass.set_bind_group(0, &group.bind_group, &[]);
            render_pass.draw(0..6, 0..group.instance_count);
        }
    }

    pub(super) fn render_visible(
        &mut self,
        accelerator: &Accelerator,
        sprites: &[SceneSpriteGraphics],
        camera_position: [f32; 2],
        camera_size: [f32; 2],
        viewport: [u32; 4],
        lighting_origin: [i32; 2],
        lighting_size: [u32; 2],
        illumination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        target: &wgpu::TextureView,
        command_encoder: &mut wgpu::CommandEncoder,
    ) {
        let groups = self.prepare(
            accelerator,
            sprites,
            Some((camera_position, camera_size, viewport, format, illumination)),
            lighting_origin,
            lighting_size,
        );
        if groups.is_empty() {
            return;
        }
        let Some(pipeline) = self.visible_pipeline.as_ref() else {
            return;
        };
        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Actor sprite visible pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render_pass.set_pipeline(pipeline);
        render_pass.set_viewport(
            viewport[0] as f32,
            viewport[1] as f32,
            viewport[2] as f32,
            viewport[3] as f32,
            0.0,
            1.0,
        );
        render_pass.set_scissor_rect(viewport[0], viewport[1], viewport[2], viewport[3]);
        for group in groups {
            render_pass.set_bind_group(0, &group.bind_group, &[]);
            render_pass.draw(0..6, 0..group.instance_count);
        }
    }

    fn prepare(
        &mut self,
        accelerator: &Accelerator,
        sprites: &[SceneSpriteGraphics],
        visible: Option<(
            [f32; 2],
            [f32; 2],
            [u32; 4],
            wgpu::TextureFormat,
            &wgpu::TextureView,
        )>,
        lighting_origin: [i32; 2],
        lighting_size: [u32; 2],
    ) -> Vec<SpriteBindGroup> {
        if sprites.is_empty() {
            return Vec::new();
        }
        self.layout(accelerator, visible.map(|value| value.3));
        let layout = self.layout.as_ref().unwrap();
        let sampler = self.sampler.as_ref().unwrap();
        let mut grouped: HashMap<
            (SpriteTextureKey, Option<SpriteTextureKey>),
            Vec<&SceneSpriteGraphics>,
        > = HashMap::new();
        for sprite in sprites {
            self.cache.get_or_upload(accelerator, &sprite.sprite_sheet);
            if let Some(sheet) = sprite.radiance_sprite_sheet.as_ref() {
                self.cache.get_or_upload(accelerator, sheet);
            }
            grouped
                .entry((
                    SpriteTextureCache::key(&sprite.sprite_sheet),
                    sprite
                        .radiance_sprite_sheet
                        .as_ref()
                        .map(SpriteTextureCache::key),
                ))
                .or_default()
                .push(sprite);
        }
        if grouped
            .keys()
            .any(|(_, radiance_key)| radiance_key.is_none())
        {
            self.cache.ensure_black(accelerator);
        }
        if visible.is_none() {
            self.cache.ensure_black(accelerator);
        }
        let mut result = Vec::with_capacity(grouped.len());
        let required_capacity = sprites.len() * 48;
        if self.instance_buffer_capacity < required_capacity {
            self.instance_buffer = Some(accelerator.wgpu_device().create_buffer(
                &wgpu::BufferDescriptor {
                    label: Some("Actor sprite instances"),
                    size: required_capacity.max(64) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            ));
            self.instance_buffer_capacity = required_capacity.max(64);
        }
        let mut instance_offset: u64 = 0;
        for ((normal_key, radiance_key), group) in grouped {
            let normal_view = self.cache.view(normal_key);
            let radiance_view = if radiance_key.is_some() {
                self.cache.view(radiance_key.unwrap())
            } else {
                self.cache.black_view()
            };
            let instance_data: Vec<u8> = group
                .iter()
                .flat_map(|sprite| {
                    sprite
                        .position
                        .into_iter()
                        .chain(sprite.world_size)
                        .chain(sprite.world_offset)
                        .chain([0.0, 0.0])
                        .chain(sprite.texture_coordinates)
                        .flat_map(f32::to_le_bytes)
                })
                .collect();
            let instance_buffer = self.instance_buffer.as_ref().unwrap();
            accelerator
                .wgpu_queue()
                .write_buffer(instance_buffer, instance_offset, &instance_data);
            let (camera_position, camera_size, viewport) = visible
                .map(|value| (value.0, value.1, value.2))
                .unwrap_or(([0.0; 2], [0.0; 2], [0; 4]));
            let illumination = visible
                .map(|value| value.4)
                .unwrap_or_else(|| self.cache.black_view());
            let uniforms: [u32; 16] = [
                camera_position[0].to_bits(),
                camera_position[1].to_bits(),
                (viewport[2] as f32).to_bits(),
                (viewport[3] as f32).to_bits(),
                camera_size[0].to_bits(),
                camera_size[1].to_bits(),
                (viewport[0] as f32).to_bits(),
                (viewport[1] as f32).to_bits(),
                lighting_origin[0] as u32,
                lighting_origin[1] as u32,
                lighting_size[0],
                lighting_size[1],
                u32::from(visible.is_some()),
                group.len() as u32,
                0,
                0,
            ];
            let uniform_buffer = self.uniform_buffer.as_ref().unwrap();
            accelerator.wgpu_queue().write_buffer(
                uniform_buffer,
                0,
                &uniforms
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            let bind_group =
                accelerator
                    .wgpu_device()
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Actor sprite bind group"),
                        layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: uniform_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: instance_buffer,
                                    offset: instance_offset,
                                    size: NonZeroU64::new(instance_data.len() as u64),
                                }),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(normal_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(radiance_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::Sampler(sampler),
                            },
                            wgpu::BindGroupEntry {
                                binding: 5,
                                resource: wgpu::BindingResource::TextureView(illumination),
                            },
                        ],
                    });
            result.push(SpriteBindGroup {
                bind_group,
                instance_count: group.len() as u32,
            });
            instance_offset += instance_data.len() as u64;
        }
        result
    }

    fn layout(&mut self, accelerator: &Accelerator, visible_format: Option<wgpu::TextureFormat>) {
        if self.layout.is_none() {
            let device = accelerator.wgpu_device();
            self.layout = Some(
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Actor sprite layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
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
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                }),
            );
            self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Actor sprite nearest sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            }));
            self.uniform_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Actor sprite uniforms"),
                size: 64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            let shader = engine_physics::simulation::create_physics_shader_module(
                device,
                "Actor sprite shader",
                include_str!("scene_sprites.wgsl"),
                "engine/src/renders/scene_sprites.wgsl",
            );
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Actor sprite pipeline layout"),
                bind_group_layouts: &[Some(self.layout.as_ref().unwrap())],
                immediate_size: 0,
            });
            self.optical_pipeline = Some(device.create_render_pipeline(
                &wgpu::RenderPipelineDescriptor {
                    label: Some("Actor sprite optical pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vertex"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("optical_fragment"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba16Float,
                            blend: Some(wgpu::BlendState {
                                color: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::One,
                                    dst_factor: wgpu::BlendFactor::One,
                                    operation: wgpu::BlendOperation::Add,
                                },
                                alpha: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::Zero,
                                    dst_factor: wgpu::BlendFactor::One,
                                    operation: wgpu::BlendOperation::Add,
                                },
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                },
            ));
        }
        if visible_format.is_some() && self.visible_format != visible_format {
            let device = accelerator.wgpu_device();
            let shader = engine_physics::simulation::create_physics_shader_module(
                device,
                "Actor sprite shader",
                include_str!("scene_sprites.wgsl"),
                "engine/src/renders/scene_sprites.wgsl",
            );
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Actor sprite visible pipeline layout"),
                bind_group_layouts: &[Some(self.layout.as_ref().unwrap())],
                immediate_size: 0,
            });
            self.visible_pipeline = Some(device.create_render_pipeline(
                &wgpu::RenderPipelineDescriptor {
                    label: Some("Actor sprite visible pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vertex"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("visible_fragment"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: visible_format.unwrap(),
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                },
            ));
            self.visible_format = visible_format;
        }
    }
}

struct SpriteBindGroup {
    bind_group: wgpu::BindGroup,
    instance_count: u32,
}

#[cfg(test)]
mod tests {
    use super::{SceneSpriteSheetGraphics, SpriteTextureCache};
    use std::sync::Arc;

    #[test]
    fn cloned_sprite_sources_have_one_texture_cache_key() {
        let source: Arc<[u8]> = vec![0, 0, 0, 255].into();
        let first = SceneSpriteSheetGraphics {
            width: 1,
            height: 1,
            rgba_data: Arc::clone(&source),
        };
        let second = SceneSpriteSheetGraphics {
            width: 1,
            height: 1,
            rgba_data: source,
        };
        assert_eq!(
            SpriteTextureCache::key(&first),
            SpriteTextureCache::key(&second)
        );
    }
}
