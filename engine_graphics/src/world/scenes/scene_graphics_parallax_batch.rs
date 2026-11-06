// Copyright Rob Gage 2026

/// One static group of sprites on a parallax plane, whose instances never change as the camera moves
#[derive(Clone, Debug)]
pub struct SceneGraphicsParallaxBatch<'a> {
    /// Color texture
    pub texture: &'a wgpu::TextureView,
    /// Radiance texture
    pub radiance_texture: &'a wgpu::TextureView,
    /// Sprite instances in the layout of the sprite renderer, positioned relative to `origin`
    pub instance_buffer: &'a wgpu::Buffer,
    /// Number of instances in the buffer
    pub instance_count: u32,
    /// Camera-relative position of the batch's origin in tiles
    pub origin: [f32; 2],
}
