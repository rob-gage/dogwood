// Copyright Rob Gage 2026

//! Cellular collision, pressure, occupancy, and body-proxy systems.

mod cell_particle;
mod cellular_collision;
mod cellular_dynamic;
mod cellular_physics_body_proxy;
mod cellular_pressure;
mod cellular_static_state;
mod cellular_static_state_gather;
mod collision_occupancy_snapshot;
mod collision_readback_slot;
mod collision_readback_status;

#[cfg(test)]
pub(crate) mod tests;

pub use cell_particle::CellParticle;
pub use cellular_collision::CellularCollision;
pub use cellular_dynamic::CellularDynamic;
pub use cellular_physics_body_proxy::CellularPhysicsBodyProxy;
pub use cellular_pressure::CellularPressure;
pub(crate) use cellular_static_state::CellularStaticState;
pub(crate) use cellular_static_state_gather::CellularStaticStateGather;
pub use collision_occupancy_snapshot::CollisionOccupancySnapshot;
pub(crate) use collision_readback_slot::CollisionReadbackSlot;
pub(crate) use collision_readback_status::CollisionReadbackStatus;
