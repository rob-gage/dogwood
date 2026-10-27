// Copyright Rob Gage 2026

use engine_graphics::sprites::SpriteSheet;
use std::sync::Arc;
use bevy_ecs::component::Component;

/// Sprites drawn for an actor
#[derive(Component, Clone, Debug, Default)]
pub struct ActorSpritesComponent {
    /// Sprites as albedo sheet, radiance sheet, frame, offset x and y, scale x and y, rotation,
    /// color, radiance intensity, and depth
    pub sprites: Vec<(
        Arc<SpriteSheet>,
        Arc<SpriteSheet>,
        u32,
        f32,
        f32,
        f32,
        f32,
        f32,
        [f32; 4],
        f32,
        i32,
    )>,
}
