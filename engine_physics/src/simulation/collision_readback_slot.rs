// Copyright Rob Gage 2026

use super::collision_readback_status::CollisionReadbackStatus;
use std::sync::{
    Arc,
    Mutex,
};

/// One reusable collision staging buffer and its asynchronous mapping state
pub struct CollisionReadbackSlot {
    /// The GPU-to-CPU staging buffer
    pub buffer: wgpu::Buffer,
    /// State shared with the asynchronous mapping callback
    pub status: Arc<Mutex<CollisionReadbackStatus>>,
}
