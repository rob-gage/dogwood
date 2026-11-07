// Copyright Rob Gage 2026

use super::SceneRendererLightingPass;
use crate::world::{
    materials::pack_radiance,
    scenes::{
        LightingQuality, SceneGraphics, SceneGraphicsCellular, SceneGraphicsLight,
        SceneGraphicsLighting, SceneGraphicsTemperature,
    },
};
use engine_host::acceleration::Accelerator;
use std::sync::mpsc;

const SIDE: u32 = 32;
const TILE_SIZE: u32 = 8;
const REGION_SIDE: u32 = SIDE / TILE_SIZE;
const ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

struct Fixture {
    identifiers: wgpu::Buffer,
    colors: wgpu::Buffer,
    matter: wgpu::Buffer,
    temperature_keys: wgpu::Buffer,
    amount_keys: wgpu::Buffer,
    quality: LightingQuality,
    lights: Vec<SceneGraphicsLight>,
}

impl Fixture {
    fn new(accelerator: &Accelerator, radiance: Option<[f32; 4]>) -> Self {
        let count = (SIDE * SIDE) as usize;
        let identifier = u32::from(radiance.is_some());
        let identifiers = storage_buffer(
            accelerator,
            "test material identifiers",
            &vec![identifier; count],
        );
        let packed = radiance.map_or(0, pack_radiance);
        let mut colors_data = Vec::with_capacity(count * 2);
        for _ in 0..count {
            colors_data.extend_from_slice(&[0, packed]);
        }
        let colors = storage_buffer(accelerator, "test cellular colors", &colors_data);
        let mut matter_data = Vec::with_capacity(count * 2);
        for _ in 0..count {
            matter_data.extend_from_slice(&[300.0_f32.to_bits(), 65536]);
        }
        let matter = storage_buffer(accelerator, "test temperatures", &matter_data);
        // A zero key count means the packed cellular radiance passes through unchanged.
        let temperature_keys = storage_buffer(accelerator, "test temperature keys", &[0_u32; 8]);
        let amount_keys = storage_buffer(accelerator, "test amount keys", &[0_u32; 8]);
        Self {
            identifiers,
            colors,
            matter,
            temperature_keys,
            amount_keys,
            quality: LightingQuality::High,
            lights: Vec::new(),
        }
    }

    fn graphics(&self) -> SceneGraphics<'_> {
        SceneGraphics {
            visible_width: REGION_SIDE as f32,
            visible_height: REGION_SIDE as f32,
            cellular_tile_size: TILE_SIZE,
            cell_grid_origin: [0.0, 0.0],
            cellular: Some(SceneGraphicsCellular {
                material_identifier_buffer: &self.identifiers,
                color_buffer: &self.colors,
                region_x: 0.0,
                region_y: 0.0,
                region_tile_x: 0,
                region_tile_y: 0,
                region_width: REGION_SIDE,
                region_height: REGION_SIDE,
                ring_offset_x: 0,
                ring_offset_y: 0,
                cellular_tile_size: TILE_SIZE,
            }),
            fluid: None,
            gas: None,
            pressure: None,
            temperature: Some(SceneGraphicsTemperature {
                matter_buffer: &self.matter,
                ambient_temperature: 300.0,
                region_x: 0.0,
                region_y: 0.0,
                region_width: REGION_SIDE,
                region_height: REGION_SIDE,
                ring_offset_x: 0,
                ring_offset_y: 0,
                cellular_tile_size: TILE_SIZE,
            }),
            parallax: Vec::new(),
            parallax_batches: Vec::new(),
            decorations: None,
            material_surfaces: None,
            material_edges: false,
            shaded: Vec::new(),
            sprites: Default::default(),
            overlays: Vec::new(),
            lighting: SceneGraphicsLighting {
                ambient: [0.0; 3],
                penetration: 1.0,
                quality: self.quality,
                temperature_color_buffer: &self.temperature_keys,
                amount_color_buffer: &self.amount_keys,
                thermal_offsets: [0; 4],
                lights: self.lights.clone(),
            },
        }
    }
}

fn storage_buffer(accelerator: &Accelerator, label: &str, values: &[u32]) -> wgpu::Buffer {
    let bytes: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let buffer = accelerator.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len().max(4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    accelerator.queue().write_buffer(&buffer, 0, &bytes);
    buffer
}

fn read_texture(
    accelerator: &Accelerator,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<[f32; 4]> {
    let device = accelerator.device();
    let row_bytes = width * 8;
    let padded_row = row_bytes.div_ceil(ROW_ALIGNMENT) * ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lighting test texture readback"),
        size: u64::from(padded_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    accelerator.queue().submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (send, receive) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = send.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let mapped = slice.get_mapped_range().unwrap();
    let mut values = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let offset = (y * padded_row + x * 8) as usize;
            let mut rgba = [0.0; 4];
            for (channel, value) in rgba.iter_mut().enumerate() {
                let bits = u16::from_le_bytes([
                    mapped[offset + channel * 2],
                    mapped[offset + channel * 2 + 1],
                ]);
                *value = f16_to_f32(bits);
            }
            values.push(rgba);
        }
    }
    drop(mapped);
    buffer.unmap();
    values
}

fn f16_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = (bits >> 10) & 0x1f;
    let fraction = u32::from(bits & 0x03ff);
    let value = match exponent {
        0 if fraction == 0 => sign,
        0 => {
            let mut significand = fraction;
            let mut shift = 0;
            while significand & 0x400 == 0 {
                significand <<= 1;
                shift += 1;
            }
            sign | ((113 - shift) << 23) | ((significand & 0x3ff) << 13)
        }
        31 => sign | 0x7f80_0000 | (fraction << 13),
        _ => sign | ((u32::from(exponent) + 112) << 23) | (fraction << 13),
    };
    f32::from_bits(value)
}

fn run(accelerator: &Accelerator, pass: &mut SceneRendererLightingPass, fixture: &Fixture) {
    let graphics = fixture.graphics();
    pass.prepare(accelerator, &graphics, &[]);
    let mut encoder = accelerator
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    pass.compute(&mut encoder);
    accelerator.queue().submit([encoder.finish()]);
    accelerator
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}

fn domain_texture(pass: &SceneRendererLightingPass, optical: bool) -> &wgpu::Texture {
    let domain = pass
        .domain
        .as_ref()
        .expect("lighting domain should be prepared");
    if optical {
        domain.optical_view.texture()
    } else {
        domain.light_view.texture()
    }
}

#[test]
fn quality_switches_resize_domain_and_keep_gpu_light_live() {
    let accelerator =
        Accelerator::new().expect("GPU adapter is required for lighting regression tests");
    let mut fixture = Fixture::new(&accelerator, None);
    fixture.lights.push(SceneGraphicsLight {
        center: [2.0, 2.0],
        radius: 0.8,
        emission: [2.0, 0.5, 0.25],
    });
    let mut pass = SceneRendererLightingPass::new(&accelerator);
    for (quality, expected_side) in [
        (LightingQuality::High, 32),
        (LightingQuality::Medium, 16),
        (LightingQuality::Low, 8),
        (LightingQuality::High, 32),
    ] {
        fixture.quality = quality;
        run(&accelerator, &mut pass, &fixture);
        let domain = pass.domain.as_ref().unwrap();
        assert_eq!([domain.width, domain.height], [expected_side; 2]);
        let image = read_texture(
            &accelerator,
            domain.light_view.texture(),
            expected_side,
            expected_side,
        );
        let peak = image.iter().map(|pixel| pixel[0]).fold(0.0_f32, f32::max);
        assert!(
            peak > 0.05,
            "{quality:?} GPU light field was dark: peak={peak}"
        );
    }
}

#[test]
fn optical_coefficients_and_dense_emission_preserve_world_units() {
    let accelerator =
        Accelerator::new().expect("GPU adapter is required for lighting regression tests");
    let mut pass = SceneRendererLightingPass::new(&accelerator);
    // Exercise both volumetric integration and the optically thick surface-emission cutoff.
    for occlusion in [0.5, 1.0] {
        let mut fixture = Fixture::new(&accelerator, Some([1.5, 0.75, 0.25, occlusion]));
        let mut reference_optical = [0.0_f32; 4];
        let mut reference_light = 0.0_f32;
        for quality in [
            LightingQuality::High,
            LightingQuality::Medium,
            LightingQuality::Low,
        ] {
            fixture.quality = quality;
            run(&accelerator, &mut pass, &fixture);
            let scale = quality.texel_size();
            let side = SIDE / scale;
            let image = read_texture(&accelerator, domain_texture(&pass, true), side, side);
            let first = image[0];
            if quality == LightingQuality::High {
                reference_optical = first;
                assert!(
                    first[0] > 1.0 && first[3] > 0.01,
                    "missing optics: {first:?}"
                );
            }
            for pixel in &image {
                for channel in 0..4 {
                    let expected = reference_optical[channel] * scale as f32;
                    assert!(
                        (pixel[channel] - expected).abs() < 0.02,
                        "{quality:?} optical coefficient changed in world units: {pixel:?} vs {expected}"
                    );
                }
            }
            let light = read_texture(&accelerator, domain_texture(&pass, false), side, side);
            let center = light[((side / 2) * side + side / 2) as usize][0];
            if quality == LightingQuality::High {
                reference_light = center;
            }
            assert!(reference_light > 0.1);
            assert!(
                (center / reference_light - 1.0).abs() < 0.08,
                "{quality:?} changed medium brightness with occlusion {occlusion}: {center} vs High {reference_light}"
            );
        }
    }
}

#[test]
fn actor_disc_has_spatial_coverage_and_fades_with_distance() {
    let accelerator =
        Accelerator::new().expect("GPU adapter is required for lighting regression tests");
    let mut fixture = Fixture::new(&accelerator, None);
    fixture.lights.push(SceneGraphicsLight {
        center: [2.0, 2.0],
        radius: 0.48,
        emission: [4.0, 1.0, 0.5],
    });
    let mut pass = SceneRendererLightingPass::new(&accelerator);
    run(&accelerator, &mut pass, &fixture);
    let image = read_texture(&accelerator, domain_texture(&pass, false), SIDE, SIDE);
    let at = |x: usize, y: usize| image[y * SIDE as usize + x][0];
    let center = at(16, 16);
    let nearby = at(20, 16);
    let far = at(1, 1);
    assert!(
        center > nearby && nearby > far,
        "disc lighting did not cover space with a falloff: center={center}, nearby={nearby}, far={far}"
    );
}

fn radiance_texture(accelerator: &Accelerator, levels: &[[u8; 4]]) -> wgpu::TextureView {
    let texture = accelerator
        .device()
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("lighting test radiance"),
            size: wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
    for (level, color) in levels.iter().enumerate() {
        let side = 32 >> level;
        let bytes: Vec<u8> = (0..side * side).flat_map(|_| *color).collect();
        accelerator.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * 4),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

#[test]
fn sprite_and_disc_keep_world_placement_emission_and_absorption() {
    let accelerator =
        Accelerator::new().expect("GPU adapter is required for lighting regression tests");
    let mut fixture = Fixture::new(&accelerator, None);
    fixture.lights.push(SceneGraphicsLight {
        center: [1.0, 3.0],
        radius: 0.8,
        emission: [0.0, 0.0, 2.0],
    });
    let radiance = radiance_texture(&accelerator, &[[128, 64, 0, 128]]);
    let mut pass = SceneRendererLightingPass::new(&accelerator);
    for quality in [
        LightingQuality::High,
        LightingQuality::Medium,
        LightingQuality::Low,
    ] {
        fixture.quality = quality;
        let mut graphics = fixture.graphics();
        graphics.sprites.sprites.push((
            &radiance,
            &radiance,
            3.0,
            1.0,
            1.0,
            1.0,
            0.0,
            0,
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0; 4],
            1.0,
        ));
        pass.prepare(&accelerator, &graphics, &[]);
        let mut encoder = accelerator
            .device()
            .create_command_encoder(&Default::default());
        pass.compute(&mut encoder);
        accelerator.queue().submit([encoder.finish()]);
        let scale = quality.texel_size();
        let side = SIDE / scale;
        let image = read_texture(&accelerator, domain_texture(&pass, true), side, side);
        let at = |x: u32, y: u32| image[((y / scale) * side + x / scale) as usize];
        let sprite = at(24, 8);
        let disc = at(8, 24);
        assert!(
            (sprite[0] / scale as f32 - 128.0 / 255.0).abs() < 0.005,
            "{quality:?} sprite moved or changed emission: {sprite:?}"
        );
        let extinction_per_cell = -(1.0_f32 - 128.0 / 255.0).ln() / TILE_SIZE as f32;
        assert!(
            (sprite[3] / scale as f32 - extinction_per_cell).abs() < 0.002,
            "{quality:?} sprite absorption changed: {sprite:?}"
        );
        assert!(
            (disc[2] / scale as f32 - 2.0).abs() < 0.005,
            "{quality:?} disc moved or changed emission: {disc:?}"
        );
        assert_eq!(at(0, 0), [0.0; 4]);
    }
}

#[test]
fn backlight_world_coverage_and_physical_blur_do_not_change_with_quality() {
    use crate::world::scenes::{ParallaxRepeat, SceneGraphicsParallax};
    let accelerator =
        Accelerator::new().expect("GPU adapter is required for lighting regression tests");
    let mut fixture = Fixture::new(&accelerator, None);
    // Distinct mip colors expose a quality-dependent blur level without image comparisons.
    let radiance = radiance_texture(&accelerator, &[[255, 0, 0, 0], [0, 255, 0, 0]]);
    let background = SceneGraphicsParallax {
        texture: &radiance,
        radiance_texture: &radiance,
        center: [3.0, 3.0],
        size: [2.0, 2.0],
        depth: 0.125,
        repeat: [ParallaxRepeat::None; 2],
        order: 0,
        rotation: 0.0,
        texture_region: [0.0, 0.0, 1.0, 1.0],
        color: [1.0; 4],
        radiance_texture_region: [0.0, 0.0, 1.0, 1.0],
        radiance_intensity: 1.0,
    };
    let mut pass = SceneRendererLightingPass::new(&accelerator);
    for quality in [
        LightingQuality::High,
        LightingQuality::Medium,
        LightingQuality::Low,
    ] {
        fixture.quality = quality;
        pass.prepare(
            &accelerator,
            &fixture.graphics(),
            std::slice::from_ref(&background),
        );
        let mut encoder = accelerator
            .device()
            .create_command_encoder(&Default::default());
        pass.compute(&mut encoder);
        accelerator.queue().submit([encoder.finish()]);
        let domain = pass.domain.as_ref().unwrap();
        let side = domain.width.div_ceil(super::LIGHTING_BACKLIGHT_CELLS);
        let image = read_texture(&accelerator, domain.backlight_view.texture(), side, side);
        let upper_right = image[(side * side - 1) as usize];
        assert!(
            upper_right[1] > 0.99 && upper_right[0] < 0.01,
            "{quality:?} changed background placement or physical blur: {upper_right:?}"
        );
        assert_eq!(image[0][0..3], [0.0; 3]);
        let light = read_texture(
            &accelerator,
            domain.light_view.texture(),
            domain.width,
            domain.height,
        );
        assert!(
            light.last().unwrap()[1] > 0.5,
            "{quality:?} lost backlight during spread"
        );
    }
}
