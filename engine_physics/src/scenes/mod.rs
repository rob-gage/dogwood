// Copyright Rob Gage 2026

//! Public scene facade and scene-facing transfer and persistence types.

#[cfg(test)]
pub(crate) mod tests;

mod scene;
mod scene_actor_contacts;
mod scene_actor_streaming;
mod scene_generator;
mod scene_pending_rigid_dormancy;
mod scene_pending_static_detachment;
mod scene_rigid_body_streaming_response;
mod scene_rigid_cell_removal_cause;
mod scene_rigid_dormancy_batch;
mod scene_rigid_io_job;
mod scene_rigid_owner_load;
mod scene_rigid_persistence_request;

pub use scene::Scene;
pub use scene_generator::SceneGenerator;

pub use crate::scene_data::SceneData;
pub(crate) use crate::scene_data::SceneDormantRigidBody;
pub(crate) use crate::scene_data::SceneDormantRigidCell;
pub(crate) use crate::scene_data::append_record;
pub(crate) use crate::scene_data::intersects_area;
pub(crate) use crate::scene_data::owner_chunk;
pub(crate) use crate::scene_data::remove_ids;
pub(crate) use crate::scene_data::world_aabb;
pub use crate::scene_editing::SceneEdit;
pub use crate::scene_editing::SceneEditBatch;
pub use crate::scene_editing::SceneEditCellPlacement;
pub use crate::scene_geometry::ScenePosition;
pub use crate::scene_geometry::SceneVelocity;
pub use crate::scenes_streaming::FluidDownload;
pub use crate::scenes_streaming::FluidUpload;
pub use crate::scenes_streaming::GasDownload;
pub use crate::scenes_streaming::GasUpload;
pub use crate::scenes_streaming::TileDownload;
pub use crate::scenes_streaming::TileUpload;
