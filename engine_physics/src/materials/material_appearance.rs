// Copyright Rob Gage 2026

use engine_graphics::Color;

/// The appearance of a `Material`
pub struct MaterialAppearance {
    /// The `Color` of this `Material` at its freezing point
    color_freezing: Color,
    /// The `Color` of this `Material` at its melting point
    color_melting: Color,
    /// The radiance light `Color` of this `Material` at its freezing point
    radiance_freezing: Color,
    /// The radiance light `Color` of this `Material` at its melting point
    radiance_melting: Color,
}

impl MaterialAppearance {

    /// Creates a new `MaterialAppearance`
    pub const fn new(
        color_freezing: Color,
        color_melting: Color,
        radiance_freezing: Color,
        radiance_melting: Color,
    ) -> Self { Self { color_freezing, color_melting, radiance_freezing, radiance_melting } }

    /// Creates a new `MaterialAppearance` from a static color
    pub const fn from_color(color: Color) -> Self {
        Self::new(color, color, Color::BLACK, Color::BLACK)
    }

    /// Returns this `MaterialAppearance` with a new radiance light color
    pub const fn with_radiance(mut self, radiance: Color) -> Self {
        self.radiance_freezing = radiance;
        self.radiance_freezing = radiance;
        self
    }

}