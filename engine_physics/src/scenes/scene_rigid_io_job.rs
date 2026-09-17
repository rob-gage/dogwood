// Copyright Rob Gage 2026

use super::scene_rigid_persistence_request::RigidPersistenceRequest;
use crate::scenes::DormantRigidBody;
use crate::tiles::TileCoordinates;

pub(super) enum RigidIoJob {
    Persist(RigidPersistenceRequest),
    Claim {
        owner: TileCoordinates,
        original: Vec<DormantRigidBody>,
        restored_ids: Vec<u64>,
    },
}
