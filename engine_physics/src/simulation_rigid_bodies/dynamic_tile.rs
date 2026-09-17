// Copyright Rob Gage 2026

use rapier2d::prelude::ColliderHandle;

pub(super) struct DynamicTile {
    pub(super) collider: Option<ColliderHandle>,
    pub(super) mask: [u32; 2],
    pub(super) last_required_tick: u64,
}
