// Copyright Rob Gage 2026

use std::time::Duration;

use super::ActorSpriteSheet;

/// Identifies an animation in one [`ActorSprites`](super::ActorSprites) value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActorSpriteAnimationIdentifier(u64);

impl ActorSpriteAnimationIdentifier {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// One horizontal actor sprite-sheet animation.
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
    ) -> Option<Self> {
        (frame_width > 0
            && frame_height > 0
            && frame_count > 0
            && sprite_sheet.width() >= frame_width.checked_mul(frame_count)?
            && sprite_sheet.height() >= frame_height
            && radiance_sprite_sheet.as_ref().is_none_or(|sheet| {
                sheet.width() == sprite_sheet.width() && sheet.height() == sprite_sheet.height()
            })
            && playback_rate.is_finite()
            && playback_rate > 0.0
            && 1.0_f64 / playback_rate as f64 <= Duration::MAX.as_secs_f64())
        .then_some(Self {
            sprite_sheet,
            radiance_sprite_sheet,
            frame_width,
            frame_height,
            frame_count,
            playback_rate,
            loops,
        })
    }

    pub const fn sprite_sheet(&self) -> &ActorSpriteSheet {
        &self.sprite_sheet
    }

    /// Returns the matching radiance sheet. `None` means an all-black RGBA sheet.
    pub const fn radiance_sprite_sheet(&self) -> Option<&ActorSpriteSheet> {
        self.radiance_sprite_sheet.as_ref()
    }

    pub const fn frame_width(&self) -> u32 {
        self.frame_width
    }

    pub const fn frame_height(&self) -> u32 {
        self.frame_height
    }

    pub const fn frame_count(&self) -> u32 {
        self.frame_count
    }

    pub const fn playback_rate(&self) -> f32 {
        self.playback_rate
    }

    pub const fn loops(&self) -> bool {
        self.loops
    }

    pub fn frame_duration(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.playback_rate as f64)
    }
}
