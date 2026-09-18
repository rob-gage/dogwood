// Copyright Rob Gage 2026

use super::ActorCollisionShape;

/// Simple configuration for an ordinary dynamic physical actor.
#[derive(Copy, Clone)]
pub struct ActorPhysicalConfiguration {
    pub collision_shape: ActorCollisionShape,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub color: [f32; 4],
}

impl Default for ActorPhysicalConfiguration {
    fn default() -> Self {
        Self {
            collision_shape: ActorCollisionShape::Circle { radius: 0.5 },
            mass: 1.0,
            friction: 0.5,
            restitution: 0.0,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}
