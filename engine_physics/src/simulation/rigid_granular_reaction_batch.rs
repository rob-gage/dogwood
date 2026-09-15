// Copyright Rob Gage 2026

/// One asynchronously completed rigid/granular reaction batch
pub(crate) struct RigidGranularReactionBatch {
    pub(crate) sequence: u64,
    pub(crate) topology_revision: u64,
    pub(crate) body_count: usize,
    pub(crate) reactions: Box<[[f32; 3]]>,
    pub(crate) contact_counts: Box<[u32]>,
}
