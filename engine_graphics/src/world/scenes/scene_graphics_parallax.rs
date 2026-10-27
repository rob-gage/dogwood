// Copyright Rob Gage 2026

/// A projected background sprite retaining physical depth for lighting.
#[derive(Clone, Debug)]
pub struct SceneGraphicsParallax<'a> {
    /// Shared sprite sheet texture
    pub texture: &'a wgpu::TextureView,
    /// Shared radiance sprite sheet texture: linear emission and occlusion per tile
    pub radiance_texture: &'a wgpu::TextureView,
    /// Projected camera-relative center in tiles
    pub center: [f32; 2],
    /// Projected dimensions in tiles
    pub size: [f32; 2],
    /// Physical distance behind gameplay in tiles, independent of draw order
    pub depth: f32,
    /// Repetition along sprite-local horizontal and vertical axes
    pub repeat: [ParallaxRepeat; 2],
    /// Sprite draw order within the same physical depth
    pub order: i32,
    /// Counterclockwise rotation in radians
    pub rotation: f32,
    /// Left, top, right, and bottom frame texture edges
    pub texture_region: [f32; 4],
    /// Linear RGBA multiplier
    pub color: [f32; 4],
    /// Left, top, right, and bottom radiance frame texture edges
    pub radiance_texture_region: [f32; 4],
    /// Multiplier of the radiance emission
    pub radiance_intensity: f32,
}

/// Repetition along one sprite-local axis.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum ParallaxRepeat {
    /// Draws only the anchored sprite along this axis
    #[default]
    None,
    /// Repeats the frame without mirroring
    Repeat,
    /// Alternates mirrored copies, joining edge silhouettes of nonseamless artwork
    Mirror,
}

impl<'a> SceneGraphicsParallax<'a> {
    /// Expands only copies intersecting the camera's bounds in sprite-local space.
    /// Very dense patterns exceeding 16,384 copies per sprite are skipped.
    pub(crate) fn visible_copies(&self, width: f32, height: f32) -> Vec<Self> {
        let [tile_width, tile_height] = [self.size[0].abs(), self.size[1].abs()];
        if !tile_width.is_finite() || !tile_height.is_finite()
            || tile_width == 0.0 || tile_height == 0.0
        { return Vec::new(); }
        let (sin, cos) = self.rotation.sin_cos();
        let center = [
            cos * self.center[0] + sin * self.center[1],
            -sin * self.center[0] + cos * self.center[1],
        ];
        let extent = [cos.abs() * width / 2.0 + sin.abs() * height / 2.0,
                      sin.abs() * width / 2.0 + cos.abs() * height / 2.0];
        let mut ranges = [(0_i64, 0_i64); 2];
        for axis in 0..2 {
            if self.repeat[axis] == ParallaxRepeat::None { continue; }
            let size = [tile_width, tile_height][axis];
            let first = ((-extent[axis] - center[axis]) / size - 0.5).ceil();
            let last = ((extent[axis] - center[axis]) / size + 0.5).floor();
            if !first.is_finite() || !last.is_finite() || last - first > 16_384.0 {
                return Vec::new();
            }
            ranges[axis] = (first as i64, last as i64);
        }
        let count = (i128::from(ranges[0].1) - i128::from(ranges[0].0) + 1).max(0)
            * (i128::from(ranges[1].1) - i128::from(ranges[1].0) + 1).max(0);
        if count > 16_384 { return Vec::new(); }
        let mut copies = Vec::with_capacity(count as usize);
        for y in ranges[1].0..=ranges[1].1 {
            for x in ranges[0].0..=ranges[0].1 {
                let mut copy = self.clone();
                let dx = x as f32 * tile_width;
                let dy = y as f32 * tile_height;
                copy.center = [self.center[0] + cos * dx - sin * dy,
                               self.center[1] + sin * dx + cos * dy];
                for (axis, index) in [x, y].into_iter().enumerate() {
                    if self.repeat[axis] == ParallaxRepeat::Mirror && index.rem_euclid(2) == 1 {
                        copy.size[axis] = -copy.size[axis];
                    }
                }
                copies.push(copy);
            }
        }
        copies
    }
}
