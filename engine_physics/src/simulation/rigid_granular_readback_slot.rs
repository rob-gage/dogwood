// Copyright Rob Gage 2026

use super::rigid_granular_readback_status::RigidGranularReadbackStatus;
use std::sync::{
    Arc,
    Mutex,
};

/// One reusable staging allocation for ordered rigid/granular reactions
pub(crate) struct RigidGranularReadbackSlot {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) status: Arc<Mutex<RigidGranularReadbackStatus>>,
}
