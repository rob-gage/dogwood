// Copyright Rob Gage 2026

//! Scene persistence data and dormant rigid-body records.

#[cfg(test)]
pub(crate) mod tests;

mod scene_data_dormant_rigid;
mod scene_data_dormant_rigid_cell;
mod scene_data_store;

pub(crate) use scene_data_dormant_rigid::{
    SceneDormantRigidBody, append_record, intersects_area, owner_chunk, remove_ids, world_aabb,
};
pub(crate) use scene_data_dormant_rigid_cell::SceneDormantRigidCell;
pub use scene_data_store::SceneData;
