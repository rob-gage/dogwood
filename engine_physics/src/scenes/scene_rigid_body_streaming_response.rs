// Copyright Rob Gage 2026

use std::io;

use super::scene_rigid_persistence_request::SceneRigidPersistenceRequest;
use crate::scenes::SceneDormantRigidBody;
use crate::tiles::TileCoordinates;

pub(super) enum SceneRigidBodyStreamingResponse {
    Loaded {
        owner: TileCoordinates,
        generation: u64,
        result: Result<Vec<SceneDormantRigidBody>, io::Error>,
    },
    Saved {
        request: SceneRigidPersistenceRequest,
        result: Result<(), io::Error>,
    },
    Claimed {
        owner: TileCoordinates,
        original: Vec<SceneDormantRigidBody>,
        restored_ids: Vec<u64>,
        result: Result<Vec<SceneDormantRigidBody>, io::Error>,
    },
}
