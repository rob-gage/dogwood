// Copyright Rob Gage 2026

mod collider;
mod collider_shape;
mod collision;
mod collision_body;
mod collision_cellular_tile;
mod collision_cellular_tile_pass;

pub use collider::Collider;
pub use collider_shape::ColliderShape;
pub use collision::Collision;
pub use collision_body::CollisionBody;
pub use collision_cellular_tile::CollisionCellularTile;

/// Maximum distance in tiles between the active region's center and the Rapier origin
const COLLISION_ORIGIN_DISTANCE_MAXIMUM: u64 = 1024;
