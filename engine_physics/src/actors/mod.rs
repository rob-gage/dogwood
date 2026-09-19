// Copyright Rob Gage 2026

//! Actor identifiers, ECS components, and pawn configuration types.

mod actor;
mod actor_contact;
mod actor_control_state;
mod actor_pawn;
mod actor_pawn_flying_configuration;
mod actor_pawn_movement;
mod actor_pawn_noclip_configuration;
mod actor_pawn_swimming_configuration;
mod actor_pawn_swimming_state;
mod actor_pawn_walking_configuration;
mod actor_pawn_walking_state;
mod actor_physical_configuration;
mod actor_physical_snapshot;
mod actor_physical_spawn;
mod actor_possessable;
mod actor_previous_position;
mod actor_sprite_animation;
mod actor_sprite_sheet;
mod actor_sprites;

pub use actor::Actor;
pub use actor_contact::ActorContactEvent;
pub use actor_contact::ActorContactState;
pub use actor_control_state::ActorControlState;
pub use actor_pawn::ActorPawn;
pub use actor_pawn_flying_configuration::ActorPawnFlyingConfiguration;
pub use actor_pawn_movement::ActorPawnMovement;
pub use actor_pawn_noclip_configuration::ActorPawnNoclipConfiguration;
pub use actor_pawn_swimming_configuration::ActorPawnSwimmingConfiguration;
pub use actor_pawn_swimming_state::ActorPawnSwimmingState;
pub use actor_pawn_walking_configuration::ActorPawnWalkingConfiguration;
pub use actor_pawn_walking_state::ActorPawnWalkingState;
pub use actor_physical_configuration::ActorPhysicalConfiguration;
pub use actor_physical_snapshot::ActorPhysicalSnapshot;
pub use actor_physical_spawn::ActorPhysicalSpawn;
pub use actor_possessable::ActorPossessable;
pub(crate) use actor_previous_position::ActorPreviousPosition;
pub use actor_sprite_animation::{ActorSpriteAnimation, ActorSpriteAnimationIdentifier};
pub use actor_sprite_sheet::ActorSpriteSheet;
pub use actor_sprites::ActorSprites;

pub(crate) use crate::actors_utility::ActorCellularProxyState;
pub use crate::actors_utility::ActorCollisionShape;
pub(crate) use crate::actors_utility::ActorPhysicsProxyState;
pub use crate::actors_utility::ActorRegistry;
