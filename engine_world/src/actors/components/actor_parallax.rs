// Copyright Rob Gage 2026

use crate::scenes::ScenePosition;
use super::ActorPersistenceComponent;
use engine_graphics::world::scenes::ParallaxRepeat;
use bevy_ecs::component::Component;

/// Routes an actor's sprites into the background parallax renderer.
/// Combine with `ActorSpritesComponent` and `ActorKinematicsPositionComponent`.
/// Gameplay can change depth, position, frames, tint, and sprite scale each frame.
/// Requires persistence so a background anchor leaving simulation does not delete its layer.
#[derive(Component, Copy, Clone, Debug)]
#[require(ActorPersistenceComponent)]
pub struct ActorParallaxComponent {
    /// Distance behind gameplay in scene tiles; zero lies on the gameplay plane.
    /// Position, sprite offsets, and sprite dimensions project by `1 / (1 + depth)`.
    /// Negative or nonfinite depths skip this actor's sprites.
    pub depth: f32,
    /// Layer-wide horizontal and vertical placement in unprojected scene tiles.
    /// Adds to actor position and sprite offsets, whether repeated or finite.
    pub offset: [f32; 2],
    /// Repetition along each sprite-local axis; defaults to finite on both axes.
    pub repeat: [ParallaxRepeat; 2],
    /// Horizontal and vertical camera-motion multipliers, applied after depth projection.
    /// One uses depth-derived scrolling; zero follows the camera on that axis.
    /// Negative factors reverse scrolling. Nonfinite factors skip the actor's sprites.
    pub scroll_factor: [f32; 2],
    /// Camera position at which scroll factors leave the world placement unchanged.
    /// Choose a nearby anchor when using nondefault factors in very large worlds.
    pub scroll_origin: ScenePosition,
}

impl Default for ActorParallaxComponent {
    fn default() -> Self {
        Self { depth: 0.0, offset: [0.0; 2], repeat: [ParallaxRepeat::None; 2], scroll_factor: [1.0; 2], scroll_origin: ScenePosition::default() }
    }
}
