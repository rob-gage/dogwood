// Copyright Rob Gage 2026

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TerrainBridgeStatistics {
    pub(crate) active_patches: usize,
    pub(crate) collider_patches: usize,
    pub(crate) patch_rebuilds: u64,
    pub(crate) patch_cells_scanned: u64,
    pub(crate) dynamic_required_tiles: usize,
    pub(crate) dynamic_cached_tiles: usize,
    pub(crate) dynamic_collider_tiles: usize,
    pub(crate) dynamic_mask_changes: u64,
    pub(crate) dynamic_shape_rebuilds: u64,
    pub(crate) dynamic_set_shape_calls: u64,
    pub(crate) dynamic_enable_disable_changes: u64,
    pub(crate) dynamic_cells_scanned: u64,
    pub(crate) dynamic_rectangles_emitted: u64,
    pub(crate) collision_snapshot_age: u64,
}
