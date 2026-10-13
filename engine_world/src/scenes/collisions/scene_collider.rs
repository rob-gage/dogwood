// Copyright Rob Gage 2026

use super::scene_collider_shape::SceneColliderShape;

/// Collision geometry placed relative to the position of its owner
#[derive(Clone, Debug, PartialEq)]
pub struct SceneCollider {
    /// Geometry of the collider
    pub shape: SceneColliderShape,
    /// Horizontal offset of the shape's center from its owner in tiles
    pub offset_x: f32,
    /// Vertical offset of the shape's center from its owner in tiles
    pub offset_y: f32,
    /// Counterclockwise rotation of the shape around its center in radians
    pub rotation: f32,
}
