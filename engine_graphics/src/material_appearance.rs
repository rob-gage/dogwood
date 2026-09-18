// Copyright Rob Gage 2026

use super::Color;

/// Optical attenuation used by the first scene-lighting pass.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MaterialOptics {
    /// Fraction of light blocked while crossing one fully occupied world cell.
    ///
    /// Values are clamped to `0.0..=1.0` when authored through the builder.
    pub occlusion: f32,
}

impl Default for MaterialOptics {
    fn default() -> Self {
        Self { occlusion: 0.0 }
    }
}

/// The appearance of one material
#[derive(Copy, Clone, Debug)]
pub struct MaterialAppearance {
    /// The `Color` of this material at its freezing point
    color_freezing: Color,
    /// The `Color` of this material at its melting point
    color_melting: Color,
    /// The radiance light `Color` of this material at its freezing point
    radiance_freezing: Color,
    /// The radiance light `Color` of this material at its melting point
    radiance_melting: Color,
    /// Dimensionless per-channel initialization amplitude for persistent cell variation.
    /// A new cell receives a signed normalized sample in `[-variation, +variation]`;
    /// this is an initialization amplitude, not statistical variance.
    variation: [f32; 4],
    /// Dimensionless fractional per-channel modulation of base surface color.
    /// Rendering applies `base * (1 + sample * color_influence)` and clamps the result.
    color_influence: [f32; 4],
    /// Dimensionless fractional per-channel modulation of emitted radiance.
    /// The future lighting path applies `base * (1 + sample * radiance_influence)`.
    radiance_influence: [f32; 4],
    optics: MaterialOptics,
}

impl MaterialAppearance {
    /// Creates new material appearance information
    pub const fn new(
        color_freezing: Color,
        color_melting: Color,
        radiance_freezing: Color,
        radiance_melting: Color,
    ) -> Self {
        Self {
            color_freezing,
            color_melting,
            radiance_freezing,
            radiance_melting,
            variation: [0.0; 4],
            color_influence: [0.0; 4],
            radiance_influence: [0.0; 4],
            optics: MaterialOptics { occlusion: 0.0 },
        }
    }

    /// Sets dimensionless initialization amplitudes in the authored range `0..=1`.
    pub const fn with_variation(mut self, variation: [f32; 4]) -> Self {
        self.variation = variation;
        self
    }

    /// Sets dimensionless fractional modulation of each base color channel
    pub const fn with_color_influence(mut self, influence: [f32; 4]) -> Self {
        self.color_influence = influence;
        self
    }

    /// Sets dimensionless fractional modulation of each emitted radiance channel
    pub const fn with_radiance_influence(mut self, influence: [f32; 4]) -> Self {
        self.radiance_influence = influence;
        self
    }

    /// Returns this material's per-channel persistent variation initialization amplitudes
    pub const fn variation(self) -> [f32; 4] {
        self.variation
    }

    /// Returns this material's per-channel base-color modulation fractions
    pub const fn color_influence(self) -> [f32; 4] {
        self.color_influence
    }

    /// Returns this material's per-channel radiance modulation fractions
    pub const fn radiance_influence(self) -> [f32; 4] {
        self.radiance_influence
    }

    /// Creates material appearance information from a static color
    pub const fn from_color(color: Color) -> Self {
        Self::new(color, color, Color::BLACK, Color::BLACK)
    }

    /// Returns this material's appearance with a new radiance light color
    pub const fn with_radiance(mut self, radiance: Color) -> Self {
        self.radiance_freezing = radiance;
        self.radiance_melting = radiance;
        self
    }

    /// Sets the fraction of light blocked while crossing one full world cell.
    pub const fn with_occlusion(mut self, occlusion: f32) -> Self {
        self.optics = MaterialOptics {
            occlusion: if occlusion < 0.0 {
                0.0
            } else if occlusion > 1.0 {
                1.0
            } else {
                occlusion
            },
        };
        self
    }

    /// Compatibility alias for older material definitions.
    pub const fn with_extinction(self, extinction: f32) -> Self {
        self.with_occlusion(extinction)
    }

    /// Returns the material's optical properties.
    pub const fn optics(self) -> MaterialOptics {
        self.optics
    }

    /// Returns this material's base color
    pub const fn base_color(self) -> Color {
        self.color_freezing
    }

    /// Returns this material's Accelerator representation
    pub fn accelerator_data(self) -> [u32; 24] {
        // WGSL MaterialAppearance is four packed colors, three vec4 fields, and
        // one occlusion scalar and WGSL's 16-byte alignment padding make a
        // 96-byte storage-buffer element.
        let mut data: [u32; 24] = [0; 24];
        data[0] = self.color_freezing.as_u32();
        data[1] = self.color_melting.as_u32();
        data[2] = self.radiance_freezing.as_u32();
        data[3] = self.radiance_melting.as_u32();
        for (index, value) in self.variation.into_iter().enumerate() {
            data[4 + index] = value.to_bits();
        }
        for (index, value) in self.color_influence.into_iter().enumerate() {
            data[8 + index] = value.to_bits();
        }
        for (index, value) in self.radiance_influence.into_iter().enumerate() {
            data[12 + index] = value.to_bits();
        }
        data[16] = self.optics.occlusion.to_bits();
        data
    }
}
