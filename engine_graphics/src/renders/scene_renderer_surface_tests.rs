use super::SceneRendererSurfacePass;
use crate::world::scenes::SceneGraphicsCellular;
use engine_host::acceleration::Accelerator;
use wgpu::util::DeviceExt;

fn storage_buffer(accelerator: &Accelerator, label: &str, words: &[u32]) -> wgpu::Buffer {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    accelerator.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: &bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}

fn read_field(accelerator: &Accelerator, pass: &SceneRendererSurfacePass) -> Vec<u8> {
    let (width, height) = pass.field_size;
    let texture = pass.field_view().unwrap().texture();
    let padded_row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = accelerator.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("surface test readback"),
        size: u64::from(padded_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = accelerator.device().create_command_encoder(&Default::default());
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
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    accelerator.queue().submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    accelerator.device().poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let mut out = Vec::new();
    for row in 0..height as usize {
        let start = row * padded_row as usize;
        out.extend_from_slice(&mapped[start..start + width as usize * 4]);
    }
    out
}

#[test]
#[ignore = "requires GPU; compares the workgroup-memory surface field against the reference"]
fn shared_surface_field_matches_reference() {
    let accelerator = Accelerator::default();
    for (tile_size, region_width, region_height, ring, visible) in [
        (8u32, 17u32, 13u32, (3u32, 5u32), (15.5f32, 11.25f32)),
        (8, 20, 12, (0, 0), (20.0, 12.0)),
        (8, 9, 9, (8, 1), (6.0, 6.0)),
    ] {
        let cells = (region_width * region_height * tile_size * tile_size) as usize;
        let mut seed: u32 = 0x1234_5678 ^ region_width;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let mut materials = vec![0u32; cells];
        for (index, material) in materials.iter_mut().enumerate() {
            // blobs of solid with scattered holes and specks, so every distance appears
            let blob = (index / 7 + index / 811) % 5 < 3;
            let speck = next() % 23 == 0;
            *material = u32::from(blob != speck);
        }
        let identifiers = storage_buffer(&accelerator, "surface test materials", &materials);
        let colors = storage_buffer(&accelerator, "surface test colors", &vec![0u32; cells * 2]);
        let mut passes = [
            SceneRendererSurfacePass::new(&accelerator),
            SceneRendererSurfacePass::new(&accelerator),
        ];
        passes[0].use_shared = false;
        passes[1].use_shared = true;
        for frame in 0..6u32 {
            // the region drifts one tile every other frame, within the jump limit
            let tile_x = i64::from(frame / 2);
            let cellular = SceneGraphicsCellular {
                material_identifier_buffer: &identifiers,
                color_buffer: &colors,
                region_x: -(tile_x as f32),
                region_y: 0.0,
                region_tile_x: tile_x,
                region_tile_y: 0,
                region_width,
                region_height,
                ring_offset_x: ring.0,
                ring_offset_y: ring.1,
                cellular_tile_size: tile_size,
            };
            let mut fields = Vec::new();
            for pass in &mut passes {
                pass.prepare(&accelerator, &cellular, visible.0, visible.1);
                let mut encoder = accelerator.device().create_command_encoder(&Default::default());
                pass.compute(&mut encoder);
                accelerator.queue().submit([encoder.finish()]);
                fields.push(read_field(&accelerator, pass));
            }
            let mismatch = fields[0].iter().zip(&fields[1]).position(|(a, b)| a != b);
            assert!(
                mismatch.is_none(),
                "surface field differs: region {region_width}x{region_height} frame {frame} byte {mismatch:?}"
            );
            assert!(fields[0].iter().any(|byte| *byte != 0), "field stayed empty at frame {frame}");
        }
    }
}
