// Copyright Rob Gage 2026

/// Graphics properties for a gas material
pub struct MaterialGraphicsGas {
    /// Packed sRGB RGBA color; alpha is the opacity of one atmosphere of the gas
    pub color: u32,
    /// Multiplier stored with the gas material properties for atmosphere opacity
    pub opacity: f32,
}
