// Copyright Rob Gage 2026

mod actor_cellular_proxy_state;
mod actor_collision_shape;
mod actor_registry;

pub use actor_collision_shape::ActorCollisionShape;
pub use actor_registry::ActorRegistry;
pub(crate) use actor_cellular_proxy_state::ActorCellularProxyState;
pub(crate) use actor_registry::ActorPhysicsProxyState;
