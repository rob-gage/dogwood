// Copyright Rob Gage 2026

use super::scene_rigid_persistence_request::RigidPersistenceRequest;
use crate::scenes::DormantRigidBody;
use crate::tiles::TileCoordinates;
use std::io;

pub(super) enum RigidStreamingResponse {
    Loaded {
        owner: TileCoordinates,
        generation: u64,
        result: Result<Vec<DormantRigidBody>, io::Error>,
    },
    Saved {
        request: RigidPersistenceRequest,
        result: Result<(), io::Error>,
    },
    Claimed {
        owner: TileCoordinates,
        original: Vec<DormantRigidBody>,
        restored_ids: Vec<u64>,
        result: Result<Vec<DormantRigidBody>, io::Error>,
    },
}
