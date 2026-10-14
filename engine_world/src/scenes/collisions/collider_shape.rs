// Copyright Rob Gage 2026

/// Geometry of a `Collider` centered on its offset, measured in tiles
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ColliderShape {
    /// A circle
    Circle {
        /// Radius of the circle
        radius: f32,
    },
    /// An axis-aligned rectangle before rotation
    Rectangle {
        /// Width of the rectangle
        width: f32,
        /// Height of the rectangle
        height: f32,
    },
    /// A vertical capsule before rotation, with semicircular ends whose diameter is its width
    Capsule {
        /// Width of the capsule, no greater than its height
        width: f32,
        /// Height of the capsule including both ends
        height: f32,
    },
}
