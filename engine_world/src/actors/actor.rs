// Copyright Rob Gage 2026

/// Stable scene-local identity of an actor, valid whether or not the actor is loaded
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Actor(u64);

impl Actor {

    /// Creates an actor from an identifier allocated by `ActorEntities`
    pub(crate) const fn new(identifier: u64) -> Self {
        Self(identifier)
    }

}
