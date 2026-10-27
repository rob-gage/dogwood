// Copyright Rob Gage 2026

/// Graphics properties for a gas material
pub struct MaterialGraphicsGas {
    /// Packed sRGB RGBA color of cells where this gas is the most plentiful; alpha is the opacity
    /// of one atmosphere of the gas
    pub color: u32,
    /// Multiplier of the atmospheres that build up the gas's opacity, so denser or thinner looking
    /// gases share one color alpha
    pub opacity: f32,
}
