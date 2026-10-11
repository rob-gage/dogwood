// Copyright Rob Gage 2026

/// Every textured sprite drawn in a scene, each borrowing its texture
#[derive(Clone, Debug, Default)]
pub struct SceneGraphicsSprites<'a> {
    /// Sprites as texture, x, y, width, height, rotation, depth, texture region, and linear color
    pub sprites: Vec<(&'a wgpu::TextureView, f32, f32, f32, f32, f32, i32, [f32; 4], [f32; 4])>,
}
