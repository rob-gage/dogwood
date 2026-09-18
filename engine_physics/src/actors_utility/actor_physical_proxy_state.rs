use super::ActorCollisionShape;
use crate::actors::Actor;

pub(crate) struct ActorPhysicalProxyState {
    pub actor: Actor,
    pub center: [f32; 2],
    pub velocity: [f32; 2],
    pub shape: ActorCollisionShape,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
}
