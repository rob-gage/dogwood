// Copyright Rob Gage 2026

use super::collider_shape::ColliderShape;

/// Collision geometry placed relative to the position of its owner
#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    /// Geometry of the collider
    pub shape: ColliderShape,
    /// Horizontal offset of the shape's center from its owner in tiles
    pub offset_x: f32,
    /// Vertical offset of the shape's center from its owner in tiles
    pub offset_y: f32,
    /// Counterclockwise rotation of the shape around its center in radians
    pub rotation: f32,
}
