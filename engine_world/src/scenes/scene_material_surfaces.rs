// Copyright Rob Gage 2026

use crate::materials::{MaterialIdentifier, MaterialTable};
use engine_graphics::world::scenes::SceneGraphicsMaterialSurfaces;
use engine_host::acceleration::Accelerator;

/// Side length of each tile of surface art
pub const MATERIAL_SURFACE_SIZE: u32 = 256;

/// Art a material shows where it meets open space, updated live as cells appear and disappear
///
/// Each tile holds straight-alpha sRGB RGBA8 pixels from the top row, repeats across the scene, and
/// appears where a cell's closeness to open space, which falls from one to zero over a few cells,
/// exceeds one minus the texel's alpha.
pub struct MaterialSurface<'a> {
    /// The material
    pub material: MaterialIdentifier,
    /// Art of surfaces facing up
    pub top: &'a [u8],
    /// Art of surfaces facing down
    pub under: &'a [u8],
}

/// GPU copy of a scene's material surface art
pub struct SceneMaterialSurfaces {
    /// Two layers per valid surface
    art: wgpu::TextureView,
    /// One plus the first layer of each thermal index
    layers: wgpu::Buffer,
}

impl SceneMaterialSurfaces {

    /// Uploads the art of materials, skipping any with tiles of the wrong size
    pub fn new(
        accelerator: &Accelerator,
        surfaces: &[MaterialSurface<'_>],
        material_table: &MaterialTable,
    ) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let tile_bytes: usize = (MATERIAL_SURFACE_SIZE * MATERIAL_SURFACE_SIZE * 4) as usize;
        let valid: Vec<&MaterialSurface<'_>> = surfaces.iter()
            .filter(|surface| surface.top.len() == tile_bytes && surface.under.len() == tile_bytes)
            .collect();
        let texture: wgpu::Texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("`SceneMaterialSurfaces.art`"),
            size: wgpu::Extent3d {
                width: MATERIAL_SURFACE_SIZE,
                height: MATERIAL_SURFACE_SIZE,
                depth_or_array_layers: (valid.len() as u32 * 2).max(1),
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let offsets: [u32; 4] = material_table.thermal_offsets();
        let thermal_index = |material: MaterialIdentifier| -> usize {
            (material.index() + offsets[material.form() as usize]) as usize
        };
        let count: usize = valid.iter()
            .map(|surface| thermal_index(surface.material) + 1)
            .max()
            .unwrap_or(1);
        let mut layer_words: Vec<u32> = vec![0; count];
        for (index, surface) in valid.iter().enumerate() {
            layer_words[thermal_index(surface.material)] = index as u32 * 2 + 1;
            for (layer, pixels) in [surface.top, surface.under].into_iter().enumerate() {
                accelerator.queue().write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: index as u32 * 2 + layer as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(MATERIAL_SURFACE_SIZE * 4),
                        rows_per_image: Some(MATERIAL_SURFACE_SIZE),
                    },
                    wgpu::Extent3d {
                        width: MATERIAL_SURFACE_SIZE,
                        height: MATERIAL_SURFACE_SIZE,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        let layers: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`SceneMaterialSurfaces.layers`"),
            size: (layer_words.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layer_bytes: Vec<u8> = layer_words.iter().flat_map(|word| word.to_le_bytes()).collect();
        accelerator.queue().write_buffer(&layers, 0, &layer_bytes);
        Self {
            art: texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            }),
            layers,
        }
    }

    /// Returns the art for drawing
    pub fn graphics(&self) -> SceneGraphicsMaterialSurfaces<'_> {
        SceneGraphicsMaterialSurfaces { art: &self.art, layers: &self.layers }
    }

}
