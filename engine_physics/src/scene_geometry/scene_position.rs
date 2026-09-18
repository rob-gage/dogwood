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
    /// Creates a position from continuous world/tile coordinates.
    pub fn from_world(world: [f32; 2]) -> Self {
        let tile_x: f32 = world[0].floor();
        let tile_y: f32 = world[1].floor();
        Self {
            tile_coordinates: TileCoordinates {
                x: tile_x as i32,
                y: tile_y as i32,
            },
            x_offset: world[0] - tile_x,
            y_offset: world[1] - tile_y,
        }
    }

    /// Returns continuous world/tile coordinates for this position.
    pub fn world(self) -> [f32; 2] {
        [
            self.tile_coordinates.x as f32 + self.x_offset,
            self.tile_coordinates.y as f32 + self.y_offset,
        ]
    }

    /// Interpolates from a previous fixed-tick position to this position
    pub(crate) fn interpolated(self, previous: Self, interpolation: f32) -> Self {
        let interpolation: f32 = interpolation.clamp(0.0, 1.0);
        let previous_x: f32 = previous.tile_coordinates.x as f32 + previous.x_offset;
        let previous_y: f32 = previous.tile_coordinates.y as f32 + previous.y_offset;
        let current_x: f32 = self.tile_coordinates.x as f32 + self.x_offset;
        let current_y: f32 = self.tile_coordinates.y as f32 + self.y_offset;
        let interpolated_world_x: f32 = previous_x + (current_x - previous_x) * interpolation;
        let interpolated_world_y: f32 = previous_y + (current_y - previous_y) * interpolation;
        let interpolated_tile_x: f32 = interpolated_world_x.floor();
        let interpolated_tile_y: f32 = interpolated_world_y.floor();
        Self {
            tile_coordinates: TileCoordinates {
                x: interpolated_tile_x as i32,
                y: interpolated_tile_y as i32,
            },
            x_offset: interpolated_world_x - interpolated_tile_x,
            y_offset: interpolated_world_y - interpolated_tile_y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScenePosition;

    #[test]
    fn world_conversion_floors_negative_coordinates() {
        let position = ScenePosition::from_world([-0.25, -1.75]);
        assert_eq!(position.tile_coordinates.x, -1);
        assert_eq!(position.tile_coordinates.y, -2);
        assert_eq!(position.world(), [-0.25, -1.75]);
    }
}
