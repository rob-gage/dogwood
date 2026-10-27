// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialColorTransformation;

/// A change of a material into another at its transition temperature, such as
/// melting, freezing, boiling, or condensing
pub struct MaterialTransition {
    /// Temperature in kelvin at which the material changes
    pub temperature: f32,
    /// Heat per unit of amount the material takes in while changing on heating, or gives off while
    /// changing on cooling, in heat capacity times kelvin
    pub latent_heat: f32,
    /// Name of the material of any form the material changes into
    pub product: String,
    /// Amount of the product made from each unit of amount, where a gas amount is in atmospheres
    /// of one cell
    pub amount: f32,
    /// Transformation of the color the material carries into its product, unused by gases, whose
    /// color belongs to their material
    pub color: MaterialColorTransformation,
}

impl MaterialTransition {

    /// Creates a transition with amount one and an identity color transformation
    pub fn new(temperature: f32, latent_heat: f32, product: impl Into<String>) -> Self {
        Self {
            temperature,
            latent_heat,
            product: product.into(),
            amount: 1.0,
            color: MaterialColorTransformation::IDENTITY,
        }
    }

    /// Sets the amount of product made by the transition
    pub fn with_amount(mut self, amount: f32) -> Self {
        self.amount = amount;
        self
    }

    /// Sets the color transformation applied by the transition
    pub fn with_color(mut self, color: MaterialColorTransformation) -> Self {
        self.color = color;
        self
    }

}
