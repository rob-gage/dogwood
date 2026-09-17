// Copyright Rob Gage 2026

#[cfg(test)]
pub(crate) mod tests;

pub use crate::scene_editing::{SceneEdit, SceneEditBatch, SceneEditCellPlacement};
pub use crate::scene_generation::SceneGenerator;
pub use crate::scene_geometry::{ScenePosition, SceneVelocity};
pub use crate::scene_persistence::SceneData;
pub(crate) use crate::scene_persistence::{
    DormantRigidBody, DormantRigidCell, append_record, intersects_area, owner_chunk, remove_ids,
    world_aabb,
};
pub use crate::scene_runtime::Scene;
pub use crate::scene_streaming::{
    FluidDownload, FluidUpload, GasDownload, GasUpload, TileDownload, TileUpload,
};
