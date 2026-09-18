// Copyright Rob Gage 2026

use crate::scenes::{ScenePosition, SceneVelocity};

/// Integrates an actor's continuous velocity and normalizes its tile-relative position
pub(super) fn integrate_actor_position(
    position: &mut ScenePosition,
    velocity: &SceneVelocity,
    delta_time: f32,
) {
    let world_position_x: f32 =
        position.tile_coordinates.x as f32 + position.x_offset + velocity.x * delta_time;
    let world_position_y: f32 =
        position.tile_coordinates.y as f32 + position.y_offset + velocity.y * delta_time;
    let containing_tile_x: f32 = world_position_x.floor();
    let containing_tile_y: f32 = world_position_y.floor();
    position.tile_coordinates.x = containing_tile_x as i32;
    position.tile_coordinates.y = containing_tile_y as i32;
    position.x_offset = world_position_x - containing_tile_x;
    position.y_offset = world_position_y - containing_tile_y;
}
