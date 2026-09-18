// Copyright Rob Gage 2026

//! Scene persistence data and dormant rigid-body records.

#[cfg(test)]
pub(crate) mod tests;

mod scene_data_dormant_rigid;
mod scene_data_dormant_rigid_cell;
mod scene_data_store;

pub(crate) use scene_data_dormant_rigid::SceneDormantRigidBody;
pub(crate) use scene_data_dormant_rigid::append_record;
pub(crate) use scene_data_dormant_rigid::intersects_area;
pub(crate) use scene_data_dormant_rigid::owner_chunk;
pub(crate) use scene_data_dormant_rigid::remove_ids;
pub(crate) use scene_data_dormant_rigid::world_aabb;
pub(crate) use scene_data_dormant_rigid_cell::SceneDormantRigidCell;
pub use scene_data_store::SceneData;
