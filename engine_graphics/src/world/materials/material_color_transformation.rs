// Copyright Rob Gage 2026

/// A transformation of a color in linear light: the color is multiplied by a matrix, offset, then
/// scaled and offset by random variances, and only clamped where it is stored, so intensities
/// above one survive the transformation
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialColorTransformation {
    /// Row-major matrix multiplying linear red, green, blue, and alpha
    pub matrix: [[f32; 4]; 4],
    /// Linear red, green, blue, and alpha added after the matrix
    pub offset: [f32; 4],
    /// Largest random fraction the whole color is brightened or darkened by, keeping its hue
    pub intensity_variance: f32,
    /// Largest random amount added to or removed from each linear channel
    pub channel_variance: [f32; 4],
}

impl MaterialColorTransformation {

    /// The transformation leaving every color unchanged
    pub const IDENTITY: Self = Self {
        matrix: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        offset: [0.0; 4],
        intensity_variance: 0.0,
        channel_variance: [0.0; 4],
    };

}
