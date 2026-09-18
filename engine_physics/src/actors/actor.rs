// Copyright Rob Gage 2026

/// Identifies an actor managed by an `ActorRegistry`
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Actor(u64);

impl Actor {
    pub(crate) const fn new(identifier: u64) -> Self {
        Self(identifier)
    }

    pub(crate) const fn stable_identifier(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    pub(crate) const fn from_bevy_entity(entity: bevy_ecs::entity::Entity) -> Self {
        Self::new(entity.to_bits())
    }
}
