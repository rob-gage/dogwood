// Copyright Rob Gage 2026

use engine_graphics::world::materials::MaterialColorTransformation;

/// A visual-only key; at render time the transformations of the two keys around a cell's
/// temperature are interpolated and applied to its stored color and radiance, never modifying
/// them. Below the first key or above the last, the nearest key applies; no keys means identity.
pub struct MaterialTemperatureColor {
    /// Temperature in kelvin
    pub temperature: f32,
    /// Transformation of the stored color
    pub color: MaterialColorTransformation,
    /// Transformation of the stored radiance
    pub radiance: MaterialColorTransformation,
}

impl MaterialTemperatureColor {

    /// Creates a key with identity color and radiance transformations
    pub fn new(temperature: f32) -> Self {
        Self {
            temperature,
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
