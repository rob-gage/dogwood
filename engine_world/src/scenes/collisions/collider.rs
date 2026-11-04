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

impl Collider {

    /// Returns this collider on an owner turned counterclockwise by a rotation in radians around
    /// its position
    pub fn rotated(&self, rotation: f32) -> Self {
        if rotation == 0.0 { return self.clone(); }
        let (sine, cosine): (f32, f32) = rotation.sin_cos();
        Self {
            shape: self.shape,
            offset_x: cosine * self.offset_x - sine * self.offset_y,
            offset_y: sine * self.offset_x + cosine * self.offset_y,
            rotation: self.rotation + rotation,
        }
    }

}
