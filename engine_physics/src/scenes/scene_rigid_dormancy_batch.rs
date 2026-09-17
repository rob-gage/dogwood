// Copyright Rob Gage 2026

use super::scene_pending_rigid_dormancy::PendingRigidDormancy;
use std::sync::mpsc::Receiver;

pub(super) struct RigidDormancyBatch {
    pub(super) bodies: Vec<PendingRigidDormancy>,
    pub(super) readback_slot: usize,
    pub(super) state_count: usize,
    pub(super) result: Receiver<Result<(), wgpu::BufferAsyncError>>,
}
