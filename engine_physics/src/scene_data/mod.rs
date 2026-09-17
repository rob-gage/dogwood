// Copyright Rob Gage 2026

//! Scene persistence data and dormant rigid-body records.

mod scene_data_dormant_rigid;
mod scene_data_store;

pub(crate) use scene_data_dormant_rigid::{
    DormantRigidBody, DormantRigidCell, append_record, intersects_area, owner_chunk, remove_ids,
    world_aabb,
};
pub use scene_data_store::SceneData;
