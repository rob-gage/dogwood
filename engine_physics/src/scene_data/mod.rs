// Copyright Rob Gage 2026

mod scene_data;
mod scene_data_dormant_rigid;

pub use scene_data::SceneData;
pub(crate) use scene_data_dormant_rigid::{
    DormantRigidBody, DormantRigidCell, append_record, intersects_area, owner_chunk, remove_ids,
    world_aabb,
};
