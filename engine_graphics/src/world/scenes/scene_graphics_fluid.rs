// Copyright Rob Gage 2026

/// Borrowed fluid raster and material properties for one resident tile region
#[derive(Clone, Debug)]
pub struct SceneGraphicsFluid<'a> {
    /// Fixed-point mass, momentum, and dominant material for each physical ring cell
    pub raster_buffer: &'a wgpu::Buffer,
    /// Eight words per physical cell: words 0..3 are existing mass-weighted packed sRGBA8 channel
    /// sums; words 4..6 sum `round(min(emission_channel, 127) * 64) * mass_fixed`; word 7 sums
    /// `round(occlusion * 255) * mass_fixed`
    pub color_buffer: &'a wgpu::Buffer,
    /// One `vec4<i32>` per physical cell: capacity*1024, energy*64, conduction*1024, and
    /// temperature change as f32 bits
    pub heat_buffer: &'a wgpu::Buffer,
    /// Seven physics values followed by packed sRGB color per 32-byte fluid material record
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
}
