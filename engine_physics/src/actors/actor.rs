// Copyright Rob Gage 2026

use super::ActorClass;

/// A game-logic controlled entity that interacts with the physics world;
/// Players, NPCs, projectiles, etc.
pub struct Actor {
    /// The class of this `Actor`
    class: Box<dyn ActorClass>,
}