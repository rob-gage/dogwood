// Copyright Rob Gage 2026

pub(super) struct RigidPersistenceRequest {
    pub(super) record: crate::scenes::DormantRigidBody,
    pub(super) slots: Vec<u32>,
}
