// Copyright Rob Gage 2026

use super::scene_pending_rigid_dormancy::ScenePendingRigidDormancy;
use std::sync::mpsc::Receiver;

pub(super) struct SceneRigidDormancyBatch {
    pub(super) bodies: Vec<ScenePendingRigidDormancy>,
    pub(super) readback_slot: usize,
    pub(super) state_count: usize,
    pub(super) result: Receiver<Result<(), wgpu::BufferAsyncError>>,
}
