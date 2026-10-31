// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialColorTransformation;

/// A visual-only key selected by the amount in a cell or fluid particle
pub struct MaterialAmountColor {
    /// Amount per unit, where one is a full cell or particle
    pub amount: f32,
    /// Transformation of the stored color
    pub color: MaterialColorTransformation,
    /// Transformation of the stored radiance
    pub radiance: MaterialColorTransformation,
}

impl MaterialAmountColor {

    /// Creates a key with identity color and radiance transformations
    pub fn new(amount: f32) -> Self {
        Self {
            amount,
            color: MaterialColorTransformation::IDENTITY,
            radiance: MaterialColorTransformation::IDENTITY,
        }
    }

    /// Sets the color transformation applied at this key
    pub fn with_color(mut self, color: MaterialColorTransformation) -> Self {
        self.color = color;
        self
    }

    /// Sets the radiance transformation applied at this key
    pub fn with_radiance(mut self, radiance: MaterialColorTransformation) -> Self {
        self.radiance = radiance;
        self
    }

}
