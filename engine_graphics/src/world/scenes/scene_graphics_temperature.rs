// Copyright Rob Gage 2026

/// Borrowed cell temperatures for one resident tile region
#[derive(Clone, Debug)]
pub struct SceneGraphicsTemperature<'a> {
    /// Tile-major matter buffer with temperature bits and amount per row-major cell
    pub matter_buffer: &'a wgpu::Buffer,
    /// Ambient temperature in kelvin
    pub ambient_temperature: f32,
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
