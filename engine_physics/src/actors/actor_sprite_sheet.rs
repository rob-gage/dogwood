// Copyright Rob Gage 2026

use std::sync::Arc;

/// Errors returned while creating actor sprite images or animations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorSpriteError {
    /// The source image has zero width or height.
    InvalidImageDimensions,
    /// The source image does not contain exactly four bytes per pixel.
    InvalidRgbaDataLength,
    /// The PNG could not be decoded as an 8-bit RGBA image.
    InvalidPng(String),
    /// A frame dimension is zero.
    InvalidFrameDimensions,
    /// An animation contains no frames.
    InvalidFrameCount,
    /// The requested horizontal frame layout exceeds the ordinary sheet.
    FrameLayoutExceedsSpriteSheet,
    /// The radiance sheet dimensions do not match the ordinary sheet.
    RadianceSpriteSheetDimensionsMismatch,
    /// The authored playback rate is not finite and positive.
    InvalidPlaybackRate,
    /// Two animations were registered with the same name.
    DuplicateAnimationName(String),
}

impl std::fmt::Display for ActorSpriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidImageDimensions => {
                formatter.write_str("sprite image dimensions must be positive")
            }
            Self::InvalidRgbaDataLength => formatter
                .write_str("sprite image must contain exactly width * height * 4 RGBA bytes"),
            Self::InvalidPng(message) => {
                write!(formatter, "invalid RGBA PNG sprite image: {message}")
            }
            Self::InvalidFrameDimensions => {
                formatter.write_str("sprite animation frame dimensions must be positive")
            }
            Self::InvalidFrameCount => {
                formatter.write_str("sprite animation frame count must be positive")
            }
            Self::FrameLayoutExceedsSpriteSheet => {
                formatter.write_str("sprite animation frame layout exceeds the sprite sheet")
            }
            Self::RadianceSpriteSheetDimensionsMismatch => formatter
                .write_str("radiance sprite sheet dimensions must match the ordinary sprite sheet"),
            Self::InvalidPlaybackRate => {
                formatter.write_str("sprite animation playback rate must be finite and positive")
            }
            Self::DuplicateAnimationName(name) => write!(
                formatter,
                "sprite animation name is already registered: {name}"
            ),
        }
    }
}

impl std::error::Error for ActorSpriteError {}

/// A shareable CPU-side RGBA sprite sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorSpriteSheet {
    width: u32,
    height: u32,
    rgba_data: Arc<[u8]>,
}

impl ActorSpriteSheet {
    /// Creates a sprite sheet from tightly packed RGBA8 pixels.
    pub fn new(
        width: u32,
        height: u32,
        rgba_data: impl Into<Arc<[u8]>>,
    ) -> Result<Self, ActorSpriteError> {
        let rgba_data: Arc<[u8]> = rgba_data.into();
        if width == 0 || height == 0 {
            return Err(ActorSpriteError::InvalidImageDimensions);
        }
        let expected_length: usize = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|length| length.checked_mul(4))
            .ok_or(ActorSpriteError::InvalidRgbaDataLength)?;
        if rgba_data.len() != expected_length {
            return Err(ActorSpriteError::InvalidRgbaDataLength);
        }
        Ok(Self {
            width,
            height,
            rgba_data,
        })
    }

    /// Decodes an 8-bit RGBA PNG once into a shareable CPU-side image.
    pub fn from_png_bytes(png_bytes: &[u8]) -> Result<Self, ActorSpriteError> {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder
            .read_info()
            .map_err(|error| ActorSpriteError::InvalidPng(error.to_string()))?;
        let mut rgba_data = vec![0; reader.output_buffer_size()];
        let output_information = reader
            .next_frame(&mut rgba_data)
            .map_err(|error| ActorSpriteError::InvalidPng(error.to_string()))?;
        if output_information.color_type != png::ColorType::Rgba
            || output_information.bit_depth != png::BitDepth::Eight
        {
            return Err(ActorSpriteError::InvalidPng(
                "sprite PNGs must use 8-bit RGBA pixels".to_owned(),
            ));
        }
        Self::new(
            output_information.width,
            output_information.height,
            rgba_data[..output_information.buffer_size()].to_vec(),
        )
    }

    /// Returns the sheet width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the sheet height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the shared tightly packed RGBA8 pixels.
    pub fn rgba_data(&self) -> &[u8] {
        &self.rgba_data
    }

    /// Returns another reference to the shared tightly packed RGBA8 pixels.
    pub fn rgba_data_shared(&self) -> Arc<[u8]> {
        Arc::clone(&self.rgba_data)
    }
}

/// The game-facing name for an [`ActorSpriteSheet`].
///
/// A game can embed PNG assets at compile time:
///
/// ```ignore
/// let sprite_sheet = ActorSpriteImage::from_png_bytes(
///     include_bytes!("player_idle.png"),
/// )?;
/// # Ok::<(), ActorSpriteError>(())
/// ```
pub type ActorSpriteImage = ActorSpriteSheet;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_shares_rgba_png_data() {
        let mut png_bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut png_bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[1, 2, 3, 4])
            .unwrap();

        let image = ActorSpriteImage::from_png_bytes(&png_bytes).unwrap();
        assert_eq!(image.rgba_data(), &[1, 2, 3, 4]);
        let first_shared_data = image.rgba_data_shared();
        let second_shared_data = image.rgba_data_shared();
        assert!(Arc::ptr_eq(&first_shared_data, &second_shared_data));
    }

    #[test]
    fn rejects_malformed_png() {
        assert!(matches!(
            ActorSpriteImage::from_png_bytes(b"not a png"),
            Err(ActorSpriteError::InvalidPng(_))
        ));
    }
}
