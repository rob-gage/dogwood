// Copyright Rob Gage 2026

/// Borrowed art drawn where materials meet open space
#[derive(Clone, Copy, Debug)]
pub struct SceneGraphicsMaterialSurfaces<'a> {
    /// Array of world-aligned square tiles, with two layers per styled material: facing up, then
    /// facing down
    pub art: &'a wgpu::TextureView,
    /// One `u32` per thermal index of a material: one plus its first layer, or zero with no style
    pub layers: &'a wgpu::Buffer,
}
