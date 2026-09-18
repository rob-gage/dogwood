// Copyright Rob Gage 2026

use std::sync::mpsc::Receiver;

use super::scene_pending_rigid_dormancy::ScenePendingRigidDormancy;

pub(super) struct SceneRigidDormancyBatch {
    pub(super) bodies: Vec<ScenePendingRigidDormancy>,
    pub(super) readback_slot: usize,
    pub(super) state_count: usize,
    pub(super) result: Receiver<Result<(), wgpu::BufferAsyncError>>,
}
