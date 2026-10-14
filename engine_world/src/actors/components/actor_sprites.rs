// Copyright Rob Gage 2026

use engine_graphics::sprites::SpriteSheet;
use std::sync::Arc;
use bevy_ecs::component::Component;

/// Sprites drawn for an actor
#[derive(Component, Clone, Debug, Default)]
pub struct ActorSpritesComponent {
    /// Sprites as sheet, frame, offset x and y, scale x and y, rotation, color, and depth
    pub sprites: Vec<(Arc<SpriteSheet>, u32, f32, f32, f32, f32, f32, [f32; 4], i32)>,
}
