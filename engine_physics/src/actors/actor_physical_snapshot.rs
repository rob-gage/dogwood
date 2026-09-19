use super::Actor;
use super::ActorPhysicalConfiguration;
use crate::scenes::ScenePosition;
use crate::scenes::SceneVelocity;

/// Engine-owned state retained while a generic physical actor is not loaded.
use super::ActorSprites;

#[derive(Clone)]
pub struct ActorPhysicalSnapshot {
    pub actor: Actor,
    pub position: ScenePosition,
    pub velocity: SceneVelocity,
    pub configuration: ActorPhysicalConfiguration,
    pub sprites: Option<ActorSprites>,
}
