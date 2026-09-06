// Copyright Rob Gage 2026

use super::Color;

/// The appearance of one material
#[derive(Copy, Clone)]
pub struct MaterialAppearance {
    /// The `Color` of this material at its freezing point
    color_freezing: Color,
    /// The `Color` of this material at its melting point
    color_melting: Color,
    /// The radiance light `Color` of this material at its freezing point
    radiance_freezing: Color,
    /// The radiance light `Color` of this material at its melting point
    radiance_melting: Color,
}

impl MaterialAppearance {

    /// Creates new material appearance information
    pub const fn new(
        color_freezing: Color,
        color_melting: Color,
        radiance_freezing: Color,
        radiance_melting: Color,
    ) -> Self { Self { color_freezing, color_melting, radiance_freezing, radiance_melting } }

    /// Creates material appearance information from a static color
    pub const fn from_color(color: Color) -> Self {
        Self::new(color, color, Color::BLACK, Color::BLACK)
    }

    /// Returns this material's appearance with a new radiance light color
    pub const fn with_radiance(mut self, radiance: Color) -> Self {
        self.radiance_freezing = radiance;
        self.radiance_freezing = radiance;
        self
    }

    /// Returns this material's base color
    pub const fn base_color(self) -> Color { self.color_freezing }

    /// Returns this material's GPU representation
    pub const fn accelerator_data(self) -> [u32; 4] {
        [
            self.color_freezing.as_u32(),
            self.color_melting.as_u32(),
            self.radiance_freezing.as_u32(),
            self.radiance_melting.as_u32(),
        ]
    }

}
