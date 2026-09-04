// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;

/// A continuous position represented relative to a discrete tile coordinate
#[derive(Copy, Clone, bevy_ecs::component::Component)]
pub struct ScenePosition {
    /// The tile containing this position
    pub tile_coordinates: TileCoordinates,
    /// The horizontal offset within the containing tile
    /// (`0.0` is the left side, `1.0` is the right side)
    pub x_offset: f32,
    /// The vertical offset within the containing tile
    /// (`0.0` is the bottom side, `1.0` is the top side)
    pub y_offset: f32,
}