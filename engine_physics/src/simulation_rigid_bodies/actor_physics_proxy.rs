// Copyright Rob Gage 2026

use rapier2d::prelude::ColliderHandle;
use rapier2d::prelude::RigidBodyHandle;

use crate::actors::ActorCollisionShape;

pub(super) struct ActorPhysicsProxy {
    pub(super) body: RigidBodyHandle,
    pub(super) collider: ColliderHandle,
    pub(super) shape: ActorCollisionShape,
}
