// Copyright Rob Gage 2026

use rapier2d::prelude::ColliderHandle;

pub(super) struct TerrainPatch {
    pub(super) collider: Option<ColliderHandle>,
    pub(super) masks: [[u32; 2]; 16],
    pub(super) last_required_tick: u64,
}
