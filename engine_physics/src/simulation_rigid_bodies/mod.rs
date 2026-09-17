// Copyright Rob Gage 2026

//! Rigid cellular bodies, state transfer, and granular reaction systems.

mod rigid_cell_state_gather;
mod rigid_cell_state_upload;
mod rigid_cellular_body;
mod rigid_cellular_body_cell;
mod rigid_cellular_body_state;
mod rigid_granular_reaction_batch;
mod rigid_granular_readback_slot;
mod rigid_granular_readback_status;
mod scene_physics_world;

pub(crate) use rigid_cell_state_gather::RigidCellStateGather;
pub(crate) use rigid_cell_state_upload::RigidCellStateUpload;
pub(crate) use rigid_cellular_body::RigidCellularBody;
pub(crate) use rigid_cellular_body_cell::RigidCellularBodyCell;
pub(crate) use rigid_cellular_body_state::RigidCellularBodyState;
pub(crate) use rigid_granular_reaction_batch::RigidGranularReactionBatch;
pub(crate) use rigid_granular_readback_slot::RigidGranularReadbackSlot;
pub(crate) use rigid_granular_readback_status::RigidGranularReadbackStatus;
pub use scene_physics_world::ScenePhysicsWorld;
