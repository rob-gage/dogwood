// Copyright Rob Gage 2026

//! Public scene facade and scene-facing transfer and persistence types.

#[cfg(test)]
pub(crate) mod tests;

mod scene;
mod scene_generator;
mod scene_pending_rigid_dormancy;
mod scene_pending_static_detachment;
mod scene_rigid_body_streaming_response;
mod scene_rigid_cell_removal_cause;
mod scene_rigid_dormancy_batch;
mod scene_rigid_io_job;
mod scene_rigid_owner_load;
mod scene_rigid_persistence_request;

pub use crate::scene_data::SceneData;
pub(crate) use crate::scene_data::{
    SceneDormantRigidBody, SceneDormantRigidCell, append_record, intersects_area, owner_chunk,
    remove_ids, world_aabb,
};
pub use crate::scene_editing::{SceneEdit, SceneEditBatch, SceneEditCellPlacement};
pub use crate::scene_geometry::{ScenePosition, SceneVelocity};
pub use crate::scenes_streaming::{
    FluidDownload, FluidUpload, GasDownload, GasUpload, TileDownload, TileUpload,
};
pub use scene::Scene;
pub use scene_generator::SceneGenerator;
