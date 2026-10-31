// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialColorTransformation;

/// A material produced by a reaction
pub struct MaterialReactionProduct {
    /// Exact, case-sensitive product material name
    pub product: String,
    /// Amount produced per event, with gas amount in atmospheres of one cell
    pub amount: f32,
    /// Transformation of the reactants' stored color
    pub color: MaterialColorTransformation,
    /// Transformation of the reactants' stored radiance
    pub radiance: MaterialColorTransformation,
}

impl MaterialReactionProduct {

    /// Creates a reaction product with identity color and radiance transformations
    pub fn new(product: impl Into<String>, amount: f32) -> Self {
        Self {
            product: product.into(),
            amount,
            color: MaterialColorTransformation::IDENTITY,
            radiance: MaterialColorTransformation::IDENTITY,
        }
    }

    /// Sets the color transformation applied to this product
    pub fn with_color(mut self, color: MaterialColorTransformation) -> Self {
        self.color = color;
        self
    }

    /// Sets the radiance transformation applied to this product
    pub fn with_radiance(mut self, radiance: MaterialColorTransformation) -> Self {
        self.radiance = radiance;
        self
    }

}
