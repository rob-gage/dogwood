// Copyright Rob Gage 2026

use crate::scenes::DormantRigidBody;

pub(super) enum RigidOwnerLoad {
    Loading,
    Ready(Vec<DormantRigidBody>),
    Claiming,
}
