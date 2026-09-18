// Copyright Rob Gage 2026

pub(super) struct SceneRigidPersistenceRequest {
    pub(super) record: crate::scenes::SceneDormantRigidBody,
    pub(super) slots: Vec<u32>,
}
