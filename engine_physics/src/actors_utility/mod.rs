// Copyright Rob Gage 2026

//! Actor registry and transient collision/proxy infrastructure.

mod actor_cellular_proxy_state;
mod actor_collision_shape;
mod actor_physical;
mod actor_physical_proxy_state;
mod actor_physics_proxy_state;
mod actor_registry;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use actor_cellular_proxy_state::ActorCellularProxyState;
pub use actor_collision_shape::ActorCollisionShape;
pub(crate) use actor_physical_proxy_state::ActorPhysicalProxyState;
pub(crate) use actor_physics_proxy_state::ActorPhysicsProxyState;
pub use actor_registry::ActorRegistry;
