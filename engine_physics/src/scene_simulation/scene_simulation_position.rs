// Copyright Rob Gage 2026

use crate::scenes::{ScenePosition, SceneVelocity};

/// Integrates an actor's continuous velocity and normalizes its tile-relative position
pub(super) fn integrate_actor_position(
    position: &mut ScenePosition,
    velocity: &SceneVelocity,
    delta_time: f32,
) {
    let x: f32 = position.tile_coordinates.x as f32 + position.x_offset + velocity.x * delta_time;
    let y: f32 = position.tile_coordinates.y as f32 + position.y_offset + velocity.y * delta_time;
    let tile_x: f32 = x.floor();
    let tile_y: f32 = y.floor();
    position.tile_coordinates.x = tile_x as i32;
    position.tile_coordinates.y = tile_y as i32;
    position.x_offset = x - tile_x;
    position.y_offset = y - tile_y;
}
