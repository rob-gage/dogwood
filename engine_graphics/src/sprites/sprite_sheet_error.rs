// Copyright Rob Gage 2026

use std::fmt;

/// An error encountered while creating a `SpriteSheet`
#[derive(Debug)]
pub enum SpriteSheetError {
    /// The image data could not be decoded in its expected format
    Decode(image::ImageError),
    /// The image is empty, too large for the device, or not divisible into the frame grid
    InvalidDimensions,
}

impl fmt::Display for SpriteSheetError {

    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) =>
                write!(formatter, "Sprite sheet image could not be decoded: {error}"),
            Self::InvalidDimensions =>
                write!(formatter, "Sprite sheet image is empty, \
                too large, or not divisible into its frame grid."),
        }
    }

}

impl std::error::Error for SpriteSheetError {

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::InvalidDimensions => None,
        }
    }

}
