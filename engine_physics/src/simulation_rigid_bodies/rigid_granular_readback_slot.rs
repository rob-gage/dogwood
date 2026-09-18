// Copyright Rob Gage 2026

use std::sync::Arc;
use std::sync::Mutex;

use super::rigid_granular_readback_status::RigidGranularReadbackStatus;

/// One reusable staging allocation for ordered rigid/granular reactions
pub(crate) struct RigidGranularReadbackSlot {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) status: Arc<Mutex<RigidGranularReadbackStatus>>,
}
