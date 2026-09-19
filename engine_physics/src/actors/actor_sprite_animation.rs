// Copyright Rob Gage 2026

use std::time::Duration;

use super::{ActorSpriteError, ActorSpriteSheet};

/// Identifies an animation in one [`ActorSprites`](super::ActorSprites) value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActorSpriteAnimationIdentifier(u64);

impl ActorSpriteAnimationIdentifier {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// One horizontal actor sprite-sheet animation.
///
/// The ordinary sheet is RGBA: RGB is the visible color and alpha controls
/// coverage. The optional radiance sheet is also RGBA: RGB is emitted
/// radiance, while its alpha is preserved for future occlusion work and is
/// currently ignored. Both sheets use the same horizontal frame layout.
#[derive(Clone, Debug)]
pub struct ActorSpriteAnimation {
    sprite_sheet: ActorSpriteSheet,
    radiance_sprite_sheet: Option<ActorSpriteSheet>,
    frame_width: u32,
    frame_height: u32,
    frame_count: u32,
    playback_rate: f32,
    loops: bool,
}

impl ActorSpriteAnimation {
    /// Creates an animation with `playback_rate` frames per second.
    pub fn new(
        sprite_sheet: ActorSpriteSheet,
        radiance_sprite_sheet: Option<ActorSpriteSheet>,
        frame_width: u32,
        frame_height: u32,
        frame_count: u32,
        playback_rate: f32,
        loops: bool,
    ) -> Result<Self, ActorSpriteError> {
        if frame_width == 0 || frame_height == 0 {
            return Err(ActorSpriteError::InvalidFrameDimensions);
        }
        if frame_count == 0 {
            return Err(ActorSpriteError::InvalidFrameCount);
        }
        if frame_width
            .checked_mul(frame_count)
            .is_none_or(|required_width| {
                sprite_sheet.width() < required_width || sprite_sheet.height() < frame_height
            })
        {
            return Err(ActorSpriteError::FrameLayoutExceedsSpriteSheet);
        }
        if radiance_sprite_sheet.as_ref().is_some_and(|sheet| {
            sheet.width() != sprite_sheet.width() || sheet.height() != sprite_sheet.height()
        }) {
            return Err(ActorSpriteError::RadianceSpriteSheetDimensionsMismatch);
        }
        if !playback_rate.is_finite()
            || playback_rate <= 0.0
            || 1.0_f64 / playback_rate as f64 > Duration::MAX.as_secs_f64()
        {
            return Err(ActorSpriteError::InvalidPlaybackRate);
        }
        Ok(Self {
            sprite_sheet,
            radiance_sprite_sheet,
            frame_width,
            frame_height,
            frame_count,
            playback_rate,
            loops,
        })
    }

    /// Creates an animation from a horizontal RGBA sprite sheet.
    pub fn from_rgba_sprite_sheet(
        sprite_sheet: ActorSpriteSheet,
        radiance_sprite_sheet: Option<ActorSpriteSheet>,
        frame_size: [u32; 2],
        frame_count: u32,
        playback_rate: f32,
        loops: bool,
    ) -> Result<Self, ActorSpriteError> {
        Self::new(
            sprite_sheet,
            radiance_sprite_sheet,
            frame_size[0],
            frame_size[1],
            frame_count,
            playback_rate,
            loops,
        )
    }

    /// Returns the ordinary RGBA visible sprite sheet.
    pub const fn sprite_sheet(&self) -> &ActorSpriteSheet {
        &self.sprite_sheet
    }

    /// Returns the matching radiance sheet. `None` means an all-black RGBA sheet.
    pub const fn radiance_sprite_sheet(&self) -> Option<&ActorSpriteSheet> {
        self.radiance_sprite_sheet.as_ref()
    }

    /// Returns the frame width in source pixels.
    pub const fn frame_width(&self) -> u32 {
        self.frame_width
    }

    /// Returns the frame height in source pixels.
    pub const fn frame_height(&self) -> u32 {
        self.frame_height
    }

    /// Returns the number of horizontal frames.
    pub const fn frame_count(&self) -> u32 {
        self.frame_count
    }

    /// Returns the authored playback rate in frames per second.
    pub const fn playback_rate(&self) -> f32 {
        self.playback_rate
    }

    /// Returns whether playback wraps to the first frame.
    pub const fn loops(&self) -> bool {
        self.loops
    }

    /// Returns the authored duration of one frame before runtime speed scaling.
    pub fn frame_duration(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.playback_rate as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite_sheet(width: u32, height: u32) -> ActorSpriteSheet {
        ActorSpriteSheet::new(width, height, vec![0; (width * height * 4) as usize]).unwrap()
    }

    #[test]
    fn validates_frame_geometry_and_matching_radiance_dimensions() {
        let ordinary_sheet = sprite_sheet(4, 2);
        let radiance_sheet = sprite_sheet(2, 2);
        assert!(matches!(
            ActorSpriteAnimation::from_rgba_sprite_sheet(
                ordinary_sheet.clone(),
                Some(radiance_sheet),
                [2, 2],
                2,
                8.0,
                true,
            ),
            Err(ActorSpriteError::RadianceSpriteSheetDimensionsMismatch)
        ));
        assert!(matches!(
            ActorSpriteAnimation::from_rgba_sprite_sheet(
                ordinary_sheet,
                None,
                [3, 2],
                2,
                8.0,
                true,
            ),
            Err(ActorSpriteError::FrameLayoutExceedsSpriteSheet)
        ));
    }

    #[test]
    fn rejects_invalid_playback_rates() {
        assert!(matches!(
            ActorSpriteAnimation::new(sprite_sheet(1, 1), None, 1, 1, 1, 0.0, true),
            Err(ActorSpriteError::InvalidPlaybackRate)
        ));
        assert!(matches!(
            ActorSpriteAnimation::new(sprite_sheet(1, 1), None, 1, 1, 1, f32::NAN, true),
            Err(ActorSpriteError::InvalidPlaybackRate)
        ));
    }
}
