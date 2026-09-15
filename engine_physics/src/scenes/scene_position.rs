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

impl ScenePosition {
    /// Interpolates from a previous fixed-tick position to this position
    pub(crate) fn interpolated(self, previous: Self, interpolation: f32) -> Self {
        let interpolation: f32 = interpolation.clamp(0.0, 1.0);
        let previous_x: f32 = previous.tile_coordinates.x as f32 + previous.x_offset;
        let previous_y: f32 = previous.tile_coordinates.y as f32 + previous.y_offset;
        let current_x: f32 = self.tile_coordinates.x as f32 + self.x_offset;
        let current_y: f32 = self.tile_coordinates.y as f32 + self.y_offset;
        let x: f32 = previous_x + (current_x - previous_x) * interpolation;
        let y: f32 = previous_y + (current_y - previous_y) * interpolation;
        let tile_x: f32 = x.floor();
        let tile_y: f32 = y.floor();
        Self {
            tile_coordinates: TileCoordinates {
                x: tile_x as i32,
                y: tile_y as i32,
            },
            x_offset: x - tile_x,
            y_offset: y - tile_y,
        }
    }
}
