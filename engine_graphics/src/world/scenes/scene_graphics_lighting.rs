// Copyright Rob Gage 2026

use super::LightingQuality;

/// Lighting inputs of one scene for one rendered frame
#[derive(Clone, Debug)]
pub struct SceneGraphicsLighting<'a> {
    /// Linear light added everywhere
    pub ambient: [f32; 3],
    /// Divisor of matter's extinction while light spreads into it, at least one
    pub penetration: f32,
    /// Resolution used to calculate radiance cascade lighting
    pub quality: LightingQuality,
    /// Temperature color keys of every material, as `MaterialTable::temperature_color_buffer`
    /// describes
    pub temperature_color_buffer: &'a wgpu::Buffer,
    /// Amount color keys of every material, as `MaterialTable::amount_color_buffer` describes
    pub amount_color_buffer: &'a wgpu::Buffer,
    /// First thermal index of each material form
    pub thermal_offsets: [u32; 4],
    /// Lights without sprites or materials
    pub lights: Vec<SceneGraphicsLight>,
}

/// One light without a sprite or material: a disc emitting light along every path through it
#[derive(Clone, Copy, Debug)]
pub struct SceneGraphicsLight {
    /// Camera-relative center in tiles
    pub center: [f32; 2],
    /// Radius in tiles
    pub radius: f32,
    /// Linear emission per cell of path
    pub emission: [f32; 3],
}
