// Copyright Rob Gage 2026

/// Borrowed pressure cells for one resident tile region
#[derive(Clone, Debug)]
pub struct SceneGraphicsPressure<'a> {
    /// One f32 gauge pressure in atmospheres per row-major cell in each physical ring tile
    pub pressure_buffer: &'a wgpu::Buffer,
    /// Ambient pressure in atmospheres
    pub ambient_pressure: f32,
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
