// Copyright Rob Gage 2026

use super::Color;

/// Graphics properties for one material
#[derive(Copy, Clone)]
pub struct MaterialGraphicsProperties {
    /// The `Color` of this material at its freezing point
    color_freezing: Color,
    /// The `Color` of this material at its melting point
    color_melting: Color,
    /// The radiance light `Color` of this material at its freezing point
    radiance_freezing: Color,
    /// The radiance light `Color` of this material at its melting point
    radiance_melting: Color,
}

impl MaterialGraphicsProperties {

    /// Creates new material graphics information
    pub const fn new(
        color_freezing: Color,
        color_melting: Color,
        radiance_freezing: Color,
        radiance_melting: Color,
    ) -> Self { Self { color_freezing, color_melting, radiance_freezing, radiance_melting } }

    /// Creates material graphics information from a static color
    pub const fn from_color(color: Color) -> Self {
        Self::new(color, color, Color::BLACK, Color::BLACK)
    }

    /// Returns this material's graphics information with a new radiance light color
    pub const fn with_radiance(mut self, radiance: Color) -> Self {
        self.radiance_freezing = radiance;
        self.radiance_freezing = radiance;
        self
    }

}
