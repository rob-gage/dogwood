// Copyright Rob Gage 2026

mod scene_collider;
mod scene_collider_shape;
mod scene_collision;
mod scene_collision_body;
mod scene_collision_cellular_tile;
mod scene_collision_cellular_tile_pass;

pub use scene_collider::SceneCollider;
pub use scene_collider_shape::SceneColliderShape;
pub use scene_collision::SceneCollision;
pub use scene_collision_body::SceneCollisionBody;
pub use scene_collision_cellular_tile::SceneCollisionCellularTile;

/// Maximum distance in tiles between the active region's center and the Rapier origin
const SCENE_COLLISION_ORIGIN_DISTANCE_MAXIMUM: u64 = 1024;
