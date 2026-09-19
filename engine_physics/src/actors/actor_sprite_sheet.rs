// Copyright Rob Gage 2026

use std::sync::Arc;

/// A shareable CPU-side RGBA sprite sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorSpriteSheet {
    width: u32,
    height: u32,
    rgba_data: Arc<[u8]>,
}

impl ActorSpriteSheet {
    /// Creates a sprite sheet from tightly packed RGBA8 pixels.
    pub fn new(width: u32, height: u32, rgba_data: impl Into<Arc<[u8]>>) -> Option<Self> {
        let rgba_data: Arc<[u8]> = rgba_data.into();
        let expected_length: usize = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?
            .checked_mul(4)?;
        (width > 0 && height > 0 && rgba_data.len() == expected_length).then_some(Self {
            width,
            height,
            rgba_data,
        })
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

    pub fn rgba_data_shared(&self) -> Arc<[u8]> {
        Arc::clone(&self.rgba_data)
    }
}
