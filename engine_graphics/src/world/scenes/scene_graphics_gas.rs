// Copyright Rob Gage 2026

/// Borrowed gas cells and material properties for one resident tile region
#[derive(Clone, Debug)]
pub struct SceneGraphicsGas<'a> {
    /// Eight u32 per cell: four species slots holding a gas index in bits 24 to 31 and an amount
    /// in bits 0 to 23, then phantom air amount, face flows, and padding
    pub gas_buffer: &'a wgpu::Buffer,
    /// One u32 per cell, zero is empty
    pub material_identifier_buffer: &'a wgpu::Buffer,
    /// Mass, diffusivity, cohesion, packed sRGB color bits, opacity, and padding as eight 32-bit
    /// words per gas index
    pub material_properties_buffer: &'a wgpu::Buffer,
    /// Horizontal offset of the region's bottom-left corner from the camera in tiles
    pub region_x: f32,
    /// Vertical offset of the region's bottom-left corner from the camera in tiles
    pub region_y: f32,
    /// Region width in tiles
    pub region_width: u32,
    /// Region height in tiles
    pub region_height: u32,
    /// Physical ring column holding the region's leftmost tiles
    pub ring_offset_x: u32,
    /// Physical ring row holding the region's bottom tiles
    pub ring_offset_y: u32,
    /// Number of cells along each side of a tile
    pub cellular_tile_size: u32,
    /// Gas amount units in one atmosphere
    pub amount_per_atmosphere: f32,
}
