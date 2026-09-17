// Copyright Rob Gage 2026

use crate::actors::ActorCollisionShape;
use rapier2d::prelude::{ColliderHandle, RigidBodyHandle};

pub(super) struct ActorPhysicsProxy {
    pub(super) body: RigidBodyHandle,
    pub(super) collider: ColliderHandle,
    pub(super) shape: ActorCollisionShape,
}
