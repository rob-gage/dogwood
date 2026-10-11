// Copyright Rob Gage 2026

/// Borrowed accelerator storage of a resident rectangular region of cellular tiles
#[derive(Clone, Debug)]
pub struct SceneGraphicsCellular<'a> {
    /// Material identifiers with one `u32` per cell, where zero is an empty cell
    pub material_identifier_buffer: &'a wgpu::Buffer,
    /// Little-endian packed sRGB RGBA8 colors with one `u32` per cell
    pub color_buffer: &'a wgpu::Buffer,
    /// Static materials by index as texture, texture region, and repeated width and height in tiles
    pub materials: Vec<Option<(&'a wgpu::TextureView, [f32; 4], f32, f32)>>,
    /// Horizontal tile coordinate of the region's bottom-left corner
    pub region_tile_x: i64,
    /// Vertical tile coordinate of the region's bottom-left corner
    pub region_tile_y: i64,
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
