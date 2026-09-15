// Copyright Rob Gage 2026

use rapier2d::prelude::{Pose, SharedShape, Vector};

/// Selects the gravity-relative primitive collision geometry of an actor pawn
#[derive(Copy, Clone)]
pub enum ActorCollisionShape {
    /// A rotation-independent circle
    Circle { radius: f32 },
    /// A capsule whose segment follows gravity-relative up
    Capsule { radius: f32, height: f32 },
    /// A rectangle whose width follows tangent and height follows up
    Rectangle { width: f32, height: f32 },
}

impl ActorCollisionShape {
    pub(crate) fn is_valid(self) -> bool {
        match self {
            Self::Circle { radius } => radius.is_finite() && radius > 0.0,
            Self::Capsule { radius, height } => {
                radius.is_finite() && radius > 0.0 && height.is_finite() && height >= radius * 2.0
            }
            Self::Rectangle { width, height } => {
                width.is_finite() && width > 0.0 && height.is_finite() && height > 0.0
            }
        }
    }

    pub(crate) fn gpu_parameters(self) -> (u32, [f32; 2]) {
        match self {
            Self::Circle { radius } => (0, [radius, 0.0]),
            Self::Capsule { radius, height } => (1, [radius, height * 0.5 - radius]),
            Self::Rectangle { width, height } => (2, [width * 0.5, height * 0.5]),
        }
    }

    pub(crate) fn rapier_shape(self) -> SharedShape {
        match self {
            Self::Circle { radius } => SharedShape::ball(radius),
            Self::Capsule { radius, height } => {
                SharedShape::capsule_y(height * 0.5 - radius, radius)
            }
            Self::Rectangle { width, height } => SharedShape::cuboid(width * 0.5, height * 0.5),
        }
    }

    pub(crate) fn pose(self, center: Vector, up: Vector) -> Pose {
        let angle = match self {
            Self::Circle { .. } => 0.0,
            _ => (-up.x).atan2(up.y),
        };
        Pose::new(center, angle)
    }

    /// Returns nominal width and height in tiles for simple actor rendering
    pub const fn nominal_dimensions(self) -> [f32; 2] {
        match self {
            Self::Circle { radius } => [radius * 2.0, radius * 2.0],
            Self::Capsule { radius, height } => [radius * 2.0, height],
            Self::Rectangle { width, height } => [width, height],
        }
    }
}
