// Copyright Rob Gage 2026

use std::sync::Arc;
use std::sync::Mutex;

use super::collision_readback_status::CollisionReadbackStatus;

/// One reusable collision staging buffer and its asynchronous mapping state
pub struct CollisionReadbackSlot {
    /// The Accelerator-to-CPU staging buffer
    pub buffer: wgpu::Buffer,
    /// State shared with the asynchronous mapping callback
    pub status: Arc<Mutex<CollisionReadbackStatus>>,
}
