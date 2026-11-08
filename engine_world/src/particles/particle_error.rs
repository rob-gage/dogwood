// Copyright Rob Gage 2026

use crate::actors::ActorError;
use engine_graphics::sprites::SpriteSheetError;
use std::fmt::{Display, Formatter, Result};

/// An error raised while creating a `ParticleSystem`
#[derive(Debug)]
pub enum ParticleError {
    /// The particle pixel sheets could not be created
    SpriteSheet(SpriteSheetError),
    /// The particle actor could not be spawned
    Actor(ActorError),
}

impl Display for ParticleError {

    /// Describes the error
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
        match self {
            Self::SpriteSheet(error) => write!(formatter, "Particle sprite sheet failed: {error:?}"),
            Self::Actor(error) => write!(formatter, "Particle actor failed: {error}"),
        }
    }

}

impl std::error::Error for ParticleError {}

impl From<SpriteSheetError> for ParticleError {
    fn from(error: SpriteSheetError) -> Self { Self::SpriteSheet(error) }
}

impl From<ActorError> for ParticleError {
    fn from(error: ActorError) -> Self { Self::Actor(error) }
}
