// Copyright Rob Gage 2026

/// Borrowed fluid raster and material properties for one resident tile region
#[derive(Clone, Debug)]
pub struct SceneGraphicsFluid<'a> {
    /// Fixed-point mass, momentum, and dominant material for each physical ring cell
    pub raster_buffer: &'a wgpu::Buffer,
    /// Summed sRGB particle color channels weighted by raster mass for each physical ring cell
    pub color_buffer: &'a wgpu::Buffer,
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
