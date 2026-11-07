// Copyright Rob Gage 2026

use crate::scenes::soft_bodies::{SceneSoftBody, SoftBodyDescriptor};
use bevy_ecs::component::Component;

/// A push on the particles of an actor's soft body, applied at its next simulation step
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ActorSoftBodyImpulse {
    /// Adds velocity in tiles per second to one particle
    Particle {
        /// The pushed particle
        particle: u32,
        /// Added velocity
        velocity: [f32; 2],
    },
    /// Adds velocity in tiles per second to every particle
    All {
        /// Added velocity
        velocity: [f32; 2],
    },
}

/// A soft body and its cells that live and die with an actor: the actor's position follows the
/// body, and when the actor despawns the body leaves the world with its cells. An actor without
/// `ActorPersistenceComponent` despawns when its body reaches the edge of the simulated region
#[derive(Component, Clone, Debug)]
pub struct ActorSoftBodyComponent {
    /// The body to create at the next simulation step, taken once it exists
    pub descriptor: Option<SoftBodyDescriptor>,
    /// The body once created, or an existing body this actor takes over
    pub body: Option<SceneSoftBody>,
    /// Pushes for the next simulation step, drained when applied
    pub impulses: Vec<ActorSoftBodyImpulse>,
    /// Whether the scene had no capacity left for the body; the descriptor is kept for a retry
    pub is_unplaced: bool,
}

impl ActorSoftBodyComponent {

    /// Returns a component that creates a body from a descriptor
    pub fn new(descriptor: SoftBodyDescriptor) -> Self {
        Self { descriptor: Some(descriptor), body: None, impulses: Vec::new(), is_unplaced: false }
    }

    /// Returns a component that takes over an existing body
    pub fn of(body: SceneSoftBody) -> Self {
        Self { descriptor: None, body: Some(body), impulses: Vec::new(), is_unplaced: false }
    }

}
