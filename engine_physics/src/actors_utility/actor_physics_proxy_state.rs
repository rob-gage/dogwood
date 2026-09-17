// Copyright Rob Gage 2026

use super::ActorCollisionShape;
use crate::actors::Actor;

/// Transient actor state exposed to physics proxy construction
pub(crate) struct ActorPhysicsProxyState {
    pub actor: Actor,
    pub center: [f32; 2],
    pub shape: ActorCollisionShape,
}
