// Copyright Rob Gage 2026

use super::scene_rigid_persistence_request::SceneRigidPersistenceRequest;
use crate::scenes::SceneDormantRigidBody;
use crate::tiles::TileCoordinates;

pub(super) enum SceneRigidIoJob {
    Persist(SceneRigidPersistenceRequest),
    Claim {
        owner: TileCoordinates,
        original: Vec<SceneDormantRigidBody>,
        restored_ids: Vec<u64>,
    },
}
