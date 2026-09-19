// Copyright Rob Gage 2026

use std::collections::HashMap;
use std::time::Duration;

use super::{ActorSpriteAnimation, ActorSpriteAnimationIdentifier};

/// Sprite and animation playback state carried by any actor type.
#[derive(bevy_ecs::component::Component, Clone, Debug)]
pub struct ActorSprites {
    animations: HashMap<ActorSpriteAnimationIdentifier, ActorSpriteAnimation>,
    animation_names: HashMap<String, ActorSpriteAnimationIdentifier>,
    current_animation_identifier: Option<ActorSpriteAnimationIdentifier>,
    next_animation_identifier: u64,
    frame_index: u32,
    frame_elapsed: Duration,
    animation_speed: f32,
    paused: bool,
}

impl Default for ActorSprites {
    fn default() -> Self {
        Self {
            animations: HashMap::new(),
            animation_names: HashMap::new(),
            current_animation_identifier: None,
            next_animation_identifier: 0,
            frame_index: 0,
            frame_elapsed: Duration::ZERO,
            animation_speed: 1.0,
            paused: false,
        }
    }
}

impl ActorSprites {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a named animation and returns its stable identifier.
    pub fn add_animation(
        &mut self,
        name: impl Into<String>,
        animation: ActorSpriteAnimation,
    ) -> Option<ActorSpriteAnimationIdentifier> {
        let name: String = name.into();
        if self.animation_names.contains_key(&name) {
            return None;
        }
        let identifier: ActorSpriteAnimationIdentifier =
            ActorSpriteAnimationIdentifier::new(self.next_animation_identifier);
        self.next_animation_identifier = self.next_animation_identifier.wrapping_add(1);
        self.animation_names.insert(name, identifier);
        self.animations.insert(identifier, animation);
        if self.current_animation_identifier.is_none() {
            self.current_animation_identifier = Some(identifier);
        }
        Some(identifier)
    }

    pub fn animation_identifier(&self, name: &str) -> Option<ActorSpriteAnimationIdentifier> {
        self.animation_names.get(name).copied()
    }

    pub fn animation(
        &self,
        identifier: ActorSpriteAnimationIdentifier,
    ) -> Option<&ActorSpriteAnimation> {
        self.animations.get(&identifier)
    }

    pub fn play(&mut self, identifier: ActorSpriteAnimationIdentifier) -> bool {
        if !self.animations.contains_key(&identifier) {
            return false;
        }
        if self.current_animation_identifier != Some(identifier) {
            self.current_animation_identifier = Some(identifier);
            self.frame_index = 0;
            self.frame_elapsed = Duration::ZERO;
        }
        self.paused = false;
        true
    }

    pub fn restart(&mut self, identifier: ActorSpriteAnimationIdentifier) -> bool {
        if !self.play(identifier) {
            return false;
        }
        self.frame_index = 0;
        self.frame_elapsed = Duration::ZERO;
        true
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn set_animation_speed(&mut self, multiplier: f32) -> bool {
        if !multiplier.is_finite() || multiplier <= 0.0 {
            return false;
        }
        self.animation_speed = multiplier;
        true
    }

    pub const fn animation_speed(&self) -> f32 {
        self.animation_speed
    }

    pub const fn current_animation_identifier(&self) -> Option<ActorSpriteAnimationIdentifier> {
        self.current_animation_identifier
    }

    pub const fn frame_index(&self) -> u32 {
        self.frame_index
    }

    pub const fn frame_elapsed(&self) -> Duration {
        self.frame_elapsed
    }

    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    pub(crate) fn advance(&mut self, elapsed: Duration) {
        if self.paused {
            return;
        }
        let Some(identifier) = self.current_animation_identifier else {
            return;
        };
        let Some(animation) = self.animations.get(&identifier) else {
            return;
        };
        let scaled_elapsed: Duration =
            Duration::try_from_secs_f64(elapsed.as_secs_f64() * self.animation_speed as f64)
                .unwrap_or(Duration::MAX);
        self.frame_elapsed = self.frame_elapsed.saturating_add(scaled_elapsed);
        let frame_duration: Duration = animation.frame_duration();
        while self.frame_elapsed >= frame_duration {
            self.frame_elapsed -= frame_duration;
            if self.frame_index + 1 < animation.frame_count() {
                self.frame_index += 1;
            } else if animation.loops() {
                self.frame_index = 0;
            } else {
                self.frame_elapsed = Duration::ZERO;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::ActorSpriteSheet;

    fn animation(frame_count: u32, loops: bool) -> ActorSpriteAnimation {
        ActorSpriteAnimation::new(
            ActorSpriteSheet::new(8, 2, vec![0; 64]).unwrap(),
            None,
            2,
            2,
            frame_count,
            2.0,
            loops,
        )
        .unwrap()
    }

    #[test]
    fn named_animations_and_playback_state_are_stable() {
        let mut sprites = ActorSprites::new();
        let idle = sprites.add_animation("idle", animation(2, true)).unwrap();
        let walk = sprites.add_animation("walk", animation(3, true)).unwrap();
        assert_eq!(sprites.animation_identifier("idle"), Some(idle));
        assert_eq!(sprites.animation_identifier("walk"), Some(walk));
        assert_eq!(sprites.current_animation_identifier(), Some(idle));
        assert!(!sprites.play(ActorSpriteAnimationIdentifier::new(99)));

        sprites.advance(Duration::from_millis(600));
        assert_eq!(sprites.frame_index(), 1);
        assert!(sprites.play(idle));
        assert_eq!(sprites.frame_index(), 1);
        assert!(sprites.restart(idle));
        assert_eq!(sprites.frame_index(), 0);
        assert!(sprites.play(walk));
        assert_eq!(sprites.frame_index(), 0);
    }

    #[test]
    fn speed_pause_resume_and_looping_behave_as_expected() {
        let mut sprites = ActorSprites::new();
        let looping = sprites
            .add_animation("looping", animation(2, true))
            .unwrap();
        assert!(!sprites.set_animation_speed(0.0));
        assert!(sprites.set_animation_speed(2.0));
        sprites.advance(Duration::from_millis(300));
        assert_eq!(sprites.frame_index(), 1);
        sprites.pause();
        sprites.advance(Duration::from_secs(2));
        assert_eq!(sprites.frame_index(), 1);
        sprites.resume();
        sprites.advance(Duration::from_millis(300));
        assert_eq!(sprites.frame_index(), 0);
        assert!(sprites.play(looping));

        let mut non_looping = ActorSprites::new();
        let identifier = non_looping
            .add_animation("once", animation(2, false))
            .unwrap();
        assert!(non_looping.play(identifier));
        non_looping.advance(Duration::from_secs(2));
        assert_eq!(non_looping.frame_index(), 1);
        non_looping.advance(Duration::from_secs(2));
        assert_eq!(non_looping.frame_index(), 1);
    }

    #[test]
    fn missing_radiance_sheet_is_cheap() {
        let sheet = ActorSpriteSheet::new(2, 2, vec![0; 16]).unwrap();
        let animation = ActorSpriteAnimation::new(sheet, None, 2, 2, 1, 1.0, true).unwrap();
        assert!(animation.radiance_sprite_sheet().is_none());
    }
}
