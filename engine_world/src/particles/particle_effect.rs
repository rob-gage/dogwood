// Copyright Rob Gage 2026

/// A data-driven particle preset
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleEffect {
    /// Burst count range, inclusive
    pub count: [u32; 2],
    /// Initial speed range in tiles per second
    pub speed: [f32; 2],
    /// Half-angle in radians around the emit direction (`PI` is all directions)
    pub spread: f32,
    /// Lifetime range in seconds
    pub lifetime: [f32; 2],
    /// Downward acceleration in tiles per second squared (negative rises)
    pub gravity: f32,
    /// Exponential velocity damping per second
    pub drag: f32,
    /// Start size range in cells
    pub size: [f32; 2],
    /// Size multiplier reached at the end of life
    pub size_end: f32,
    /// Start color is picked uniformly between these linear RGBA colors
    pub colors: [[f32; 4]; 2],
    /// Linear RGBA color (including alpha) approached over life
    pub color_end: [f32; 4],
    /// Fraction of life spent fading alpha in (zero is instant)
    pub fade_in: f32,
    /// Radiance intensity at birth (zero for none), decaying with alpha
    pub glow: f32,
    /// Sprite depth
    pub depth: i32,
    /// Birth position jitter radius in tiles
    pub jitter: f32,
    /// Fraction of the emitter velocity added to particles
    pub inherit: f32,
    /// Zero for square pixels, otherwise a streak of `speed * stretch` single grid cells trailing
    /// back along the velocity, at most 12
    pub stretch: f32,
}

impl ParticleEffect {

    /// A neutral preset: one short-lived white cell flying in all directions
    pub const DEFAULT: ParticleEffect = ParticleEffect {
        count: [1, 1],
        speed: [1.0, 1.0],
        spread: std::f32::consts::PI,
        lifetime: [0.5, 0.5],
        gravity: 0.0,
        drag: 0.0,
        size: [1.0, 1.0],
        size_end: 1.0,
        colors: [[1.0, 1.0, 1.0, 1.0]; 2],
        color_end: [1.0, 1.0, 1.0, 0.0],
        fade_in: 0.0,
        glow: 0.0,
        depth: 0,
        jitter: 0.0,
        inherit: 0.0,
        stretch: 0.0,
    };

}

impl Default for ParticleEffect {
    fn default() -> Self { Self::DEFAULT }
}
