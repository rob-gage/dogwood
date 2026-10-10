// Copyright Rob Gage 2026

use super::actor::Actor;
use std::{
    error::Error,
    fmt::{
        Display,
        Formatter,
        Result,
    },
};

/// An error encountered while managing actors
#[derive(Clone, Debug)]
pub enum ActorError {
    /// Every actor identifier of the scene has been allocated
    ActorIdentifiersExhausted,
    /// The actor has no live entity because it is unloaded or was despawned
    ActorNotLoaded {
        /// Actor that is not loaded
        actor: Actor,
    },
    /// The live actor does not have the requested component
    ActorComponentMissing {
        /// Actor missing the component
        actor: Actor,
    },
}

impl Display for ActorError {

    /// Describes the error
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
        match self {
            Self::ActorIdentifiersExhausted =>
                write!(formatter, "Actor identifiers are exhausted"),
            Self::ActorNotLoaded { actor } =>
                write!(formatter, "Actor {actor:?} is not loaded"),
            Self::ActorComponentMissing { actor } =>
                write!(formatter, "Actor {actor:?} does not have the requested component"),
        }
    }

}

impl Error for ActorError {}
