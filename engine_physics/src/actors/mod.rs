// Copyright Rob Gage 2026

mod actor;
mod actor_cellular_proxy_state;
mod actor_collision_shape;
mod actor_control_state;
mod actor_pawn;
mod actor_pawn_flying_configuration;
mod actor_pawn_movement;
mod actor_pawn_noclip_configuration;
mod actor_pawn_swimming_configuration;
mod actor_pawn_swimming_state;
mod actor_pawn_walking_configuration;
mod actor_pawn_walking_state;
mod actor_possessable;
mod actor_previous_position;
mod actor_registry;

pub use actor::Actor;
pub(crate) use actor_cellular_proxy_state::ActorCellularProxyState;
pub use actor_collision_shape::ActorCollisionShape;
pub use actor_control_state::ActorControlState;
pub use actor_pawn::ActorPawn;
pub use actor_pawn_flying_configuration::ActorPawnFlyingConfiguration;
pub use actor_pawn_movement::ActorPawnMovement;
pub use actor_pawn_noclip_configuration::ActorPawnNoclipConfiguration;
pub use actor_pawn_swimming_configuration::ActorPawnSwimmingConfiguration;
pub use actor_pawn_swimming_state::ActorPawnSwimmingState;
pub use actor_pawn_walking_configuration::ActorPawnWalkingConfiguration;
pub use actor_pawn_walking_state::ActorPawnWalkingState;
pub use actor_possessable::ActorPossessable;
pub(crate) use actor_previous_position::ActorPreviousPosition;
pub(crate) use actor_registry::ActorPhysicsProxyState;
pub use actor_registry::ActorRegistry;
