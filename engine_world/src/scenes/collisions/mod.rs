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

/// Collision group of actor colliders, reserved by the engine
pub const COLLISION_GROUP_ACTOR: u32 = 1 << 28;

/// Collision group of the solid cells of the active cellular tiles, reserved by the engine
pub const COLLISION_GROUP_CELLULAR_TILE: u32 = 1 << 29;

/// Collision group of cellular rigid bodies, reserved by the engine
pub const COLLISION_GROUP_RIGID_BODY: u32 = 1 << 30;

/// Collision group of colliders whose cells `EditMode::Insert` skips, reserved by the engine
pub const COLLISION_GROUP_EDIT: u32 = 1 << 31;
