// Copyright Rob Gage 2026

use super::SceneRendererLightingPass;
use super::super::scene_renderer_shaded_pass::SceneRendererShadedPass;
use crate::world::{
    materials::pack_radiance,
    scenes::{
        LightingQuality, SceneGraphics, SceneGraphicsCellular, SceneGraphicsLight,
        SceneGraphicsLighting, SceneGraphicsShaded, SceneGraphicsTemperature,
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
    pass.compute(&mut encoder, |_, _| {});
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
    pass.throttle_frames = Some(0);
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
    pass.throttle_frames = Some(0);
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
    pass.throttle_frames = Some(0);
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
    pass.throttle_frames = Some(0);
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
        pass.compute(&mut encoder, |_, _| {});
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
            // the soft rim keeps the disc's total, taking a little from the middle of a small disc
            (disc[2] / scale as f32 - 2.0).abs() < 0.08,
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
    pass.throttle_frames = Some(0);
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
        pass.compute(&mut encoder, |_, _| {});
        accelerator.queue().submit([encoder.finish()]);
        let domain = pass.domain.as_ref().unwrap();
        // the same world resolution at every quality
        let side = SIDE.div_ceil(super::LIGHTING_BACKLIGHT_CELLS);
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

/// Renders the optics of bodies drawn with `SHADED_OPTICS_TEST_SHADER`, and returns the optical field
fn shaded_optics(
    accelerator: &Accelerator,
    pass: &mut SceneRendererLightingPass,
    shaded_pass: &mut SceneRendererShadedPass,
    fixture: &Fixture,
    outline: Vec<[f32; 2]>,
) -> Vec<[f32; 4]> {
    let body = SceneGraphicsShaded {
        fragment: SHADED_OPTICS_TEST_SHADER,
        parameters: [[0.0; 4]; 4],
        is_cell_snapped: false,
        outline,
    };
    let graphics = fixture.graphics();
    pass.prepare(accelerator, &graphics, &[]);
    shaded_pass.prepare(
        accelerator,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [SIDE as f32, SIDE as f32],
        TILE_SIZE as f32,
        TILE_SIZE as f32,
        [0.0, 0.0],
        &[body],
    );
    let mut encoder = accelerator
        .device()
        .create_command_encoder(&Default::default());
    pass.compute(&mut encoder, |render_pass, lighting| {
        shaded_pass.draw_optics(render_pass, lighting);
    });
    accelerator.queue().submit([encoder.finish()]);
    let side = SIDE / fixture.quality.texel_size();
    read_texture(accelerator, domain_texture(pass, true), side, side)
}

const SHADED_OPTICS_TEST_SHADER: &str = "
fn shade(input: ShadeInput) -> ShadeOutput {
    var output: ShadeOutput;
    output.albedo = vec4<f32>(1.0);
    output.emission = vec3<f32>(9.0);
    output.radiance = vec3<f32>(0.5, 0.25, 0.0);
    output.occlusion = 0.5;
    return output;
}
";

#[test]
fn shaded_bodies_emit_and_occlude_like_cells_at_every_quality() {
    let accelerator =
        Accelerator::new().expect("GPU adapter is required for lighting regression tests");
    let fixture_empty = Fixture::new(&accelerator, None);
    let mut pass = SceneRendererLightingPass::new(&accelerator);
    pass.throttle_frames = Some(0);
    let mut shaded_pass = SceneRendererShadedPass::new(&accelerator, pass.bind_group_layout());
    let square = |low: f32, high: f32| vec![[low, low], [high, low], [high, high], [low, high]];
    let mut fixture = fixture_empty;
    let extinction_per_cell = -(0.5_f32).ln() / TILE_SIZE as f32;
    let mut small_body_totals = Vec::new();
    for quality in [
        LightingQuality::High,
        LightingQuality::Medium,
        LightingQuality::Low,
    ] {
        fixture.quality = quality;
        let scale = quality.texel_size();
        let side = SIDE / scale;
        // a body covering whole texels at every quality
        let image = shaded_optics(&accelerator, &mut pass, &mut shaded_pass, &fixture, square(1.0, 3.0));
        let inside = image[((16 / scale) * side + 16 / scale) as usize];
        assert!(
            (inside[0] / scale as f32 - 0.5).abs() < 0.01,
            "{quality:?} changed a shaded body's emission: {inside:?}"
        );
        assert!(
            (inside[3] / scale as f32 - extinction_per_cell).abs() < 0.002,
            "{quality:?} changed a shaded body's occlusion: {inside:?}"
        );
        assert_eq!(image[0], [0.0; 4], "{quality:?} moved a shaded body");
        // a body smaller than the coarsest texel, off the texel grid, neither vanishes nor brightens
        let image = shaded_optics(
            &accelerator,
            &mut pass,
            &mut shaded_pass,
            &fixture,
            square(1.8125, 2.0625),
        );
        let total: f32 = image.iter().map(|pixel| pixel[0] * scale as f32).sum();
        small_body_totals.push(total);
    }
    for total in &small_body_totals {
        assert!(
            (total / small_body_totals[0] - 1.0).abs() < 0.01 && *total > 0.0,
            "a small shaded body's total emission depends on quality: {small_body_totals:?}"
        );
    }
}

#[test]
#[ignore = "requires GPU; exact smooth gas rendering with empty-support rejection"]
fn gas_empty_support_matches_full_scan() {
    use super::super::scene_renderer_gas_pass::SceneRendererGasPass;
    use crate::world::scenes::SceneGraphicsGas;
    let a=Accelerator::default();let mut f=Fixture::new(&a,None);f.quality=LightingQuality::Low;
    let gas=storage_buffer(&a,"gas support test cells",&vec![0u32;(SIDE*SIDE*8) as usize]);
    let properties=storage_buffer(&a,"gas support test material",&[1f32.to_bits(),0,0,0xff80a060,0.4f32.to_bits(),0,0,0]);
    let appearance=storage_buffer(&a,"gas support test texture",&[0.4f32.to_bits(),2f32.to_bits(),0.3f32.to_bits(),0.2f32.to_bits()]);
    let mut lighting=SceneRendererLightingPass::new(&a);
    lighting.throttle_frames=Some(0);
    let mut graphics=f.graphics();graphics.lighting.ambient=[1.;3];lighting.prepare(&a,&graphics,&[]);
    let mut reference=SceneRendererGasPass::new_config(&a,lighting.bind_group_layout(),false,false);
    let mut optimized=SceneRendererGasPass::new_config(&a,lighting.bind_group_layout(),true,true);
    for (width,height) in [(128u32,96u32),(127,73)] {
        let target=a.device().create_texture(&wgpu::TextureDescriptor{label:None,size:wgpu::Extent3d{width,height,depth_or_array_layers:1},mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,format:wgpu::TextureFormat::Rgba16Float,usage:wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::COPY_SRC,view_formats:&[]});
        let view=target.create_view(&Default::default());
        for pattern in [1,0,2,3,4,5,0,6,0,2,5,1] {for fraction in [0.0f32,0.03125,0.09375,0.125,0.5,0.625] {
            let mut cells=vec![0u32;(SIDE*SIDE*8) as usize];
            for y in 0..SIDE {for x in 0..SIDE {
                let present=match pattern {1=>true,2=>x==16&&y==16,3=>(x+y)%3==0,4=>x==0||x==SIDE-1||y==0||y==SIDE-1,5=>x<3&&y<3,6=>x>=SIDE-2&&y>=SIDE-2,_=>false};
                let tile=((y/8+3)%REGION_SIDE)*REGION_SIDE+(x/8+2)%REGION_SIDE;
                let index=tile*64+y%8*8+x%8;
                if present {cells[index as usize*8]=4096;}
            }}
            a.queue().write_buffer(&gas,0,&cells.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>());
            let g=SceneGraphicsGas{gas_buffer:&gas,material_identifier_buffer:&f.identifiers,material_properties_buffer:&properties,material_graphics_buffer:&appearance,region_x:-2.+fraction,region_y:-2.-fraction,region_width:REGION_SIDE,region_height:REGION_SIDE,ring_offset_x:2,ring_offset_y:3,cellular_tile_size:8,amount_per_atmosphere:4096.,time:0.375};
            let mut images=Vec::new();
            for pass in [&mut reference,&mut optimized] {
                pass.prepare(&a,wgpu::TextureFormat::Rgba16Float,0,0,width,height,width as f32/4.0+0.125,&g,&f.matter);
                let mut encoder=a.device().create_command_encoder(&Default::default());pass.shade(&mut encoder,lighting.bind_group().unwrap());
                {let mut p=encoder.begin_render_pass(&wgpu::RenderPassDescriptor{label:None,color_attachments:&[Some(wgpu::RenderPassColorAttachment{view:&view,depth_slice:None,resolve_target:None,ops:wgpu::Operations{load:wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),store:wgpu::StoreOp::Store}})],..Default::default()});pass.draw(&mut p,wgpu::TextureFormat::Rgba16Float,lighting.bind_group().unwrap());}
                a.queue().submit([encoder.finish()]);images.push(read_texture(&a,&target,width,height));
            }
            // the reference must show gas exactly where the pattern puts it, or nothing is compared
            let is_lit=images[0].iter().any(|pixel|pixel[3]>0.0);
            if pattern==1 {assert!(is_lit,"full gas is not visible: fraction={fraction}");}
            if pattern==0 {assert!(!is_lit,"empty gas is visible: fraction={fraction}");}
            let mismatch=images[0].iter().zip(&images[1]).position(|(a,b)|a!=b);
            assert!(mismatch.is_none(),"gas mask changed pixels: {width}x{height}, pattern={pattern}, fraction={fraction}, pixel={mismatch:?}");
        }}
    }
}

#[test]
#[ignore = "requires GPU; compares localized optical invalidation against global bounds"]
fn local_dirty_matches_global_bounds() {
    let a=Accelerator::default();
    for (width,height,quality) in [(128u32,96u32,LightingQuality::High),(136,104,LightingQuality::Medium),(136,104,LightingQuality::Low),(1280,720,LightingQuality::Low)] {
        let count=(width*height) as usize;
        let mut fixture=Fixture::new(&a,None);
        fixture.quality=quality;
        fixture.identifiers=storage_buffer(&a,"local dirty identifiers",&vec![1;count]);
        fixture.colors=storage_buffer(&a,"local dirty colors",&vec![0;count*2]);
        let mut matter=vec![0u32;count*2];
        for cell in matter.chunks_exact_mut(2) {cell[0]=300f32.to_bits();cell[1]=65536;}
        fixture.matter=storage_buffer(&a,"local dirty matter",&matter);
        let mut passes=[SceneRendererLightingPass::new_local_dirty(&a,false),SceneRendererLightingPass::new_local_dirty(&a,true)];
        for pass in &mut passes {pass.throttle_frames=Some(0);}
        for frame in 0u32..12 {
            let ring=if frame>=7 {(1u32,1u32)} else {(0,0)};
            let rw=width/8;let rh=height/8;
            let mut colors=vec![0u32;count*2];
            let sites=[(4+frame%3,4),(width-5,height-5-frame%3),(width/2,height/2)];
            for y in 0..height {for x in 0..width {
                let tile=((y/8+ring.1)%rh)*rw+(x/8+ring.0)%rw;
                let cell=(tile*64+(y%8)*8+x%8) as usize;
                let mut emission=[0.;4];
                if frame!=0 && frame!=5 && frame!=6 && sites.iter().any(|p|*p==(x,y)) {
                    emission=if frame&1==0 {[1.,0.25,0.125,0.]} else {[0.125,0.5,1.,0.]};
                }
                if frame>=3 && x==width/2 {emission[3]=0.95;}
                if frame==11 {emission=[0.1,0.2,0.05,0.35];}
                colors[cell*2+1]=pack_radiance(emission);
            }}
            a.queue().write_buffer(&fixture.colors,0,&colors.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>());
            let mut graphics=fixture.graphics();
            graphics.visible_width=rw as f32;graphics.visible_height=rh as f32;
            let c=graphics.cellular.as_mut().unwrap();c.region_width=rw;c.region_height=rh;c.ring_offset_x=ring.0;c.ring_offset_y=ring.1;
            if frame>=9 {c.region_tile_x=-2;c.region_tile_y=1;c.region_x=-2.;c.region_y=1.;}
            let t=graphics.temperature.as_mut().unwrap();t.region_width=rw;t.region_height=rh;t.ring_offset_x=ring.0;t.ring_offset_y=ring.1;
            let mut images=Vec::new();
            for pass in &mut passes {
                pass.prepare(&a,&graphics,&[]);
                let mut e=a.device().create_command_encoder(&Default::default());
                pass.compute(&mut e,|_,_|{});a.queue().submit([e.finish()]);
                let domain=pass.domain.as_ref().unwrap();
                images.push(read_texture(&a,domain.light_view.texture(),domain.width,domain.height));
            }
            assert_eq!(images[0],images[1],"local invalidation changed lighting: {width}x{height}, frame {frame}");
            if frame==1 {assert!(images[0].iter().any(|v|v[0]>0.001),"fixture produced no light");}
        }
    }
}

#[test]
#[ignore = "requires GPU; compares throttled lighting against computing every frame"]
fn throttled_light_matches_unthrottled_when_computed() {
    let a=Accelerator::default();
    for (width,height,quality) in [(136u32,104u32,LightingQuality::Medium),(136,104,LightingQuality::Low),(1280,720,LightingQuality::Low),(128,96,LightingQuality::High)] {
        let count=(width*height) as usize;
        let mut fixture=Fixture::new(&a,None);
        fixture.quality=quality;
        fixture.identifiers=storage_buffer(&a,"throttle identifiers",&vec![1;count]);
        fixture.colors=storage_buffer(&a,"throttle colors",&vec![0;count*2]);
        let mut matter=vec![0u32;count*2];
        for cell in matter.chunks_exact_mut(2) {cell[0]=300f32.to_bits();cell[1]=65536;}
        fixture.matter=storage_buffer(&a,"throttle matter",&matter);
        let mut passes=[SceneRendererLightingPass::new(&a),SceneRendererLightingPass::new(&a)];
        passes[0].throttle_frames=Some(0);
        passes[1].throttle_frames=None;
        let mut previous: Option<Vec<[f32;4]>>=None;
        let mut throttled=0;
        for frame in 0u32..14 {
            let rw=width/8;let rh=height/8;
            let mut colors=vec![0u32;count*2];
            let sites=[(4+frame%3,4),(width-5,height-5-frame%3),(width/2,height/2)];
            for y in 0..height {for x in 0..width {
                let tile=(y/8)*rw+x/8;
                let cell=(tile*64+(y%8)*8+x%8) as usize;
                let mut emission=[0.;4];
                if frame!=0 && sites.iter().any(|p|*p==(x,y)) {
                    emission=if frame&1==0 {[1.,0.25,0.125,0.]} else {[0.125,0.5,1.,0.]};
                }
                if frame>=3 && x==width/2 {emission[3]=0.95;}
                colors[cell*2+1]=pack_radiance(emission);
            }}
            a.queue().write_buffer(&fixture.colors,0,&colors.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>());
            let mut graphics=fixture.graphics();
            graphics.visible_width=width as f32;graphics.visible_height=height as f32;
            let c=graphics.cellular.as_mut().unwrap();c.region_width=rw;c.region_height=rh;
            let t=graphics.temperature.as_mut().unwrap();t.region_width=rw;t.region_height=rh;
            // the region moves on frame 9, which no frame may keep stale light across
            if frame>=9 {
                let c=graphics.cellular.as_mut().unwrap();c.region_tile_x=-2;c.region_tile_y=1;c.region_x=-2.;c.region_y=1.;
            }
            let mut images=Vec::new();
            for pass in &mut passes {
                pass.prepare(&a,&graphics,&[]);
                let mut e=a.device().create_command_encoder(&Default::default());
                pass.compute(&mut e,|_,_|{});a.queue().submit([e.finish()]);
                let domain=pass.domain.as_ref().unwrap();
                images.push(read_texture(&a,domain.light_view.texture(),domain.width,domain.height));
            }
            if passes[1].is_throttled {
                throttled+=1;
                assert!(quality!=LightingQuality::High,"High kept stale light at frame {frame}");
                assert!(frame!=0 && frame!=9,"{quality:?} kept stale light across a fresh domain or moved region at frame {frame}");
                assert_eq!(Some(&images[1]),previous.as_ref(),"a throttled frame changed the light: {quality:?} {width}x{height} frame {frame}");
            } else {
                assert_eq!(images[0],images[1],"throttled lighting differs when computed: {quality:?} {width}x{height} frame {frame}");
            }
            previous=Some(images.remove(1));
        }
        if quality==LightingQuality::High {assert_eq!(throttled,0,"High throttled");} else {assert!(throttled>=4,"{quality:?} never throttled: {throttled}");}
    }
}

/// Renders one emitter scene at several qualities and ray counts, writing tone-mapped images to
/// `DOGWOOD_LIGHTING_IMAGE_DIR` and printing how far each differs from High
#[test]
#[ignore = "requires GPU; writes comparison images and prints metrics"]
fn lighting_direction_reduction_report() {
    let a=Accelerator::default();
    let (width,height)=(1280u32,720u32);
    let count=(width*height) as usize;
    let (rw,rh)=(width/8,height/8);
    let mut colors=vec![0u32;count*2];
    let mut set=|x:u32,y:u32,emission:[f32;4]| {
        let tile=(y/8)*rw+x/8;
        let cell=(tile*64+(y%8)*8+x%8) as usize;
        colors[cell*2+1]=pack_radiance(emission);
    };
    // point emitters of different colors and strengths in open air
    for (x,y,e) in [(300u32,360u32,[8.,4.,2.,0.]),(900,200,[2.,6.,8.,0.]),(640,600,[6.,6.,1.,0.]),(1100,500,[1.,1.,10.,0.])] { set(x,y,e); }
    // a one-cell-wide dim emitter and a three-cell lamp
    set(150,150,[1.,0.2,0.2,0.]);
    for dx in 0..3 { set(500+dx,120,[5.,5.,5.,0.]); }
    // walls and a block that shade the emitters
    for y in 250..470 { set(600,y,[0.,0.,0.,0.95]); set(601,y,[0.,0.,0.,0.95]); }
    for y in 330..400 { for x in 960..1000 { set(x,y,[0.,0.,0.,0.9]); } }
    let mut results: Vec<(String,u32,Vec<[f32;4]>)>=Vec::new();
    for (name,quality,percent) in [
        ("high",LightingQuality::High,100u32),
        ("medium100",LightingQuality::Medium,100),("medium50",LightingQuality::Medium,50),
        ("low100",LightingQuality::Low,100),("low75",LightingQuality::Low,75),("low50",LightingQuality::Low,50),("low25",LightingQuality::Low,25),
    ] {
        let mut fixture=Fixture::new(&a,None);
        fixture.quality=quality;
        fixture.identifiers=storage_buffer(&a,"report identifiers",&vec![1;count]);
        fixture.colors=storage_buffer(&a,"report colors",&colors);
        let mut matter=vec![0u32;count*2];
        for cell in matter.chunks_exact_mut(2) {cell[0]=300f32.to_bits();cell[1]=65536;}
        fixture.matter=storage_buffer(&a,"report matter",&matter);
        let mut pass=SceneRendererLightingPass::new(&a);
        pass.throttle_frames=Some(0);pass.direction_percent=percent;
        let mut graphics=fixture.graphics();
        graphics.visible_width=width as f32;graphics.visible_height=height as f32;
        let c=graphics.cellular.as_mut().unwrap();c.region_width=rw;c.region_height=rh;
        let t=graphics.temperature.as_mut().unwrap();t.region_width=rw;t.region_height=rh;
        pass.prepare(&a,&graphics,&[]);
        let mut e=a.device().create_command_encoder(&Default::default());
        pass.compute(&mut e,|_,_|{});a.queue().submit([e.finish()]);
        let domain=pass.domain.as_ref().unwrap();
        let image=read_texture(&a,domain.light_view.texture(),domain.width,domain.height);
        results.push((name.to_string(),quality.texel_size(),image));
    }
    let reference=results[0].2.clone();
    let directory=std::env::var("DOGWOOD_LIGHTING_IMAGE_DIR").ok();
    for (name,texel,image) in &results {
        let (w,h)=(width/texel,height/texel);
        // High box-averaged to this resolution
        let mut expected=vec![[0f32;3];(w*h) as usize];
        for y in 0..height {for x in 0..width {
            let p=reference[(y*width+x) as usize];
            let e=&mut expected[((y/texel)*w+x/texel) as usize];
            for k in 0..3 {e[k]+=p[k]/(texel*texel) as f32;}
        }}
        let (mut error,mut total,mut ghost)=(0f64,0f64,0f64);
        let mut peak=0f32;
        for i in 0..(w*h) as usize {
            let (p,r)=(image[i],expected[i]);
            let (pl,rl)=((p[0]+p[1]+p[2])/3.,(r[0]+r[1]+r[2])/3.);
            error+=(pl-rl).abs() as f64;total+=rl as f64;
            ghost+=(pl-rl*1.5-0.01).max(0.) as f64;
            peak=peak.max(pl);
        }
        println!("LIGHT_REPORT {name}: size={w}x{h} mean_abs_error/total={:.4} ghost_excess/total={:.4} peak={peak:.3}",error/total,ghost/total);
        if let Some(directory)=&directory {
            let mut bytes=format!("P6\n{w} {h}\n255\n").into_bytes();
            for y in (0..h).rev() {for x in 0..w {
                let p=image[(y*w+x) as usize];
                for k in 0..3 {bytes.push(((1.0-(-p[k]*2.0).exp()).powf(1.0/2.2)*255.0).clamp(0.,255.) as u8);}
            }}
            std::fs::write(format!("{directory}/{name}.ppm"),bytes).unwrap();
        }
    }
}
