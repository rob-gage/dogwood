// Copyright Rob Gage 2026

//! Public scene facade and scene-facing transfer and persistence types.

mod scene;
mod scene_generator;

pub use crate::scene_editing::{SceneEdit, SceneEditBatch, SceneEditCellPlacement};
pub use crate::scene_geometry::{ScenePosition, SceneVelocity};
pub use crate::scene_data::SceneData;
pub(crate) use crate::scene_data::{
    DormantRigidBody, DormantRigidCell, append_record, intersects_area, owner_chunk, remove_ids,
    world_aabb,
};
pub use crate::scene_streaming::{
    FluidDownload, FluidUpload, GasDownload, GasUpload, TileDownload, TileUpload,
};
pub use scene::Scene;
pub use scene_generator::SceneGenerator;
