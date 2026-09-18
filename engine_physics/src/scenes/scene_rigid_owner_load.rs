// Copyright Rob Gage 2026

use crate::scenes::SceneDormantRigidBody;

pub(super) enum SceneRigidOwnerLoad {
    Loading,
    Ready(Vec<SceneDormantRigidBody>),
    Claiming,
}
