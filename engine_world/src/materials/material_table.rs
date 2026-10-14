// Copyright Rob Gage 2026

use super::{
    Material,
    MaterialError,
    MaterialIdentifier,
    MaterialPropertiesCellularDynamic,
    MaterialPropertiesCellularStatic,
    MaterialPropertiesFluid,
    MaterialPropertiesGas,
    MaterialRegistry,
};
use engine_host::acceleration::Accelerator;
use crate::scenes::cellular_tiles::CELLULAR_TILE_SIZE;

/// Immutable registered materials and compiled GPU resources
pub struct MaterialTable {
    /// Sole owned material definitions with registration inaccessible after compilation
    material_registry: MaterialRegistry,
    /// Static cellular graphics records indexed directly from reserved empty index zero
    cellular_static_properties_buffer: wgpu::Buffer,
    /// Absent until dynamic cellular properties have meaningful GPU records
    cellular_dynamic_properties_buffer: Option<wgpu::Buffer>,
    /// Absent until fluid properties have meaningful GPU records
    fluid_properties_buffer: Option<wgpu::Buffer>,
    /// Absent until gas properties have meaningful GPU records
    gas_properties_buffer: Option<wgpu::Buffer>,
    /// Compiled sRGB albedo frames of static cellular materials
    cellular_static_albedo_atlas_texture_view: wgpu::TextureView,
    /// Static cellular repetition sizes indexed from reserved empty index zero
    cellular_static_repetition_sizes: Vec<[f32; 2]>,
}

impl MaterialTable {

    /// Compiles registered properties and static albedo resources once
    fn new(accelerator: &Accelerator, material_registry: MaterialRegistry) -> Result<Self, String> {
        let device: &wgpu::Device = accelerator.device();
        let maximum_dimension: u32 = device.limits().max_texture_dimension_2d;
        let mut material_records: Vec<[u32; 8]> = Vec::new();
        let mut repetition_sizes: Vec<[f32; 2]> = Vec::new();
        let mut copy_sources: Vec<Option<(&wgpu::TextureView, [f32; 4])>> = Vec::new();
        let mut atlas_regions: Vec<[u32; 4]> = Vec::new();
        let mut atlas_width: u32 = 1;
        let mut atlas_height: u32 = 1;
        let mut row_x: u32 = 0;
        let mut row_y: u32 = 0;
        let mut row_height: u32 = 0;
        for material in material_registry.cellular_static_graphics() {
            let Some(material) = material else {
                material_records.push([0, 0, 0, 0, 1.0f32.to_bits(), 1.0f32.to_bits(), 0, 0]);
                repetition_sizes.push([1.0, 1.0]);
                copy_sources.push(None);
                atlas_regions.push([0; 4]);
                continue;
            };
            if !material.width.is_finite() || !material.height.is_finite()
                || material.width <= 0.0 || material.height <= 0.0
            {
                return Err(String::from(
                    "static cellular material repetition sizes must be finite and positive",
                ));
            }
            let texture_region: [f32; 4] = material.sprite_sheet
                .frame_texture_region(material.frame)
                .ok_or_else(|| String::from(
                    "static cellular material frame is outside its sprite sheet",
                ))?;
            let texture_view: &wgpu::TextureView = material.sprite_sheet.texture_view();
            let texture: &wgpu::Texture = texture_view.texture();
            let frame_width: u32 = ((texture_region[2] - texture_region[0])
                * texture.width() as f32).round() as u32;
            let frame_height: u32 = ((texture_region[3] - texture_region[1])
                * texture.height() as f32).round() as u32;
            if frame_width == 0 || frame_height == 0
                || frame_width > maximum_dimension || frame_height > maximum_dimension
            {
                return Err("static cellular material frame exceeds atlas dimensions".to_owned());
            }
            if row_x.checked_add(frame_width).is_none_or(|right| right > maximum_dimension) {
                row_x = 0;
                row_y = row_y.checked_add(row_height)
                    .ok_or_else(|| "static cellular material atlas capacity exceeded".to_owned())?;
                row_height = 0;
            }
            let bottom: u32 = row_y.checked_add(frame_height)
                .ok_or_else(|| "static cellular material atlas capacity exceeded".to_owned())?;
            if bottom > maximum_dimension {
                return Err("static cellular material atlas capacity exceeded".to_owned());
            }
            let atlas_region: [u32; 4] = [row_x, row_y, frame_width, frame_height];
            // Convert the per-pixel cell scale to the frame's repetition size in tiles.
            let repetition_width: f32 =
                frame_width as f32 * material.width / CELLULAR_TILE_SIZE as f32;
            let repetition_height: f32 =
                frame_height as f32 * material.height / CELLULAR_TILE_SIZE as f32;
            if !repetition_width.is_finite() || !repetition_height.is_finite()
                || repetition_width <= 0.0 || repetition_height <= 0.0
            {
                return Err(String::from(
                    "static cellular material repetition sizes must be finite and positive",
                ));
            }
            material_records.push([
                atlas_region[0],
                atlas_region[1],
                atlas_region[2],
                atlas_region[3],
                repetition_width.to_bits(),
                repetition_height.to_bits(),
                0,
                0,
            ]);
            repetition_sizes.push([repetition_width, repetition_height]);
            copy_sources.push(Some((texture_view, texture_region)));
            atlas_regions.push(atlas_region);
            row_x += frame_width;
            row_height = row_height.max(frame_height);
            atlas_width = atlas_width.max(row_x);
            atlas_height = atlas_height.max(bottom);
        }
        // a one-pixel atlas satisfies the texture binding when no material has a sprite
        let atlas_texture: wgpu::Texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("`MaterialTable.cellular_static_albedo_atlas_texture`"),
            size: wgpu::Extent3d {
                width: atlas_width,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut command_encoder: wgpu::CommandEncoder = device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("`MaterialTable cellular static albedo atlas copy`"),
            },
        );
        for (source, atlas_region) in copy_sources.iter().zip(&atlas_regions) {
            let Some((texture_view, texture_region)) = source else { continue; };
            let texture: &wgpu::Texture = texture_view.texture();
            command_encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: (texture_region[0] * texture.width() as f32).round() as u32,
                        y: (texture_region[1] * texture.height() as f32).round() as u32,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: atlas_region[0], y: atlas_region[1], z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: atlas_region[2],
                    height: atlas_region[3],
                    depth_or_array_layers: 1,
                },
            );
        }
        accelerator.queue().submit([command_encoder.finish()]);
        let property_byte_count: u64 = (material_records.len() as u64)
            .checked_mul(32)
            .ok_or_else(|| "static cellular material property buffer size overflowed".to_owned())?;
        if property_byte_count > device.limits().max_storage_buffer_binding_size as u64
            || property_byte_count > device.limits().max_buffer_size
        {
            return Err("static cellular material property buffer exceeds GPU capacity".to_owned());
        }
        let mut property_bytes: Vec<u8> = Vec::with_capacity(property_byte_count as usize);
        // WGSL vec4<u32>, vec2<f32>, vec2<u32> at offsets 0, 16, 24 with stride 32
        for record in &material_records {
            for value in record {
                property_bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let properties_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`MaterialTable.cellular_static_properties_buffer`"),
            size: property_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        accelerator.queue().write_buffer(&properties_buffer, 0, &property_bytes);
        let atlas_texture_view: wgpu::TextureView =
            atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            material_registry,
            cellular_static_properties_buffer: properties_buffer,
            cellular_dynamic_properties_buffer: None,
            fluid_properties_buffer: None,
            gas_properties_buffer: None,
            cellular_static_albedo_atlas_texture_view: atlas_texture_view,
            cellular_static_repetition_sizes: repetition_sizes,
        })
    }

    /// Returns the material represented by a valid registered identifier
    pub fn get(&self, identifier: MaterialIdentifier) -> Option<&Material> {
        self.material_registry.get(identifier)
    }

    /// Returns the number of registered materials
    pub fn material_count(&self) -> usize {
        self.material_registry.material_count()
    }

    /// Returns the properties of a registered cellular static material
    pub fn cellular_static_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesCellularStatic> {
        self.material_registry.cellular_static_properties(identifier)
    }

    /// Returns the properties of a registered cellular dynamic material
    pub fn cellular_dynamic_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesCellularDynamic> {
        self.material_registry.cellular_dynamic_properties(identifier)
    }

    /// Returns the properties of a registered fluid material
    pub fn fluid_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesFluid> {
        self.material_registry.fluid_properties(identifier)
    }

    /// Returns the properties of a registered gas material
    pub fn gas_properties(
        &self,
        identifier: MaterialIdentifier,
    ) -> Option<&MaterialPropertiesGas> {
        self.material_registry.gas_properties(identifier)
    }

    /// Returns the static cellular GPU properties with 32-byte records aligned to 16 bytes
    pub fn cellular_static_properties_buffer(&self) -> &wgpu::Buffer {
        &self.cellular_static_properties_buffer
    }

    /// Returns dynamic cellular GPU properties when defined
    pub fn cellular_dynamic_properties_buffer(&self) -> Option<&wgpu::Buffer> {
        self.cellular_dynamic_properties_buffer.as_ref()
    }

    /// Returns fluid GPU properties when defined
    pub fn fluid_properties_buffer(&self) -> Option<&wgpu::Buffer> {
        self.fluid_properties_buffer.as_ref()
    }

    /// Returns gas GPU properties when defined
    pub fn gas_properties_buffer(&self) -> Option<&wgpu::Buffer> {
        self.gas_properties_buffer.as_ref()
    }

    /// Returns the compiled static cellular albedo atlas
    pub fn cellular_static_albedo_atlas_texture_view(&self) -> &wgpu::TextureView {
        &self.cellular_static_albedo_atlas_texture_view
    }

    /// Returns static cellular repetition sizes including reserved empty index zero
    pub fn cellular_static_repetition_sizes(&self) -> &[[f32; 2]] {
        &self.cellular_static_repetition_sizes
    }

}

impl MaterialRegistry {

    /// Compiles this registry into immutable material resources
    pub fn compile(self, accelerator: &Accelerator) -> Result<MaterialTable, MaterialError> {
        MaterialTable::new(accelerator, self).map_err(MaterialError::Compilation)
    }

}
