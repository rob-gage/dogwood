// Copyright Rob Gage 2026

mod collider;
mod collider_shape;
mod collision;
mod collision_body;
mod collision_cellular_tile;
mod collision_cellular_tile_pass;
mod collision_rigid_body;
mod collision_rigid_body_pass;

pub use collider::Collider;
pub use collider_shape::ColliderShape;
pub use collision::Collision;
pub use collision_body::CollisionBody;
pub use collision_cellular_tile::CollisionCellularTile;
pub use collision_rigid_body_pass::CollisionRigidBodyPass;

/// Maximum distance in tiles between the active region's center and the Rapier origin
const COLLISION_ORIGIN_DISTANCE_MAXIMUM: u64 = 1024;

/// Constraint passes per solver substep, keeping stacked bodies from outrunning floor contacts
const COLLISION_CONTACT_ITERATION_COUNT: usize = 4;

/// Natural frequency in hertz of fixed contacts, resisting concentrated loads on voxel surfaces
const COLLISION_STATIC_CONTACT_FREQUENCY: f32 = 600.0;

/// Fewest cells of a rigid body, below which its cells become debris
const COLLISION_RIGID_BODY_CELL_COUNT_MINIMUM: usize = 12;

/// Most cells of floating static cells that form a rigid body, above which they stay in place
const COLLISION_RIGID_BODY_CELL_COUNT_MAXIMUM: usize = 4096;

/// Distance in tiles ahead of a moving rigid body within which contacts are predicted, since
/// voxel contacts cannot push a body out once it moves a cell or more into other voxels
const COLLISION_RIGID_BODY_CCD_PREDICTION: f32 = 1.0;

/// Additional solver substeps for contact islands containing cellular rigid bodies
const COLLISION_RIGID_BODY_SOLVER_ITERATION_COUNT: usize = 8;

/// Passes stopping a rigid body against the dynamic cells holding it on both sides
const COLLISION_RIGID_BODY_HOLDING_ITERATION_COUNT: usize = 4;

/// Maximum number of rigid bodies simulated at once
pub const COLLISION_RIGID_BODY_COUNT_MAXIMUM: usize = 1024;

/// Maximum number of tiles of cells held by every simulated rigid body
const COLLISION_RIGID_BODY_CELLULAR_TILE_COUNT_MAXIMUM: usize = 16384;

/// Collision group of settled dynamic cells, reserved by the engine
pub const COLLISION_GROUP_CELLULAR_DYNAMIC: u32 = 1 << 27;

/// Collision group of actor colliders, reserved by the engine
pub const COLLISION_GROUP_ACTOR: u32 = 1 << 28;

/// Collision group of the solid cells of the active cellular tiles, reserved by the engine
pub const COLLISION_GROUP_CELLULAR_TILE: u32 = 1 << 29;

/// Collision group of cellular rigid bodies, reserved by the engine
pub const COLLISION_GROUP_RIGID_BODY: u32 = 1 << 30;

/// Collision group of colliders whose cells `EditMode::Insert` skips, reserved by the engine
pub const COLLISION_GROUP_EDIT: u32 = 1 << 31;
