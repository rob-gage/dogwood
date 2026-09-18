use super::ActorPhysicalConfiguration;
use crate::scenes::ScenePosition;
use crate::scenes::SceneVelocity;

/// A generator-provided initial generic physical actor.
#[derive(Copy, Clone)]
pub struct ActorPhysicalSpawn {
    pub configuration: ActorPhysicalConfiguration,
    pub position: ScenePosition,
    pub velocity: SceneVelocity,
}
