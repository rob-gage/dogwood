// Copyright Rob Gage 2026

/// Graphics properties for a fluid material
#[derive(Clone, Copy, Debug)]
pub struct MaterialGraphicsFluid {
    /// Linear RGB fraction of light absorbed per cell of depth below the fluid's surface, as an
    /// exponential rate
    pub absorption: [f32; 3],
    /// Rate per cell of depth at which the fluid's transparency fades toward opaque
    pub depth_opacity: f32,
    /// Multiplier of the fluid's lit color within its surface band
    pub surface_brightness: f32,
    /// The light falling on the surface, added within the band, scaled by the fluid's opacity
    pub surface_sheen: f32,
    /// Height in cells of the band along an open surface's top cell, from 0 to 1; zero means no
    /// band
    pub surface_band: f32,
    /// Strength of the noise varying the lit color's brightness, averaging out
    pub shimmer: f32,
    /// Feature size of the texture in cells
    pub texture_scale: f32,
    /// How fast the texture churns
    pub texture_speed: f32,
    /// Strength of the dark crust and bright seams on the emission, averaging out
    pub crust: f32,
}

impl Default for MaterialGraphicsFluid {
    /// Returns the graphics properties that reproduce the original fluid look
    fn default() -> Self {
        Self {
            absorption: [0.0356; 3],
            depth_opacity: 0.0,
            surface_brightness: 1.08,
            surface_sheen: 0.0,
            surface_band: 1.0,
            shimmer: 0.0,
            texture_scale: 3.0,
            texture_speed: 1.0,
            crust: 0.0,
        }
    }
}
